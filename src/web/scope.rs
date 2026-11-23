//! The scope of a page, carried in the URL as `?p=&w=` (ADR 0004) so the same
//! URL shows the same view after a reload. Parsing is pure; validation reads
//! the Project list from the database. Nothing here reads the process cwd.
//!
//! | query                | scope                                      |
//! |----------------------|--------------------------------------------|
//! | none, or `p=@all`    | every Project                              |
//! | `p=KEY`              | one Project, every Worktree                |
//! | `p=KEY&w=@all`       | the same                                   |
//! | `p=KEY&w=@root`      | the root checkout of the Project           |
//! | `p=KEY&w=<path>`     | one Worktree, path as stored               |
//! | `p=` (empty)         | "No Project": the unscoped LOCAL Project   |
//!
//! `@all` exists so the picker's selects always send a value; `@root` because
//! stored Worktree paths are absolute and never start with `@`. Anything else
//! (unknown key or path, `w` without `p`, `w` with `p=` or `p=@all`, a literal
//! `p=LOCAL`, a repeated `p` or `w`) is a 404.

use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;

use super::AppState;
use super::error::WebError;
use super::state::blocking;
use crate::error::{Error, Result};
use crate::model::ScopeOverview;
use crate::project;
use crate::ticket::ScopeFilter;

pub const ALL: &str = "@all";
pub const ROOT: &str = "@root";
/// The key of the unscoped Project, which `p=` (empty) names.
const LOCAL: &str = "LOCAL";

/// A validated scope with the overview the picker renders from.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    pub filter: ScopeFilter,
    pub overview: ScopeOverview,
}

fn bad(what: &str) -> Error {
    Error::NotFound(format!("not a scope: {what}"))
}

/// The `p` and `w` values of a query to a [`ScopeFilter`], without looking at
/// the database. `None` is an absent parameter.
pub fn parse(p: Option<&str>, w: Option<&str>) -> Result<ScopeFilter> {
    match (p, w) {
        (None | Some(ALL), None) => Ok(ScopeFilter::All),
        (None, Some(_)) => Err(bad("w needs p")),
        (Some(ALL), Some(_)) => Err(bad("w with all Projects")),
        (Some(""), None) => Ok(ScopeFilter::Project { key: LOCAL.into() }),
        (Some(""), Some(_)) => Err(bad("w with No Project")),
        (Some(LOCAL), _) => Err(bad("LOCAL is named by an empty p")),
        (Some(key), None | Some(ALL)) => Ok(ScopeFilter::Project { key: key.into() }),
        (Some(key), Some(ROOT)) => Ok(ScopeFilter::Worktree {
            key: key.into(),
            path: None,
        }),
        (Some(key), Some(path)) => Ok(ScopeFilter::Worktree {
            key: key.into(),
            path: Some(path.into()),
        }),
    }
}

/// The inverse of [`parse`]: the query string (no leading `?`, empty for all
/// Projects) that names `filter`, percent-encoded.
pub fn query(filter: &ScopeFilter) -> String {
    let mut s = form_urlencoded::Serializer::new(String::new());
    match filter {
        ScopeFilter::All => {}
        ScopeFilter::Project { key } if key == LOCAL => {
            s.append_pair("p", "");
        }
        ScopeFilter::Project { key } => {
            s.append_pair("p", key);
        }
        ScopeFilter::Worktree { key, path } => {
            s.append_pair("p", key);
            s.append_pair("w", path.as_deref().unwrap_or(ROOT));
        }
    }
    s.finish()
}

/// Check `filter` against the Projects and Worktrees `list_with_counts`
/// reports (the same list the picker shows).
pub fn resolve(conn: &rusqlite::Connection, filter: ScopeFilter) -> Result<Scope> {
    let overview = project::list_with_counts(conn)?;
    match &filter {
        ScopeFilter::All => {}
        ScopeFilter::Project { key } if key == LOCAL => {}
        ScopeFilter::Project { key } => {
            if !overview.projects.iter().any(|p| &p.key == key) {
                return Err(Error::NotFound(format!("no Project {key}")));
            }
        }
        ScopeFilter::Worktree { key, path } => {
            let p = overview
                .projects
                .iter()
                .find(|p| &p.key == key)
                .ok_or_else(|| Error::NotFound(format!("no Project {key}")))?;
            if let Some(path) = path
                && !p.worktrees.iter().any(|w| &w.path == path)
            {
                return Err(Error::NotFound(format!("no Worktree {path} in {key}")));
            }
        }
    }
    Ok(Scope { filter, overview })
}

/// Axum extractor: the page's [`Scope`] from the query string. A bad scope
/// rejects with a 404 whose fragment is `err-scope`.
pub struct Scoped(pub Scope);

impl FromRequestParts<AppState> for Scoped {
    type Rejection = WebError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> std::result::Result<Self, WebError> {
        let reject = |e| WebError::new(e, "scope");
        let Query(pairs) = Query::<Vec<(String, String)>>::try_from_uri(&parts.uri)
            .map_err(|e| reject(Error::Usage(e.to_string())))?;
        let get = |name: &str| -> Result<Option<&str>> {
            let mut found = pairs.iter().filter(|(k, _)| k == name);
            let first = found.next().map(|(_, v)| v.as_str());
            if found.next().is_some() {
                return Err(bad(&format!("{name} given twice")));
            }
            Ok(first)
        };
        let filter = get("p")
            .and_then(|p| Ok((p, get("w")?)))
            .and_then(|(p, w)| parse(p, w))
            .map_err(reject)?;
        let db = state.db.clone();
        blocking(move || resolve(&crate::db::open(&db)?, filter))
            .await
            .map(Scoped)
            .map_err(reject)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(key: &str) -> ScopeFilter {
        ScopeFilter::Project { key: key.into() }
    }

    #[test]
    fn parse_table() {
        assert_eq!(parse(None, None).unwrap(), ScopeFilter::All);
        assert_eq!(parse(Some(ALL), None).unwrap(), ScopeFilter::All);
        assert_eq!(parse(Some(""), None).unwrap(), p("LOCAL"));
        assert_eq!(parse(Some("SCR"), None).unwrap(), p("SCR"));
        assert_eq!(parse(Some("SCR"), Some(ALL)).unwrap(), p("SCR"));
        assert_eq!(
            parse(Some("SCR"), Some(ROOT)).unwrap(),
            ScopeFilter::Worktree {
                key: "SCR".into(),
                path: None
            }
        );
        assert_eq!(
            parse(Some("SCR"), Some("/w/feat")).unwrap(),
            ScopeFilter::Worktree {
                key: "SCR".into(),
                path: Some("/w/feat".into())
            }
        );
        for (pp, ww) in [
            (None, Some("/w")),
            (None, Some(ROOT)),
            (Some(ALL), Some("/w")),
            (Some(""), Some("/w")),
            (Some(""), Some(ROOT)),
            (Some("LOCAL"), None),
            (Some("LOCAL"), Some("/w")),
        ] {
            assert!(
                matches!(parse(pp, ww), Err(Error::NotFound(_))),
                "{pp:?} {ww:?}"
            );
        }
    }

    #[test]
    fn query_round_trips_through_parse() {
        let filters = [
            ScopeFilter::All,
            p("LOCAL"),
            p("SCR"),
            ScopeFilter::Worktree {
                key: "SCR".into(),
                path: None,
            },
            ScopeFilter::Worktree {
                key: "SCR".into(),
                path: Some("/w/a b&c=d/é".into()),
            },
        ];
        for f in filters {
            let q = query(&f);
            let pairs: Vec<(String, String)> =
                form_urlencoded::parse(q.as_bytes()).into_owned().collect();
            let get = |n: &str| pairs.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
            assert_eq!(parse(get("p"), get("w")).unwrap(), f, "{q}");
        }
        assert_eq!(query(&ScopeFilter::All), "");
        assert_eq!(query(&p("LOCAL")), "p=");
    }

    #[test]
    fn resolve_checks_the_overview() {
        let conn = crate::db::open(std::path::Path::new(":memory:")).unwrap();
        assert!(resolve(&conn, ScopeFilter::All).is_ok());
        assert!(resolve(&conn, p("LOCAL")).is_ok());
        assert!(matches!(resolve(&conn, p("NOPE")), Err(Error::NotFound(_))));
    }
}
