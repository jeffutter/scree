//! The List screen (home, `/`): tickets of the scope as rows, grouped by
//! Project when the scope spans several, narrowed by filters that reuse the
//! CLI `list` flag names. All state is in the URL, so a reload shows the same
//! view. The page and the live stream share [`region`], so they cannot
//! disagree about what a URL shows.

use axum::extract::{RawQuery, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use clap::ValueEnum;
use maud::{Markup, html};
use rusqlite::Connection;

use crate::cli::{ForWho, ListArgs, Status};
use crate::error::{Error, Result};
use crate::identity::{Identity, Role};
use crate::ticket::{self, ScopeFilter};
use crate::web::AppState;
use crate::web::error::WebError;
use crate::web::scope::{self, Scope, Scoped};
use crate::web::state::blocking;
use crate::web::view::components::{self, FIRST_RENDER_ROWS};
use crate::web::view::layout::{self, PageCtx, Route};
use crate::web::view::{self, picker, ticket_row};

/// The most rows one request may ask for with `limit`.
pub const MAX_LIMIT: usize = 5000;

/// The filter half of a List URL (the scope half is `p` and `w`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListQuery {
    /// Empty means the CLI default: draft, ready and active.
    pub statuses: Vec<Status>,
    pub for_who: Option<ForWho>,
    pub blocked: bool,
    pub available: bool,
    pub archived: bool,
    pub stale: bool,
    pub older_than: Option<String>,
    /// Trimmed search text; empty means no search.
    pub q: String,
    /// `None` is the first-render cap.
    pub limit: Option<usize>,
}

fn usage(msg: impl Into<String>) -> Error {
    Error::Usage(msg.into())
}

/// The one value of `name`, or `None` when absent or blank.
fn single<'a>(pairs: &'a [(String, String)], name: &str) -> Result<Option<&'a str>> {
    let mut found = pairs.iter().filter(|(k, _)| k == name);
    let first = found.next().map(|(_, v)| v.as_str());
    if found.next().is_some() {
        return Err(usage(format!("{name} given twice")));
    }
    Ok(first.filter(|v| !v.trim().is_empty()))
}

fn flag(pairs: &[(String, String)], name: &str) -> Result<bool> {
    match single(pairs, name)? {
        None | Some("0" | "false") => Ok(false),
        Some("1" | "true" | "on") => Ok(true),
        Some(v) => Err(usage(format!("{name} must be 1 or 0, not {v:?}"))),
    }
}

impl ListQuery {
    /// Read the filter parameters of a query; `p`, `w` and anything unknown
    /// are ignored here. A bad value is [`Error::Usage`].
    pub fn parse(pairs: &[(String, String)]) -> Result<ListQuery> {
        let mut statuses = Vec::new();
        for (_, v) in pairs.iter().filter(|(k, _)| k == "status") {
            if v.trim().is_empty() {
                continue;
            }
            let s = Status::from_str(v.trim(), true)
                .map_err(|_| usage(format!("unknown status {v:?}")))?;
            if !statuses.contains(&s) {
                statuses.push(s);
            }
        }
        let for_who = single(pairs, "for")?
            .map(|v| ForWho::from_str(v, true).map_err(|_| usage(format!("unknown for {v:?}"))))
            .transpose()?;
        let limit = single(pairs, "limit")?
            .map(|v| match v.parse::<usize>() {
                Ok(n) if n >= 1 => Ok(n.min(MAX_LIMIT)),
                _ => Err(usage(format!("limit must be a positive number, not {v:?}"))),
            })
            .transpose()?;
        let q = ListQuery {
            statuses,
            for_who,
            blocked: flag(pairs, "blocked")?,
            available: flag(pairs, "available")?,
            archived: flag(pairs, "archived")?,
            stale: flag(pairs, "stale")?,
            older_than: single(pairs, "older-than")?.map(|v| v.trim().to_string()),
            q: single(pairs, "q")?
                .map(|v| v.trim().to_string())
                .unwrap_or_default(),
            limit,
        };
        if q.blocked && q.available {
            return Err(usage("blocked and available cannot be combined"));
        }
        if q.older_than.is_some() && !q.stale {
            return Err(usage("older-than needs stale"));
        }
        Ok(q)
    }

    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(FIRST_RENDER_ROWS)
    }

    fn args(&self) -> ListArgs {
        ListArgs {
            status: self.statuses.clone(),
            for_who: self.for_who,
            limit: Some(u32::try_from(self.limit()).unwrap_or(u32::MAX)),
            archived: self.archived,
            blocked: self.blocked,
            available: self.available,
            stale: self.stale,
            older_than: self.older_than.clone(),
            search: (!self.q.is_empty()).then(|| self.q.clone()),
            ..ListArgs::default()
        }
    }

    /// The query pairs that name this filter, in a fixed order, so equal
    /// filters give equal URLs. `limit` is included only when set.
    pub fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        for s in &self.statuses {
            v.push(("status", s.as_str().to_string()));
        }
        if let Some(f) = self.for_who {
            v.push(("for", f.as_str().to_string()));
        }
        for (name, on) in [
            ("blocked", self.blocked),
            ("available", self.available),
            ("archived", self.archived),
            ("stale", self.stale),
        ] {
            if on {
                v.push((name, "1".to_string()));
            }
        }
        if let Some(o) = &self.older_than {
            v.push(("older-than", o.clone()));
        }
        if !self.q.is_empty() {
            v.push(("q", self.q.clone()));
        }
        if let Some(l) = self.limit {
            v.push(("limit", l.to_string()));
        }
        v
    }

    fn is_filtered(&self) -> bool {
        let none = ListQuery {
            limit: self.limit,
            ..ListQuery::default()
        };
        *self != none
    }
}

/// `/` plus the query made of the scope and extra pairs.
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
        "/".into()
    } else {
        format!("/?{q}")
    }
}

/// The web UI is always human; the list read never uses the caller's name.
fn identity() -> Identity {
    Identity {
        role: Role::Human,
        author: "web".into(),
        session: None,
    }
}

/// The live part of the page: rows (or an empty state) and the "show more"
/// footer. Filters live outside it so a patch never touches what is typed.
pub fn region(conn: &Connection, scope: &Scope, q: &ListQuery) -> Result<Markup> {
    let who = identity();
    let args = q.args();
    let tickets = ticket::list_in(conn, &scope.filter, &args, &who)?.tickets;
    let total = usize::try_from(ticket::count_in(conn, &scope.filter, &args, &who)?)
        .unwrap_or(usize::MAX)
        .max(tickets.len());

    let body = if tickets.is_empty() {
        let everything = ListArgs {
            status: Status::ALL.to_vec(),
            archived: true,
            ..ListArgs::default()
        };
        let any = ticket::count_in(conn, &scope.filter, &everything, &who)? > 0;
        if any {
            html! {
                (components::empty(
                    "No tickets match",
                    "Nothing in this scope fits the filters or the search."))
                p class="more" {
                    a href=(href(&scope.filter, &[])) { "Clear filters" }
                }
            }
        } else {
            html! {
                (components::empty(
                    "No tickets here",
                    "This scope has no tickets. Create one with: scree add \"Title\""))
            }
        }
    } else if picker::qualify_ids(scope) {
        // Rows arrive ordered by Project key, so a group is a run.
        let mut groups: Vec<(&str, Vec<&crate::model::Ticket>)> = Vec::new();
        for t in &tickets {
            match groups.last_mut() {
                Some((k, rows)) if *k == t.project.key => rows.push(t),
                _ => groups.push((&t.project.key, vec![t])),
            }
        }
        html! {
            @for (key, rows) in &groups {
                section class="group" {
                    h2 class="group-title" { (key) }
                    ul class="rows" {
                        @for t in rows { (ticket_row::row(t)) }
                    }
                }
            }
        }
    } else {
        html! { ul class="rows" { @for t in &tickets { (ticket_row::row(t)) } } }
    };

    let more = {
        let next = (q.limit() * 2).min(MAX_LIMIT);
        let mut extra: Vec<(&str, String)> = q
            .pairs()
            .into_iter()
            .filter(|(k, _)| *k != "limit")
            .collect();
        extra.push(("limit", next.to_string()));
        components::show_more(tickets.len(), total, &href(&scope.filter, &extra))
    };
    Ok(html! {
        div id="list-region" { (body) (more) }
    })
}

fn checkbox(name: &str, label: &str, value: &str, on: bool) -> Markup {
    html! {
        label class="check" {
            input type="checkbox" name=(name) value=(value) checked[on];
            " " (label)
        }
    }
}

fn filter_bar(scope: &Scope, q: &ListQuery) -> Markup {
    let statuses: &[Status] = if q.statuses.is_empty() {
        &Status::OPEN
    } else {
        &q.statuses
    };
    html! {
        form class="filters" method="get" action=(Route::List.path()) role="search" {
            @for (k, v) in scope::pairs(&scope.filter) {
                input type="hidden" name=(k) value=(v);
            }
            fieldset class="filter-group" {
                legend { "Status" }
                @for s in Status::ALL {
                    (checkbox("status", s.as_str(), s.as_str(), statuses.contains(&s)))
                }
            }
            label class="field" {
                span class="picker-label" { "For" }
                select name="for" {
                    option value="" selected[q.for_who.is_none()] { "Anyone" }
                    @for f in [ForWho::Agent, ForWho::Human] {
                        option value=(f.as_str()) selected[q.for_who == Some(f)] { (f.as_str()) }
                    }
                }
            }
            fieldset class="filter-group" {
                legend { "Only" }
                (checkbox("blocked", "blocked", "1", q.blocked))
                (checkbox("available", "available", "1", q.available))
                (checkbox("stale", "stale claims", "1", q.stale))
                (checkbox("archived", "include archived", "1", q.archived))
            }
            label class="field" {
                span class="picker-label" { "Stale after" }
                input type="text" name="older-than" value=(q.older_than.as_deref().unwrap_or(""))
                    placeholder="4h";
            }
            label class="field field-search" {
                span class="picker-label" { "Search" }
                input type="search" id="q" name="q" value=(q.q);
            }
            button class="btn" type="submit" { "Apply filters" }
            div id="err-filter" {}
        }
    }
}

/// The URL the live stream follows: the canonical form of this view.
fn live_view(scope: &Scope, q: &ListQuery) -> String {
    let pairs: Vec<(&str, String)> = q.pairs();
    href(&scope.filter, &pairs)
}

/// The whole page body for a scope and filter.
fn body(conn: &Connection, scope: &Scope, q: &ListQuery, role: &str) -> Result<Markup> {
    let region = region(conn, scope, q)?;
    let next = href(
        &scope.filter,
        &[("available", "1".into()), ("for", role.into())],
    );
    let stale = href(&scope.filter, &[("stale", "1".into())]);
    let view: String = form_urlencoded::byte_serialize(live_view(scope, q).as_bytes()).collect();
    Ok(html! {
        h1 { "Tickets" }
        (picker::picker(scope, Route::List.path()))
        (filter_bar(scope, q))
        p class="saved" {
            "Saved: " a href=(next) { "Next for me" } " " a href=(stale) { "Stale claims" }
        }
        @if q.is_filtered() {
            p class="more" { a href=(href(&scope.filter, &[])) { "Clear filters" } }
        }
        div id="list" data-init=(format!("@get('/stream?view={view}')")) { (region) }
        div id="list-changed" hidden { (components::stale("This ticket changed or was removed")) }
    })
}

/// The live-view arm for `/` and `/?query`: the region for that URL.
pub fn live(db: &std::path::Path, query: &str) -> Result<String> {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let conn = crate::db::open(db)?;
    let scope = scope::resolve(&conn, scope::filter_from_pairs(&pairs)?)?;
    let q = ListQuery::parse(&pairs)?;
    Ok(region(&conn, &scope, &q)?.into_string())
}

pub async fn index(
    State(state): State<AppState>,
    Scoped(scope): Scoped,
    RawQuery(raw): RawQuery,
) -> Response {
    let pairs: Vec<(String, String)> =
        form_urlencoded::parse(raw.as_deref().unwrap_or("").as_bytes())
            .into_owned()
            .collect();
    let q = match ListQuery::parse(&pairs) {
        Ok(q) => q,
        Err(e) => return WebError::new(e, "filter").into_response(),
    };
    let db = state.db.clone();
    let role = state.identity.role.as_str();
    let result = blocking(move || {
        let conn = crate::db::open(&db)?;
        let banner = view::banner_counts(&conn)?;
        let body = body(&conn, &scope, &q, role)?;
        Ok((banner, body))
    })
    .await;
    let (status, ctx, markup) = match result {
        Ok((banner, body)) => (
            StatusCode::OK,
            PageCtx {
                route: Route::List,
                role,
                banner,
            },
            body,
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            PageCtx {
                route: Route::List,
                role,
                banner: None,
            },
            components::error("Could not read the database", &e.to_string()),
        ),
    };
    (
        status,
        Html(layout::page(&ctx, "Tickets", markup).into_string()),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(q: &str) -> Vec<(String, String)> {
        form_urlencoded::parse(q.as_bytes()).into_owned().collect()
    }

    #[test]
    fn defaults_are_the_cli_defaults() {
        let q = ListQuery::parse(&pairs("")).unwrap();
        assert_eq!(q, ListQuery::default());
        let a = q.args();
        assert!(a.status.is_empty(), "empty status means draft/ready/active");
        assert_eq!(a.limit, Some(200));
        assert_eq!(a.search, None);
        assert!(q.pairs().is_empty());
    }

    #[test]
    fn round_trips_to_list_args_and_back() {
        let q = ListQuery::parse(&pairs(
            "p=SCR&status=done&status=ready&status=done&for=human&available=1&archived=1&q=%20a%26b%20&limit=50",
        ))
        .unwrap();
        assert_eq!(q.statuses, [Status::Done, Status::Ready]);
        assert_eq!(q.for_who, Some(ForWho::Human));
        assert_eq!(q.q, "a&b");
        let a = q.args();
        assert_eq!(a.limit, Some(50));
        assert_eq!(a.search.as_deref(), Some("a&b"));
        assert!(a.available && a.archived && !a.blocked);
        let again: Vec<(String, String)> = q
            .pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        assert_eq!(ListQuery::parse(&again).unwrap(), q);
    }

    #[test]
    fn blank_values_are_absent() {
        let q = ListQuery::parse(&pairs("for=&q=&status=&limit=&older-than=")).unwrap();
        assert_eq!(q, ListQuery::default());
    }

    #[test]
    fn limit_is_clamped() {
        let q = ListQuery::parse(&pairs("limit=999999")).unwrap();
        assert_eq!(q.limit(), MAX_LIMIT);
    }

    #[test]
    fn bad_values_and_conflicts_are_usage_errors() {
        for bad in [
            "status=nope",
            "for=robot",
            "blocked=maybe",
            "limit=0",
            "limit=x",
            "limit=1&limit=2",
            "blocked=1&available=1",
            "older-than=4h",
            "q=a&q=b",
        ] {
            assert!(
                matches!(ListQuery::parse(&pairs(bad)), Err(Error::Usage(_))),
                "{bad}"
            );
        }
        assert!(ListQuery::parse(&pairs("stale=1&older-than=4h")).is_ok());
    }

    #[test]
    fn href_encodes_scope_and_filters() {
        assert_eq!(href(&ScopeFilter::All, &[]), "/");
        assert_eq!(
            href(
                &ScopeFilter::Project { key: "A".into() },
                &[("q", "a b&c".into())]
            ),
            "/?p=A&q=a+b%26c"
        );
    }
}
