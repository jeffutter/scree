//! The Decisions screen (`/decisions`): the open Decisions of the scope as a
//! queue (most unblocked tickets first, then oldest), the selected one in
//! full, and the two ways to answer it: accept the recommendation, or write an
//! answer with a rationale.
//!
//! The selection is in the URL (`?d=SCR-D-3`, always a qualified id: a bare
//! `D-3` means whatever Project the caller is in, and the web has no caller
//! directory). The page and the live stream share [`region`]. The write forms
//! sit outside the live region, so a patch never wipes typed text; answering a
//! Decision that was answered elsewhere gives an inline 409 from the library.

use std::path::Path as FsPath;

use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use maud::{Markup, html};
use rusqlite::Connection;

use crate::decision::{self, Ctx, Resolution};
use crate::error::{Error, Result};
use crate::model::OpenDecision;
use crate::project;
use crate::ticket::ScopeFilter;
use crate::web::AppState;
use crate::web::error::{LoggedExt, WebError, unreadable};
use crate::web::handlers::Form;
use crate::web::pages::list::MAX_LIMIT;
use crate::web::scope::{self, Scope, Scoped};
use crate::web::state::blocking;
use crate::web::view::components::{self, FIRST_RENDER_ROWS};
use crate::web::view::layout::{self, PageCtx, Route};
use crate::web::view::{self, picker};

/// The parts of a Decisions URL beyond the scope.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DecisionsQuery {
    /// The selected Decision, as a qualified id in upper case.
    pub d: Option<String>,
    /// `None` is the first-render cap.
    pub limit: Option<usize>,
}

fn usage(msg: impl Into<String>) -> Error {
    Error::Usage(msg.into())
}

/// `KEY-D-n` in upper case, or `Usage`. A bare `D-n` is refused on purpose.
pub(super) fn qualified(raw: &str) -> Result<String> {
    let up = raw.trim().to_ascii_uppercase();
    let ok = up.split_once("-D-").is_some_and(|(key, n)| {
        project::valid_key(key) && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
    });
    if ok {
        Ok(up)
    } else {
        Err(usage(format!(
            "{raw:?} is not a qualified Decision id; ids look like SCR-D-3"
        )))
    }
}

impl DecisionsQuery {
    /// Read `d` and `limit`; `p`, `w` and anything unknown are ignored.
    pub fn parse(pairs: &[(String, String)]) -> Result<DecisionsQuery> {
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
        Ok(DecisionsQuery {
            d: one("d")?.map(qualified).transpose()?,
            limit,
        })
    }

    fn limit(&self) -> usize {
        self.limit.unwrap_or(FIRST_RENDER_ROWS)
    }

    fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        if let Some(d) = &self.d {
            v.push(("d", d.clone()));
        }
        if let Some(l) = self.limit {
            v.push(("limit", l.to_string()));
        }
        v
    }
}

/// `/decisions` plus the query made of the scope and extra pairs.
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
        Route::Decisions.path().into()
    } else {
        format!("{}?{q}", Route::Decisions.path())
    }
}

/// The id of a queue row: `<project key>-D-<number>`.
fn qualified_id(d: &OpenDecision) -> String {
    format!("{}-D-{}", d.project.key, d.number)
}

/// The library never reads this for a qualified id (see `decision::locate`),
/// so the web hands it a fixed neutral path instead of the process cwd.
pub(super) fn ctx() -> Ctx<'static> {
    Ctx {
        cwd: FsPath::new("/"),
        project: None,
    }
}

/// What the selected Decision is right now.
enum Selection {
    None,
    /// Open: can be answered. Carries what the summary and forms need.
    Open(Summary),
    /// Not open any more (or never was, or gone): the sentence that says so.
    Changed(String),
}

struct Summary {
    id: String,
    question: String,
    options: Vec<String>,
    recommendation: Option<String>,
    tickets: Vec<String>,
}

fn selection(conn: &Connection, queue: &[OpenDecision], q: &DecisionsQuery) -> Result<Selection> {
    let Some(want) = &q.d else {
        return Ok(Selection::None);
    };
    if let Some(d) = queue
        .iter()
        .find(|d| qualified_id(d).eq_ignore_ascii_case(want))
    {
        return Ok(Selection::Open(Summary {
            id: qualified_id(d),
            question: d.question.clone(),
            options: d.options.clone(),
            recommendation: d.recommendation.clone(),
            tickets: d.tickets.clone(),
        }));
    }
    // Not in the queue: say what happened instead of showing nothing.
    match decision::get(conn, &ctx(), want) {
        Ok(d) if d.status == "open" && d.superseded_by.is_none() => Ok(Selection::Open(Summary {
            id: want.clone(),
            question: d.question,
            options: d.options,
            recommendation: d.recommendation,
            tickets: d.tickets,
        })),
        Ok(d) if d.superseded_by.is_some() => Ok(Selection::Changed(format!(
            "This Decision changed: {want} was superseded by {}",
            d.superseded_by.unwrap_or_default()
        ))),
        Ok(d) => Ok(Selection::Changed(format!(
            "This Decision changed: {want} was answered by {}: {}",
            d.decided_by.as_deref().unwrap_or("someone"),
            d.answer.as_deref().unwrap_or("(no answer text)")
        ))),
        Err(Error::NotFound(_)) => Ok(Selection::Changed(format!(
            "This Decision changed or was removed: {want}"
        ))),
        Err(e) => Err(e),
    }
}

fn summary(s: &Summary) -> Markup {
    html! {
        section class="detail-section" aria-labelledby="decision-question" {
            p class="meta" { span class="row-id" { (s.id) } }
            h2 id="decision-question" class="detail-title" { (s.question) }
            @if s.options.is_empty() {
                p class="meta" { "No options were offered." }
            } @else {
                ol class="options" {
                    @for o in &s.options { li { (o) } }
                }
            }
            @match &s.recommendation {
                Some(r) => p class="recommendation" {
                    span class="tag" { "recommended" } " " (r)
                },
                None => p class="meta" { "No recommendation." },
            }
            h3 { "Unblocks" }
            @match s.tickets.as_slice() {
                [] => p class="meta" { "No unfinished ticket is waiting on this." },
                [one] => p {
                    a href=(format!("/ticket/{one}")) { "Open ticket " (one) }
                },
                many => ul class="plain" {
                    @for t in many {
                        li class="related" { a href=(format!("/ticket/{t}")) { (t) } }
                    }
                },
            }
        }
    }
}

/// The live part: the queue and the selected Decision's summary.
pub fn region(conn: &Connection, scope: &Scope, q: &DecisionsQuery) -> Result<Markup> {
    let queue = decision::open_queue_in(conn, &scope.filter)?.decisions;
    let total = queue.len();
    let selected = selection(conn, &queue, q)?;
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
        div id="decisions-region" {
            @if queue.is_empty() {
                (components::empty(
                    "No open Decisions",
                    "Nothing is waiting on an answer in this scope. Add one with: scree decision add"))
            } @else {
                ul class="rows" aria-label="Open Decisions" {
                    @for d in &queue[..shown] {
                        @let id = qualified_id(d);
                        @let on = current.is_some_and(|c| c.eq_ignore_ascii_case(&id));
                        li class="row" data-id=(id) aria-current=[on.then_some("true")] {
                            a class="row-id" href=(href(&scope.filter, &[("d", id.clone())])) { (id) }
                            span class="row-title" { (d.question) }
                            span class="tag" { "unblocks " (d.unblocks) }
                            span class="tag" { "opened " (d.created_at.get(..10).unwrap_or(&d.created_at)) }
                        }
                    }
                }
                (more)
            }
            @match &selected {
                Selection::None => {
                    @if !queue.is_empty() {
                        p class="more" { "Select a Decision to read and answer it." }
                    }
                }
                Selection::Open(s) => (summary(s)),
                Selection::Changed(msg) => (components::stale(msg)),
            }
        }
    })
}

/// The write forms for an open Decision. Outside the live region.
fn actions(scope: &ScopeFilter, s: &Summary) -> Markup {
    let id = &s.id;
    let hidden = |m: &mut Vec<(&'static str, String)>| m.extend(scope::pairs(scope));
    let mut scope_fields = Vec::new();
    hidden(&mut scope_fields);
    html! {
        div id="decision-actions" {
            @if s.recommendation.is_some() {
                form class="inline-form" data-on:submit=(format!("@post('/decisions/{id}/accept', {{contentType: 'form'}})")) {
                    @for (k, v) in &scope_fields { input type="hidden" name=(k) value=(v); }
                    button type="submit" class="btn" data-confirm="Click again to accept" {
                        "Accept recommendation"
                    }
                }
                div id="err-accept" {}
            }
            form class="stack-form" data-on:submit=(format!("@post('/decisions/{id}/answer', {{contentType: 'form'}})")) {
                @for (k, v) in &scope_fields { input type="hidden" name=(k) value=(v); }
                label class="field" {
                    span class="meta" { "Answer" }
                    textarea name="answer" rows="3" {}
                }
                label class="field" {
                    span class="meta" { "Rationale (optional)" }
                    textarea name="rationale" rows="3" {}
                }
                button type="submit" class="btn" { "Answer" }
            }
            div id="err-answer" {}
        }
    }
}

/// The whole page body.
fn body(conn: &Connection, scope: &Scope, q: &DecisionsQuery) -> Result<Markup> {
    let region = region(conn, scope, q)?;
    let queue = decision::open_queue_in(conn, &scope.filter)?.decisions;
    let forms = match selection(conn, &queue, q)? {
        Selection::Open(s) => actions(&scope.filter, &s),
        _ => html! { div id="decision-actions" {} },
    };
    let view: String = form_urlencoded::byte_serialize(live_view(scope, q).as_bytes()).collect();
    Ok(html! {
        h1 { "Decisions" }
        (picker::picker(scope, Route::Decisions.path()))
        div id="decisions" data-init=(format!("@get('/stream?view={view}')")) { (region) }
        (forms)
    })
}

fn live_view(scope: &Scope, q: &DecisionsQuery) -> String {
    href(&scope.filter, &q.pairs())
}

/// The live-view arm for `/decisions` and `/decisions?query`.
pub fn live(db: &FsPath, query: &str) -> Result<String> {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let conn = crate::db::open(db)?;
    let scope = scope::resolve(&conn, scope::filter_from_pairs(&pairs)?)?;
    let q = DecisionsQuery::parse(&pairs)?;
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
    let q = match DecisionsQuery::parse(&pairs) {
        Ok(q) => q,
        Err(e) => return WebError::new(e, "decision").into_response(),
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
                route: Route::Decisions,
                role,
                banner,
            },
            body,
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            PageCtx {
                route: Route::Decisions,
                role,
                banner: None,
            },
            components::error("Could not read the database", &unreadable(&e, &mut logged)),
        ),
    };
    (
        status,
        Html(layout::page(&ctx, "Decisions", markup).into_string()),
    )
        .into_response()
        .logged(logged)
}

/// The scope a write form carried, for the "next" link only. A bad one falls
/// back to every Project: the write itself never depends on it.
fn form_scope(f: &Form) -> ScopeFilter {
    scope::parse(
        f.has("p").then(|| f.get("p")),
        f.has("w").then(|| f.get("w")),
    )
    .unwrap_or(ScopeFilter::All)
}

/// How a form settles a Decision; owned so it can move into `blocking`.
enum How {
    Accept,
    Text(String),
}

/// Answer `id` as the human Identity and reply with the replacement for
/// `#decision-actions`.
async fn settle(
    state: AppState,
    id: String,
    body: Bytes,
    control: &'static str,
    how: How,
) -> Response {
    let f = Form::parse(&body);
    let scope = form_scope(&f);
    let rationale = f.opt("rationale");
    let db = state.db.clone();
    let who = state.identity.clone();
    let result = blocking(move || {
        let mut conn = crate::db::open(&db)?;
        let resolution = match &how {
            How::Accept => Resolution::Accept,
            How::Text(t) => Resolution::Text(t),
        };
        decision::answer(
            &mut conn,
            &ctx(),
            &id,
            resolution,
            rationale.as_deref(),
            &who,
        )?;
        let next = decision::open_queue_in(&conn, &scope)?
            .decisions
            .first()
            .map(qualified_id);
        Ok((id, scope, next))
    })
    .await;
    match result {
        Ok((id, scope, next)) => Html(
            html! {
                div id="decision-actions" {
                    p class="notice-hint" role="status" { "Answered " (id) "." }
                    @match next {
                        Some(n) => p { a href=(href(&scope, &[("d", n)])) { "Next open Decision" } },
                        None => p class="meta" { "No more open Decisions in this scope." },
                    }
                }
            }
            .into_string(),
        )
        .into_response(),
        Err(e) => WebError::new(e, control).into_response(),
    }
}

/// `POST /decisions/{id}/accept`: decide with the stored recommendation.
pub async fn accept(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    settle(state, id, body, "accept", How::Accept).await
}

/// `POST /decisions/{id}/answer`: decide with free text. A blank answer is the
/// library's `Usage` (400).
pub async fn answer(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let text = Form::parse(&body).get("answer").to_string();
    settle(state, id, body, "answer", How::Text(text)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(q: &str) -> Vec<(String, String)> {
        form_urlencoded::parse(q.as_bytes()).into_owned().collect()
    }

    #[test]
    fn query_reads_a_qualified_id_and_a_limit() {
        let q = DecisionsQuery::parse(&pairs("p=SCR&d=scr-d-3&limit=7")).unwrap();
        assert_eq!(q.d.as_deref(), Some("SCR-D-3"));
        assert_eq!(q.limit(), 7);
        assert_eq!(
            DecisionsQuery::parse(&pairs("")).unwrap(),
            DecisionsQuery::default()
        );
        assert_eq!(
            DecisionsQuery::parse(&pairs("d=&limit=")).unwrap(),
            DecisionsQuery::default()
        );
    }

    #[test]
    fn bare_and_malformed_ids_are_usage_errors() {
        for bad in [
            "d=D-3",
            "d=SCR-D-",
            "d=SCR-D-x",
            "d=S-D-1",
            "d=SCR-3",
            "d=a&d=b",
            "limit=0",
        ] {
            assert!(
                matches!(DecisionsQuery::parse(&pairs(bad)), Err(Error::Usage(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn href_carries_scope_and_selection() {
        assert_eq!(href(&ScopeFilter::All, &[]), "/decisions");
        assert_eq!(
            href(
                &ScopeFilter::Project { key: "A".into() },
                &[("d", "A-D-1".into())]
            ),
            "/decisions?p=A&d=A-D-1"
        );
    }

    #[test]
    fn query_pairs_round_trip() {
        let q = DecisionsQuery::parse(&pairs("d=SCR-D-2&limit=9")).unwrap();
        let again: Vec<(String, String)> = q
            .pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        assert_eq!(DecisionsQuery::parse(&again).unwrap(), q);
    }

    #[test]
    fn summary_escapes_and_picks_the_ticket_link_shape() {
        let evil = "<script>alert(1)</script>";
        let mut s = Summary {
            id: "SCR-D-1".into(),
            question: evil.into(),
            options: vec![evil.into()],
            recommendation: Some(evil.into()),
            tickets: vec!["SCR-1".into()],
        };
        let one = summary(&s).into_string();
        assert!(!one.contains("<script>"), "{one}");
        assert!(one.contains("Open ticket SCR-1"), "{one}");
        s.tickets.push("SCR-2".into());
        let many = summary(&s).into_string();
        assert!(many.contains("href=\"/ticket/SCR-2\""), "{many}");
        assert!(!many.contains("Open ticket"), "{many}");
    }
}
