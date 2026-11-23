//! The Graph (`/graph`): the dependency graph of the scope as a layered SVG
//! (`view::graph_svg`), with the Critical path and the tickets that block the
//! most others as plain lists beside it. The data is `graph::build`, the same
//! as `scree graph`; by default it covers draft, ready and active tickets,
//! and `all=1` adds done and abandoned ones.
//!
//! Past [`NODE_CAP`] nodes the page draws nothing and asks the reader to
//! narrow the scope: a layout with no crossing minimisation is not readable
//! there. The page and the live stream share [`region`].

use std::path::Path as FsPath;

use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use clap::ValueEnum;
use maud::{Markup, html};
use rusqlite::Connection;

use crate::cli::{ForWho, ListArgs, Status};
use crate::error::{Error, Result};
use crate::graph::{self, Graph};
use crate::identity::{Identity, Role};
use crate::ticket::{self, ScopeFilter};
use crate::web::AppState;
use crate::web::error::{LoggedExt, WebError, unreadable};
use crate::web::scope::{self, Scope, Scoped};
use crate::web::state::blocking;
use crate::web::view::graph_svg::{self, State as NodeState};
use crate::web::view::layout::{self, PageCtx, Route};
use crate::web::view::{self, components, picker};

/// The most nodes the page draws.
pub const NODE_CAP: usize = 150;

/// The parts of a Graph URL beyond the scope.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct GraphQuery {
    pub for_who: Option<ForWho>,
    /// Also include done and abandoned tickets.
    pub all: bool,
}

fn usage(msg: impl Into<String>) -> Error {
    Error::Usage(msg.into())
}

impl GraphQuery {
    /// Read `for` and `all`; `p`, `w` and anything unknown are ignored. A
    /// repeated or bad value is [`Error::Usage`].
    pub fn parse(pairs: &[(String, String)]) -> Result<GraphQuery> {
        let one = |name: &str| -> Result<Option<&str>> {
            let mut found = pairs.iter().filter(|(k, _)| k == name);
            let first = found.next().map(|(_, v)| v.as_str());
            if found.next().is_some() {
                return Err(usage(format!("{name} given twice")));
            }
            Ok(first.map(str::trim).filter(|v| !v.is_empty()))
        };
        let for_who = one("for")?
            .map(|v| ForWho::from_str(v, true).map_err(|_| usage(format!("unknown for {v:?}"))))
            .transpose()?;
        let all = match one("all")? {
            None | Some("0") => false,
            Some("1") => true,
            Some(v) => return Err(usage(format!("all must be 1 or 0, not {v:?}"))),
        };
        Ok(GraphQuery { for_who, all })
    }

    fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        if let Some(f) = self.for_who {
            v.push(("for", f.as_str().to_string()));
        }
        if self.all {
            v.push(("all", "1".into()));
        }
        v
    }

    fn statuses(&self) -> Vec<Status> {
        if self.all {
            Status::ALL.to_vec()
        } else {
            vec![Status::Draft, Status::Ready, Status::Active]
        }
    }
}

/// `/graph` plus the query made of the scope and extra pairs.
fn href(scope: &ScopeFilter, extra: &[(&str, String)]) -> String {
    let mut s = form_urlencoded::Serializer::new(String::new());
    for (k, v) in scope::pairs(scope) {
        s.append_pair(k, &v);
    }
    for (k, v) in extra {
        s.append_pair(k, v);
    }
    let q = s.finish();
    if q.is_empty() {
        Route::Graph.path().into()
    } else {
        format!("{}?{q}", Route::Graph.path())
    }
}

/// The web UI is always human.
fn identity() -> Identity {
    Identity {
        role: Role::Human,
        author: "web".into(),
        session: None,
    }
}

fn ticket_link(id: &str) -> Markup {
    html! { a class="row-id" href=(format!("/ticket/{id}")) { (id) } }
}

fn too_large(count: usize) -> Markup {
    components::empty(
        "Too large, narrow the scope",
        &format!(
            "This graph has {count} tickets and the page draws at most {NODE_CAP}. Pick a \
             Project or Worktree, filter by who it is for, or leave out done and abandoned \
             tickets."
        ),
    )
}

fn legend() -> Markup {
    html! {
        ul class="graph-legend" aria-label="Legend" {
            @for s in NodeState::ALL {
                li class=(format!("graph-{}", s.class())) {
                    span class="graph-legend-icon" aria-hidden="true" { (s.icon()) }
                    " " (s.label())
                }
            }
        }
    }
}

fn summary(g: &Graph) -> Markup {
    html! {
        div class="graph-lists" {
            section aria-label="Critical path" {
                h2 { "Critical path" }
                @if g.critical_path.is_empty() {
                    p class="muted" { "No unfinished ticket waits on another." }
                } @else {
                    ol class="graph-path" {
                        @for id in &g.critical_path { li { (ticket_link(id)) } }
                    }
                }
            }
            section aria-label="Top blockers" {
                h2 { "Top blockers" }
                @if g.top_blockers.is_empty() {
                    p class="muted" { "No unfinished ticket blocks another." }
                } @else {
                    ul class="graph-blockers" {
                        @for b in &g.top_blockers {
                            li { (ticket_link(&b.id)) " blocks " (b.blocks) }
                        }
                    }
                }
            }
        }
    }
}

/// The live part: the graph and its lists, or one state for an empty or
/// oversized scope.
pub fn region(conn: &Connection, scope: &Scope, q: &GraphQuery) -> Result<Markup> {
    let args = ListArgs {
        status: q.statuses(),
        for_who: q.for_who,
        // A graph must not drop tickets silently, so there is no limit.
        limit: Some(u32::MAX),
        ..ListArgs::default()
    };
    let list = ticket::list_in(conn, &scope.filter, &args, &identity())?;
    if list.tickets.is_empty() {
        return Ok(html! {
            div id="graph-region" {
                (components::empty(
                    "No tickets here",
                    "No tickets match. Create one with: scree add \"Title\""))
            }
        });
    }
    if list.tickets.len() > NODE_CAP {
        return Ok(html! { div id="graph-region" { (too_large(list.tickets.len())) } });
    }
    let g = graph::build(&list)?;
    if g.nodes.len() > NODE_CAP {
        return Ok(html! { div id="graph-region" { (too_large(g.nodes.len())) } });
    }
    let layout = graph_svg::layout(&g);
    Ok(html! {
        div id="graph-region" {
            (legend())
            div class="graph" tabindex="0" role="region" aria-label="Dependency graph" {
                (graph_svg::render(&g, &layout))
            }
            (summary(&g))
        }
    })
}

fn filters(scope: &Scope, q: &GraphQuery) -> Markup {
    html! {
        form class="filters" method="get" action=(Route::Graph.path()) role="search" {
            @for (k, v) in scope::pairs(&scope.filter) { input type="hidden" name=(k) value=(v); }
            label class="field" {
                span class="picker-label" { "For" }
                select name="for" {
                    option value="" selected[q.for_who.is_none()] { "anyone" }
                    option value="agent" selected[q.for_who == Some(ForWho::Agent)] { "agent" }
                    option value="human" selected[q.for_who == Some(ForWho::Human)] { "human" }
                }
            }
            label class="field" {
                input type="checkbox" name="all" value="1" checked[q.all];
                span { "Include done and abandoned" }
            }
            button type="submit" class="btn" { "Filter" }
        }
    }
}

fn body(conn: &Connection, scope: &Scope, q: &GraphQuery) -> Result<Markup> {
    let region = region(conn, scope, q)?;
    let view: String =
        form_urlencoded::byte_serialize(href(&scope.filter, &q.pairs()).as_bytes()).collect();
    Ok(html! {
        h1 { "Graph" }
        (picker::picker(scope, Route::Graph.path()))
        (filters(scope, q))
        div id="graph" data-init=(format!("@get('/stream?view={view}')")) { (region) }
    })
}

/// The live-view arm for `/graph` and `/graph?query`.
pub fn live(db: &FsPath, query: &str) -> Result<String> {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let conn = crate::db::open(db)?;
    let scope = scope::resolve(&conn, scope::filter_from_pairs(&pairs)?)?;
    let q = GraphQuery::parse(&pairs)?;
    Ok(region(&conn, &scope, &q)?.into_string())
}

pub async fn page(
    State(state): State<AppState>,
    Scoped(scope): Scoped,
    RawQuery(raw): RawQuery,
) -> Response {
    let pairs: Vec<(String, String)> =
        form_urlencoded::parse(raw.as_deref().unwrap_or("").as_bytes())
            .into_owned()
            .collect();
    let q = match GraphQuery::parse(&pairs) {
        Ok(q) => q,
        Err(e) => return WebError::new(e, "graph").into_response(),
    };
    let db = state.db.clone();
    let role = state.identity.role.as_str();
    let result = blocking(move || {
        let conn = crate::db::open(&db)?;
        let banner = view::banner_counts(&conn)?;
        Ok((banner, body(&conn, &scope, &q)?))
    })
    .await;
    let mut logged = None;
    let (status, ctx, markup) = match result {
        Ok((banner, body)) => (
            StatusCode::OK,
            PageCtx {
                route: Route::Graph,
                role,
                banner,
            },
            body,
        ),
        Err(e @ (Error::Usage(_) | Error::NotFound(_))) => {
            return WebError::new(e, "graph").into_response();
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            PageCtx {
                route: Route::Graph,
                role,
                banner: None,
            },
            components::error("Could not read the database", &unreadable(&e, &mut logged)),
        ),
    };
    (
        status,
        Html(layout::page(&ctx, "Graph", markup).into_string()),
    )
        .into_response()
        .logged(logged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(q: &str) -> Vec<(String, String)> {
        form_urlencoded::parse(q.as_bytes()).into_owned().collect()
    }

    #[test]
    fn query_reads_for_and_all() {
        let q = GraphQuery::parse(&pairs("p=SCR&for=human&all=1&x=1")).unwrap();
        assert_eq!(q.for_who, Some(ForWho::Human));
        assert!(q.all);
        assert_eq!(q.statuses().len(), 5);
        assert_eq!(
            GraphQuery::parse(&pairs("for=&all=0")).unwrap(),
            GraphQuery::default()
        );
        assert_eq!(GraphQuery::default().statuses().len(), 3);
    }

    #[test]
    fn query_round_trips_and_rejects_bad_values() {
        let q = GraphQuery::parse(&pairs("for=agent&all=1")).unwrap();
        let again: Vec<(String, String)> = q
            .pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        assert_eq!(GraphQuery::parse(&again).unwrap(), q);
        for bad in ["for=a&for=b", "for=robot", "all=yes", "all=1&all=0"] {
            assert!(
                matches!(GraphQuery::parse(&pairs(bad)), Err(Error::Usage(_))),
                "{bad}"
            );
        }
        assert_eq!(
            href(
                &ScopeFilter::Project { key: "A".into() },
                &[("all", "1".into())]
            ),
            "/graph?p=A&all=1"
        );
    }
}
