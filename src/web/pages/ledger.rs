//! The Decisions ledger (`/ledger`): decided Decisions of the scope, newest
//! first, with answer, rationale, who and when, the tickets they unblocked and
//! a marker for superseded ones; search and since filters; and the supersede
//! action.
//!
//! The filters and the selection (`?d=SCR-D-3`, always a qualified id) live in
//! the URL, so they survive a reload. The page and the live stream share
//! [`region`]. The supersede form sits outside the live region so a patch never
//! wipes typed text. The library decides what may be superseded (not itself,
//! not an already superseded Decision, same Project, decided): a refusal comes
//! back as an inline 422.

use std::path::Path as FsPath;

use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use maud::{Markup, html};
use rusqlite::Connection;

use crate::decision;
use crate::error::{Error, Result};
use crate::model::DecidedDecision;
use crate::web::AppState;
use crate::web::error::{LoggedExt, WebError, unreadable};
use crate::web::handlers::Form;
use crate::web::pages::decisions::{ctx, qualified};
use crate::web::pages::list::MAX_LIMIT;
use crate::web::scope::{self, Scope, Scoped};
use crate::web::state::blocking;
use crate::web::view::components::{self, FIRST_RENDER_ROWS};
use crate::web::view::layout::{self, PageCtx, Route};
use crate::web::view::{self, picker};

/// The parts of a Ledger URL beyond the scope.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LedgerQuery {
    /// Text to find in the question, answer or rationale.
    pub q: Option<String>,
    /// The `--done-since` grammar (`7d`, `2026-01-01`); the library parses it.
    pub since: Option<String>,
    /// Also list superseded Decisions.
    pub superseded: bool,
    /// The selected Decision, as a qualified id in upper case.
    pub d: Option<String>,
    /// `None` is the first-render cap.
    pub limit: Option<usize>,
}

fn usage(msg: impl Into<String>) -> Error {
    Error::Usage(msg.into())
}

impl LedgerQuery {
    /// Read the ledger's parameters; `p`, `w` and anything unknown are ignored.
    pub fn parse(pairs: &[(String, String)]) -> Result<LedgerQuery> {
        let one = |name: &str| -> Result<Option<&str>> {
            let mut found = pairs.iter().filter(|(k, _)| k == name);
            let first = found.next().map(|(_, v)| v.as_str());
            if found.next().is_some() {
                return Err(usage(format!("{name} given twice")));
            }
            Ok(first.filter(|v| !v.trim().is_empty()))
        };
        let limit = one("limit")?
            .map(|v| match v.parse::<usize>() {
                Ok(n) if n >= 1 => Ok(n.min(MAX_LIMIT)),
                _ => Err(usage(format!("limit must be a positive number, not {v:?}"))),
            })
            .transpose()?;
        Ok(LedgerQuery {
            q: one("q")?.map(|v| v.trim().to_string()),
            since: one("since")?.map(|v| v.trim().to_string()),
            superseded: one("superseded")?.is_some(),
            d: one("d")?.map(qualified).transpose()?,
            limit,
        })
    }

    fn limit(&self) -> usize {
        self.limit.unwrap_or(FIRST_RENDER_ROWS)
    }

    fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        if let Some(q) = &self.q {
            v.push(("q", q.clone()));
        }
        if let Some(s) = &self.since {
            v.push(("since", s.clone()));
        }
        if self.superseded {
            v.push(("superseded", "1".into()));
        }
        if let Some(d) = &self.d {
            v.push(("d", d.clone()));
        }
        if let Some(l) = self.limit {
            v.push(("limit", l.to_string()));
        }
        v
    }

    fn filtered(&self) -> bool {
        self.q.is_some() || self.since.is_some()
    }
}

/// `/ledger` plus the query made of the scope and extra pairs.
fn href(scope: &crate::ticket::ScopeFilter, extra: &[(&str, String)]) -> String {
    let mut s = form_urlencoded::Serializer::new(String::new());
    for (k, v) in scope::pairs(scope) {
        s.append_pair(k, &v);
    }
    for (k, v) in extra {
        s.append_pair(k, v);
    }
    let q = s.finish();
    if q.is_empty() {
        Route::Ledger.path().into()
    } else {
        format!("{}?{q}", Route::Ledger.path())
    }
}

fn qualified_id(d: &DecidedDecision) -> String {
    format!("{}-{}", d.project.key, d.id)
}

fn read(conn: &Connection, scope: &Scope, q: &LedgerQuery) -> Result<Vec<DecidedDecision>> {
    Ok(decision::decided_log_in(
        conn,
        &scope.filter,
        q.since.as_deref(),
        q.q.as_deref(),
        u32::try_from(MAX_LIMIT).unwrap_or(u32::MAX),
        q.superseded,
    )?
    .decisions)
}

fn row(scope: &Scope, current: Option<&str>, d: &DecidedDecision) -> Markup {
    let id = qualified_id(d);
    let on = current.is_some_and(|c| c.eq_ignore_ascii_case(&id));
    let when = d
        .decided_at
        .as_deref()
        .map(|t| t.get(..10).unwrap_or(t))
        .unwrap_or("");
    html! {
        li class="row" data-id=(id) aria-current=[on.then_some("true")] {
            a class="row-id" href=(href(&scope.filter, &[("d", id.clone())])) { (id) }
            span class="row-title ledger-question" { (d.question) }
            @if let Some(by) = &d.superseded_by {
                span class="tag" { "superseded by " (d.project.key) "-" (by) }
            }
            p class="ledger-answer" {
                "Answer: " (d.answer.as_deref().unwrap_or("(no answer text)"))
            }
            @if let Some(r) = &d.rationale {
                p class="meta" { "Rationale: " (r) }
            }
            p class="meta ledger-by" {
                (components::icon("person"))
                "Decided by " (d.decided_by.as_deref().unwrap_or("someone"))
                @if !when.is_empty() { " on " (when) }
                @if !d.tickets.is_empty() {
                    " - tickets: "
                    @for (i, t) in d.tickets.iter().enumerate() {
                        @if i > 0 { ", " }
                        a href=(format!("/ticket/{t}")) { (t) }
                    }
                }
            }
        }
    }
}

/// The live part: the filtered list of decided Decisions.
pub fn region(conn: &Connection, scope: &Scope, q: &LedgerQuery) -> Result<Markup> {
    let all = read(conn, scope, q)?;
    let total = all.len();
    let shown = q.limit().min(total);
    let current = q.d.as_deref();
    let more = {
        let next = (q.limit() * 2).min(MAX_LIMIT);
        let mut extra: Vec<(&str, String)> = q
            .pairs()
            .into_iter()
            .filter(|(k, _)| *k != "limit")
            .collect();
        extra.push(("limit", next.to_string()));
        components::show_more(shown, total, &href(&scope.filter, &extra))
    };
    Ok(html! {
        div id="ledger-region" {
            @if all.is_empty() {
                @if q.filtered() {
                    (components::empty(
                        "No decided Decisions match",
                        "Change or clear the search and the since filter."))
                } @else {
                    (components::empty(
                        "No decided Decisions",
                        "Nothing has been decided in this scope yet. Superseded Decisions are hidden unless you ask for them."))
                }
            } @else {
                ul class="rows" aria-label="Decided Decisions" {
                    @for d in &all[..shown] { (row(scope, current, d)) }
                }
                (more)
            }
        }
    })
}

/// The supersede form for the selected Decision. Outside the live region.
fn actions(scope: &Scope, selected: Option<&str>) -> Markup {
    html! {
        div id="ledger-actions" class=[selected.map(|_| "panel")] {
            @if let Some(id) = selected {
                form class="stack-form" data-on:submit=(format!("@post('/ledger/{id}/supersede', {{contentType: 'form'}})")) {
                    @for (k, v) in scope::pairs(&scope.filter) { input type="hidden" name=(k) value=(v); }
                    h2 { "Supersede " (id) }
                    label class="field" {
                        span class="meta" { "Replaced by (qualified id, e.g. SCR-D-9)" }
                        input type="text" name="by" required;
                    }
                    button type="submit" class="btn" { "Supersede" }
                }
                div id="err-supersede" {}
            } @else {
                p class="more" { "Select a Decision to supersede it." }
            }
        }
    }
}

fn filters(scope: &Scope, q: &LedgerQuery) -> Markup {
    html! {
        form class="filters" method="get" action=(Route::Ledger.path()) role="search" data-autosubmit {
            @for (k, v) in scope::pairs(&scope.filter) { input type="hidden" name=(k) value=(v); }
            label class="field" {
                span class="picker-label" { "Search" }
                input type="search" name="q" value=(q.q.as_deref().unwrap_or(""));
            }
            label class="field" {
                span class="picker-label" { "Since" }
                input type="text" name="since" value=(q.since.as_deref().unwrap_or(""))
                    placeholder="7d, 2026-01-01";
            }
            label class="check" {
                input type="checkbox" name="superseded" value="1" checked[q.superseded];
                " show superseded"
            }
            noscript { button type="submit" class="btn" { "Filter" } }
        }
    }
}

fn live_view(scope: &Scope, q: &LedgerQuery) -> String {
    href(&scope.filter, &q.pairs())
}

/// The selected id when it names a decided Decision; `None` for anything else.
fn selectable(conn: &Connection, q: &LedgerQuery) -> Result<Option<String>> {
    let Some(want) = &q.d else {
        return Ok(None);
    };
    match decision::get(conn, &ctx(), want) {
        Ok(d) if d.status == "decided" => Ok(Some(want.clone())),
        Ok(_) | Err(Error::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

fn body(conn: &Connection, scope: &Scope, q: &LedgerQuery) -> Result<Markup> {
    let region = region(conn, scope, q)?;
    let selected = selectable(conn, q)?;
    let view: String = form_urlencoded::byte_serialize(live_view(scope, q).as_bytes()).collect();
    Ok(html! {
        h1 { "Ledger" }
        (picker::bar(html! { (picker::picker(scope, Route::Ledger.path())) (filters(scope, q)) }))
        div id="ledger" data-init=(format!("@get('/stream?view={view}')")) { (region) }
        (actions(scope, selected.as_deref()))
    })
}

/// The live-view arm for `/ledger` and `/ledger?query`.
pub fn live(db: &FsPath, query: &str) -> Result<String> {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let conn = crate::db::open(db)?;
    let scope = scope::resolve(&conn, scope::filter_from_pairs(&pairs)?)?;
    let q = LedgerQuery::parse(&pairs)?;
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
    let q = match LedgerQuery::parse(&pairs) {
        Ok(q) => q,
        Err(e) => return WebError::new(e, "ledger").into_response(),
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
                route: Route::Ledger,
                role,
                banner,
            },
            body,
        ),
        // A bad `since` is the caller's mistake, not a database failure.
        Err(e @ (Error::Usage(_) | Error::NotFound(_))) => {
            return WebError::new(e, "ledger").into_response();
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            PageCtx {
                route: Route::Ledger,
                role,
                banner: None,
            },
            components::error("Could not read the database", &unreadable(&e, &mut logged)),
        ),
    };
    (
        status,
        Html(layout::page(&ctx, "Ledger", markup).into_string()),
    )
        .into_response()
        .logged(logged)
}

/// `POST /ledger/{id}/supersede`: mark `id` as replaced by the Decision named
/// in `by`. Both must be qualified ids; a blank `by` is `Usage` (400).
pub async fn supersede(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let by = Form::parse(&body).get("by").trim().to_string();
    let db = state.db.clone();
    let result = blocking(move || {
        if by.is_empty() {
            return Err(usage("name the Decision that replaces it (by)"));
        }
        let (id, by) = (qualified(&id)?, qualified(&by)?);
        let mut conn = crate::db::open(&db)?;
        decision::supersede(&mut conn, &ctx(), &id, &by)?;
        Ok((id, by))
    })
    .await;
    match result {
        Ok((id, by)) => Html(
            html! {
                div id="ledger-actions" class="panel" {
                    p class="notice-hint" role="status" { (id) " superseded by " (by) "." }
                }
            }
            .into_string(),
        )
        .into_response(),
        Err(e) => WebError::new(e, "supersede").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ticket::ScopeFilter;

    fn pairs(q: &str) -> Vec<(String, String)> {
        form_urlencoded::parse(q.as_bytes()).into_owned().collect()
    }

    #[test]
    fn query_reads_filters_selection_and_limit() {
        let q = LedgerQuery::parse(&pairs(
            "p=SCR&q=%20db%20&since=7d&superseded=1&d=scr-d-3&limit=7",
        ))
        .unwrap();
        assert_eq!(q.q.as_deref(), Some("db"));
        assert_eq!(q.since.as_deref(), Some("7d"));
        assert!(q.superseded);
        assert_eq!(q.d.as_deref(), Some("SCR-D-3"));
        assert_eq!(q.limit(), 7);
        assert_eq!(
            LedgerQuery::parse(&pairs("q=&since=&d=")).unwrap(),
            LedgerQuery::default()
        );
    }

    #[test]
    fn duplicates_and_bad_ids_are_usage_errors() {
        for bad in ["q=a&q=b", "d=D-3", "limit=0", "since=1&since=2"] {
            assert!(
                matches!(LedgerQuery::parse(&pairs(bad)), Err(Error::Usage(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn query_pairs_round_trip_and_href_carries_scope() {
        let q = LedgerQuery::parse(&pairs("q=x&since=7d&superseded=1&d=AB-D-2&limit=9")).unwrap();
        let again: Vec<(String, String)> = q
            .pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        assert_eq!(LedgerQuery::parse(&again).unwrap(), q);
        assert_eq!(href(&ScopeFilter::All, &[]), "/ledger");
        assert_eq!(
            href(
                &ScopeFilter::Project { key: "A".into() },
                &[("d", "A-D-1".into())]
            ),
            "/ledger?p=A&d=A-D-1"
        );
    }
}
