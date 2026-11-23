//! The Board (`/board`): the tickets of the scope as five columns (draft,
//! ready, active, done, abandoned). A column with no tickets collapses to a
//! labelled strip. A card shows the id, title, who it is for, why it is
//! blocked, subtask progress, criteria progress, the claimer and a stale
//! cue; every state is text, never colour alone.
//!
//! There is no drag. A card carries one-click buttons for the moves the
//! status allows, each posting to `POST /board/ID/MOVE`, which calls the same
//! `actions::*` the ticket detail uses; the library's rules decide and a
//! refusal comes back as an inline 422 in `err-board-ID`. Abandon (needs a
//! reason) and force-done stay on the ticket detail. The page and the live
//! stream share [`region`]; a successful move changes the database and the
//! stream patches the board, so the move's own answer is only an empty slot.

use std::path::Path as FsPath;

use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use clap::ValueEnum;
use maud::{Markup, html};
use rusqlite::Connection;

use crate::actions;
use crate::cli::{ForWho, ListArgs, Status};
use crate::error::{Error, Result};
use crate::identity::{Identity, Role};
use crate::model::Ticket;
use crate::ticket::{self, ScopeFilter};
use crate::web::AppState;
use crate::web::error::WebError;
use crate::web::pages::list::MAX_LIMIT;
use crate::web::scope::{self, Scope, Scoped};
use crate::web::state::blocking;
use crate::web::view::components::{self, FIRST_RENDER_ROWS};
use crate::web::view::layout::{self, PageCtx, Route};
use crate::web::view::{self, picker, ticket_row};

/// The parts of a Board URL beyond the scope.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BoardQuery {
    pub for_who: Option<ForWho>,
    /// Per-column row limit, in `Status::ALL` order; `None` is the first-render cap.
    pub limits: [Option<usize>; 5],
}

fn usage(msg: impl Into<String>) -> Error {
    Error::Usage(msg.into())
}

impl BoardQuery {
    /// Read `for` and the `limit_STATUS` parameters; `p`, `w` and anything
    /// unknown are ignored. A repeated or bad value is [`Error::Usage`].
    pub fn parse(pairs: &[(String, String)]) -> Result<BoardQuery> {
        let one = |name: &str| -> Result<Option<&str>> {
            let mut found = pairs.iter().filter(|(k, _)| k == name);
            let first = found.next().map(|(_, v)| v.as_str());
            if found.next().is_some() {
                return Err(usage(format!("{name} given twice")));
            }
            Ok(first.filter(|v| !v.trim().is_empty()))
        };
        let for_who = one("for")?
            .map(|v| {
                ForWho::from_str(v.trim(), true).map_err(|_| usage(format!("unknown for {v:?}")))
            })
            .transpose()?;
        let mut limits = [None; 5];
        for (slot, s) in limits.iter_mut().zip(Status::ALL) {
            let name = limit_key(s);
            *slot = one(name)?
                .map(|v| match v.trim().parse::<usize>() {
                    Ok(n) if n >= 1 => Ok(n.min(MAX_LIMIT)),
                    _ => Err(usage(format!(
                        "{name} must be a positive number, not {v:?}"
                    ))),
                })
                .transpose()?;
        }
        Ok(BoardQuery { for_who, limits })
    }

    fn limit(&self, s: Status) -> usize {
        self.limits[index(s)].unwrap_or(FIRST_RENDER_ROWS)
    }

    fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut v = Vec::new();
        if let Some(f) = self.for_who {
            v.push(("for", f.as_str().to_string()));
        }
        for (l, s) in self.limits.iter().zip(Status::ALL) {
            if let Some(n) = l {
                v.push((limit_key(s), n.to_string()));
            }
        }
        v
    }
}

fn index(s: Status) -> usize {
    Status::ALL.iter().position(|x| *x == s).unwrap_or(0)
}

fn limit_key(s: Status) -> &'static str {
    match s {
        Status::Draft => "limit_draft",
        Status::Ready => "limit_ready",
        Status::Active => "limit_active",
        Status::Done => "limit_done",
        Status::Abandoned => "limit_abandoned",
    }
}

/// `/board` plus the query made of the scope and extra pairs.
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
        Route::Board.path().into()
    } else {
        format!("{}?{q}", Route::Board.path())
    }
}

/// A move a card offers: one lifecycle action of `actions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Ready,
    Draft,
    Start,
    Release,
    Done,
    Reopen,
}

impl Move {
    pub fn parse(s: &str) -> Option<Move> {
        Some(match s {
            "ready" => Move::Ready,
            "draft" => Move::Draft,
            "start" => Move::Start,
            "release" => Move::Release,
            "done" => Move::Done,
            "reopen" => Move::Reopen,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Move::Ready => "ready",
            Move::Draft => "draft",
            Move::Start => "start",
            Move::Release => "release",
            Move::Done => "done",
            Move::Reopen => "reopen",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Move::Ready => "Ready",
            Move::Draft => "Draft",
            Move::Start => "Start",
            Move::Release => "Release",
            Move::Done => "Done",
            Move::Reopen => "Reopen",
        }
    }

    /// The moves offered from a status. Advisory: the library still decides.
    /// Start is offered only on an Available ticket.
    fn offered(status: &str, available: bool) -> Vec<Move> {
        match status {
            "draft" => vec![Move::Ready],
            "ready" if available => vec![Move::Start, Move::Draft],
            "ready" => vec![Move::Draft],
            "active" => vec![Move::Release, Move::Done],
            "done" | "abandoned" => vec![Move::Reopen],
            _ => vec![],
        }
    }

    fn run(self, conn: &mut Connection, id: &str, who: &Identity) -> Result<()> {
        match self {
            Move::Ready => actions::ready(conn, id, who),
            Move::Draft => actions::draft(conn, id, who),
            Move::Start => actions::start(conn, id, who),
            Move::Release => actions::release(conn, id, None, who),
            Move::Done => actions::done(conn, id, false, vec![], who),
            Move::Reopen => actions::reopen(conn, id, who),
        }
        .map(drop)
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

/// An active ticket whose claim was last touched before `cutoff`. Both are
/// RFC 3339 UTC strings in the same format, so they compare as text.
fn is_stale(t: &Ticket, cutoff: &str) -> bool {
    t.status == "active"
        && t.claim_touched_at
            .as_deref()
            .is_some_and(|touched| touched < cutoff)
}

fn card(t: &Ticket, cutoff: &str) -> Markup {
    let done_subtasks = t.subtasks.iter().filter(|s| s.status == "done").count();
    let blocked = t
        .blocked
        .then(|| ticket_row::blocked_by(t).unwrap_or_else(|| "[blocked]".into()));
    let moves = Move::offered(&t.status, t.available);
    html! {
        li class="card" data-id=(t.id) {
            div class="card-head" {
                a class="row-id" href=(format!("/ticket/{}", t.id)) { (t.id) }
                @if t.for_who == "human" { span class="tag" { "human" } }
            }
            p class="card-title" { (t.title) }
            @if let Some(b) = blocked { p class="row-blocked" { (b) } }
            p class="card-meta" {
                @if !t.subtasks.is_empty() {
                    span { (done_subtasks) "/" (t.subtasks.len()) " subtasks" }
                }
                @if t.criteria.total > 0 {
                    span { "criteria " (t.criteria.checked) "/" (t.criteria.total) }
                }
                @if let Some(c) = &t.claimed_by {
                    span { "claimed by " (c.author) }
                }
                @if is_stale(t, cutoff) { span class="tag" { "stale claim" } }
            }
            @if !moves.is_empty() {
                div class="card-moves" {
                    @for m in moves {
                        form data-on:submit=(format!(
                            "@post('/board/{}/{}', {{contentType: 'form'}})", t.id, m.as_str())) {
                            button type="submit" class="btn" { (m.label()) }
                        }
                    }
                }
            }
            div id=(format!("err-board-{}", t.id)) {}
        }
    }
}

fn column_title(s: Status) -> &'static str {
    match s {
        Status::Draft => "Draft",
        Status::Ready => "Ready",
        Status::Active => "Active",
        Status::Done => "Done",
        Status::Abandoned => "Abandoned",
    }
}

/// This Board's URL with column `s` asking for twice as many rows.
fn more_href(scope: &Scope, q: &BoardQuery, s: Status) -> String {
    let mut extra: Vec<(&str, String)> = q
        .pairs()
        .into_iter()
        .filter(|(k, _)| *k != limit_key(s))
        .collect();
    extra.push((limit_key(s), (q.limit(s) * 2).min(MAX_LIMIT).to_string()));
    href(&scope.filter, &extra)
}

/// The live part: five columns, or one empty state for an empty scope.
pub fn region(conn: &Connection, scope: &Scope, q: &BoardQuery) -> Result<Markup> {
    let who = identity();
    let cutoff = ticket::stale_cutoff(conn, None)?;
    let mut cols = Vec::new();
    for s in Status::ALL {
        let args = ListArgs {
            status: vec![s],
            for_who: q.for_who,
            limit: Some(u32::try_from(q.limit(s)).unwrap_or(u32::MAX)),
            ..ListArgs::default()
        };
        let tickets = ticket::list_in(conn, &scope.filter, &args, &who)?.tickets;
        let total = usize::try_from(ticket::count_in(conn, &scope.filter, &args, &who)?)
            .unwrap_or(usize::MAX)
            .max(tickets.len());
        cols.push((s, tickets, total));
    }
    if cols.iter().all(|(_, _, total)| *total == 0) {
        return Ok(html! {
            div id="board-region" {
                (components::empty(
                    "No tickets here",
                    "This scope has no tickets. Create one with: scree add \"Title\""))
            }
        });
    }
    Ok(html! {
        div id="board-region" class="board" {
            @for (s, tickets, total) in &cols {
                @if *total == 0 {
                    section class="board-col board-col-empty" data-empty aria-label=(column_title(*s)) {
                        h2 class="board-head" {
                            (column_title(*s)) " " span class="board-count" { "0" }
                        }
                    }
                } @else {
                    section class="board-col" aria-label=(column_title(*s)) {
                        h2 class="board-head" {
                            (column_title(*s)) " " span class="board-count" { (total) }
                        }
                        ul class="cards" {
                            @for t in tickets { (card(t, &cutoff)) }
                        }
                        (components::show_more(tickets.len(), *total, &more_href(scope, q, *s)))
                    }
                }
            }
        }
    })
}

fn filters(scope: &Scope, q: &BoardQuery) -> Markup {
    html! {
        form class="filters" method="get" action=(Route::Board.path()) role="search" {
            @for (k, v) in scope::pairs(&scope.filter) { input type="hidden" name=(k) value=(v); }
            label class="field" {
                span class="picker-label" { "For" }
                select name="for" {
                    option value="" selected[q.for_who.is_none()] { "anyone" }
                    option value="agent" selected[q.for_who == Some(ForWho::Agent)] { "agent" }
                    option value="human" selected[q.for_who == Some(ForWho::Human)] { "human" }
                }
            }
            button type="submit" class="btn" { "Filter" }
        }
    }
}

fn body(conn: &Connection, scope: &Scope, q: &BoardQuery) -> Result<Markup> {
    let region = region(conn, scope, q)?;
    let view: String =
        form_urlencoded::byte_serialize(href(&scope.filter, &q.pairs()).as_bytes()).collect();
    Ok(html! {
        h1 { "Board" }
        (picker::picker(scope, Route::Board.path()))
        (filters(scope, q))
        div id="board" data-init=(format!("@get('/stream?view={view}')")) { (region) }
    })
}

/// The live-view arm for `/board` and `/board?query`.
pub fn live(db: &FsPath, query: &str) -> Result<String> {
    let pairs: Vec<(String, String)> = form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    let conn = crate::db::open(db)?;
    let scope = scope::resolve(&conn, scope::filter_from_pairs(&pairs)?)?;
    let q = BoardQuery::parse(&pairs)?;
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
    let q = match BoardQuery::parse(&pairs) {
        Ok(q) => q,
        Err(e) => return WebError::new(e, "board").into_response(),
    };
    let db = state.db.clone();
    let role = state.identity.role.as_str();
    let result = blocking(move || {
        let conn = crate::db::open(&db)?;
        let banner = view::banner_counts(&conn)?;
        Ok((banner, body(&conn, &scope, &q)?))
    })
    .await;
    let (status, ctx, markup) = match result {
        Ok((banner, body)) => (
            StatusCode::OK,
            PageCtx {
                route: Route::Board,
                role,
                banner,
            },
            body,
        ),
        Err(e @ (Error::Usage(_) | Error::NotFound(_))) => {
            return WebError::new(e, "board").into_response();
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            PageCtx {
                route: Route::Board,
                role,
                banner: None,
            },
            components::error("Could not read the database", &e.to_string()),
        ),
    };
    (
        status,
        Html(layout::page(&ctx, "Board", markup).into_string()),
    )
        .into_response()
}

/// `POST /board/{id}/{mv}`: run one move as the web's human Identity. Success
/// answers with the card's empty error slot; the live stream carries the new
/// board. A failure names `err-board-ID`.
pub async fn move_card(
    State(state): State<AppState>,
    Path((id, mv)): Path<(String, String)>,
) -> Response {
    let control = format!("board-{id}");
    let db = state.db.clone();
    let who = state.identity.clone();
    let slot = control.clone();
    let result = blocking(move || {
        let Some(mv) = Move::parse(&mv) else {
            return Err(Error::NotFound(format!("no move {mv}")));
        };
        let mut conn = crate::db::open(&db)?;
        mv.run(&mut conn, &id, &who)
    })
    .await;
    match result {
        Ok(()) => Html(html! { div id=(format!("err-{slot}")) {} }.into_string()).into_response(),
        Err(e) => WebError::new(e, &control).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Claimer, CriteriaSummary, Subtask};

    fn pairs(q: &str) -> Vec<(String, String)> {
        form_urlencoded::parse(q.as_bytes()).into_owned().collect()
    }

    fn ticket(status: &str) -> Ticket {
        Ticket {
            archived_at: None,
            available: false,
            blocked: false,
            blocked_by: vec![],
            blocking_decisions: vec![],
            claim_touched_at: None,
            claimed_by: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            criteria: CriteriaSummary::default(),
            done_at: None,
            for_who: "agent".into(),
            id: "SCR-1".into(),
            parent: None,
            project: Default::default(),
            status: status.into(),
            subtasks: vec![],
            title: "A <b>title</b>".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            worktree: None,
        }
    }

    #[test]
    fn query_reads_for_and_column_limits() {
        let q = BoardQuery::parse(&pairs("p=SCR&for=human&limit_done=7&x=1")).unwrap();
        assert_eq!(q.for_who, Some(ForWho::Human));
        assert_eq!(q.limit(Status::Done), 7);
        assert_eq!(q.limit(Status::Ready), FIRST_RENDER_ROWS);
        assert_eq!(
            BoardQuery::parse(&pairs("for=&limit_done=")).unwrap(),
            BoardQuery::default()
        );
    }

    #[test]
    fn query_round_trips_and_rejects_bad_values() {
        let q = BoardQuery::parse(&pairs("for=agent&limit_draft=3&limit_abandoned=9")).unwrap();
        let again: Vec<(String, String)> = q
            .pairs()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        assert_eq!(BoardQuery::parse(&again).unwrap(), q);
        for bad in [
            "for=a&for=b",
            "for=robot",
            "limit_done=0",
            "limit_ready=x",
            "limit_ready=1&limit_ready=2",
        ] {
            assert!(
                matches!(BoardQuery::parse(&pairs(bad)), Err(Error::Usage(_))),
                "{bad}"
            );
        }
        assert_eq!(
            href(
                &ScopeFilter::Project { key: "A".into() },
                &[("limit_done", "9".into())]
            ),
            "/board?p=A&limit_done=9"
        );
    }

    #[test]
    fn stale_cue_flips_at_the_cutoff() {
        let mut t = ticket("active");
        t.claim_touched_at = Some("2026-01-01T10:00:00Z".into());
        assert!(is_stale(&t, "2026-01-01T10:00:01Z"));
        assert!(!is_stale(&t, "2026-01-01T10:00:00Z"));
        assert!(!is_stale(&t, "2026-01-01T09:00:00Z"));
        t.status = "ready".into();
        assert!(!is_stale(&t, "2030-01-01T00:00:00Z"));
    }

    #[test]
    fn moves_parse_and_are_offered_by_status() {
        for m in [
            Move::Ready,
            Move::Draft,
            Move::Start,
            Move::Release,
            Move::Done,
            Move::Reopen,
        ] {
            assert_eq!(Move::parse(m.as_str()), Some(m));
        }
        assert_eq!(Move::parse("abandon"), None);
        assert_eq!(Move::offered("ready", true), [Move::Start, Move::Draft]);
        assert_eq!(Move::offered("ready", false), [Move::Draft]);
        assert_eq!(Move::offered("active", false), [Move::Release, Move::Done]);
        assert_eq!(Move::offered("abandoned", false), [Move::Reopen]);
    }

    #[test]
    fn card_shows_facts_as_text_and_escapes() {
        let mut t = ticket("active");
        t.for_who = "human".into();
        t.blocked = true;
        t.subtasks = vec![
            Subtask {
                id: "SCR-2".into(),
                status: "done".into(),
                title: "a".into(),
            },
            Subtask {
                id: "SCR-3".into(),
                status: "ready".into(),
                title: "b".into(),
            },
        ];
        t.criteria = CriteriaSummary {
            checked: 1,
            total: 3,
        };
        t.claimed_by = Some(Claimer {
            author: "ann".into(),
            session: None,
        });
        t.claim_touched_at = Some("2020-01-01T00:00:00Z".into());
        let h = card(&t, "2026-01-01T00:00:00Z").into_string();
        for want in [
            "data-id=\"SCR-1\"",
            "human",
            "[blocked]",
            "1/2 subtasks",
            "criteria 1/3",
            "claimed by ann",
            "stale claim",
            "id=\"err-board-SCR-1\"",
            "@post('/board/SCR-1/release'",
            "A &lt;b&gt;title&lt;/b&gt;",
        ] {
            assert!(h.contains(want), "{want}: {h}");
        }
        assert!(!h.contains("<b>"), "{h}");
    }
}
