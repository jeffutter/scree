//! The ticket detail screen (`/ticket/ID`), read-only: every section of
//! `scree show` plus what the ticket blocks. The page and the live stream
//! share [`region`], so they cannot disagree. The writes live in
//! `web/handlers.rs`; this file renders their buttons and forms. A ticket that is gone (removed
//! by another process, or never existed) renders an inline "gone" state
//! instead of a blank. Rendering reads only: no sweep, no git (docs/web-ui.md,
//! Sweep).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use maud::{Markup, html};
use rusqlite::Connection;

use crate::error::{Error, Result};
use crate::identity::Role;
use crate::model::{Blocked, Comment, Criterion, TicketDetail};
use crate::ticket::{self, Move};
use crate::web::AppState;
use crate::web::state::blocking;
use crate::web::view::components;
use crate::web::view::layout::{self, PageCtx, Route};
use crate::web::view::{self, markdown};

/// The text of the gone state; the live test looks for it.
pub const GONE: &str = "This ticket was removed or never existed";

fn gone() -> Markup {
    html! {
        div id="ticket" {
            (components::empty(GONE, "It may have been removed by another process."))
            p class="more" { a href="/" { "Back to the list" } }
        }
    }
}

/// An id that cannot name a ticket is the same as one that names none.
fn read(conn: &Connection, id: &str) -> Result<Option<(TicketDetail, Vec<Blocked>)>> {
    if id.is_empty() || id.contains('/') {
        return Ok(None);
    }
    let detail = match ticket::show(conn, id) {
        Ok(d) => d,
        Err(Error::NotFound(_) | Error::Usage(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    match ticket::blocks(conn, id) {
        Ok(b) => Ok(Some((detail, b))),
        // Removed between the two reads.
        Err(Error::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

fn ticket_link(id: &str) -> String {
    format!(
        "/ticket/{}",
        form_urlencoded::byte_serialize(id.as_bytes()).collect::<String>()
    )
}

/// The Worktree directory name, as the CLI text view shows it.
fn dir_name(full: &str) -> String {
    std::path::Path::new(full)
        .file_name()
        .map_or(full.to_string(), |n| n.to_string_lossy().into_owned())
}

fn prose(title: &str, src: &str) -> Markup {
    html! {
        @if !src.trim().is_empty() {
            section class="detail-section" {
                h2 { (title) }
                div class="prose" { (markdown::render(src)) }
            }
        }
    }
}

/// A form that posts to `/ticket/ID/ACTION` through Datastar. Each form has an
/// empty `err-CONTROL` slot beside it for the failure fragment.
fn post_form(id: &str, action: &str, control: &str, class: &str, inner: Markup) -> Markup {
    html! {
        form class=(class)
            data-on:submit=(format!("@post('{}/{}', {{contentType: 'form'}})", ticket_link(id), action)) {
            (inner)
        }
        div id=(format!("err-{control}")) {}
    }
}

fn button(label: &str) -> Markup {
    html! { button type="submit" class="btn" { (label) } }
}

/// A move that is not offered: the button stays visible but disabled, with
/// the reason as text so it is never a mystery.
fn disabled(label: &str, reason: &str) -> Markup {
    html! {
        div class="inline-form" {
            button type="button" class="btn" disabled { (label) }
            span class="meta" { (reason) }
        }
    }
}

fn why_not_start(d: &TicketDetail) -> String {
    let t = &d.ticket;
    match t.status.as_str() {
        "active" => match &t.claimed_by {
            Some(c) => format!("claimed by {}", c.author),
            None => "already claimed".into(),
        },
        "draft" => "a draft; make it ready first".into(),
        "done" | "abandoned" => "ticket is closed".into(),
        _ if t.blocked => "blocked".into(),
        _ => "not available to you".into(),
    }
}

/// The lifecycle moves, from [`ticket::available_actions`]. A move that is not
/// offered is shown disabled with its reason.
fn actions(d: &TicketDetail, role: Role) -> Markup {
    let t = &d.ticket;
    let id = t.id.as_str();
    let offered = ticket::available_actions(d, role);
    let has = |m: Move| offered.contains(&m);
    let closed = !has(Move::Done);
    let archive = if has(Move::Unarchive) {
        ("unarchive", "Unarchive")
    } else {
        ("archive", "Archive")
    };
    html! {
        section class="detail-section" id="actions" {
            h2 { "Actions" }
            @if has(Move::Ready) {
                (post_form(id, "ready", "ready", "inline-form", button("Make ready")))
            } @else {
                (disabled("Make ready", if t.status == "draft" { "not available" } else { "only a draft can be made ready" }))
            }
            @if has(Move::Start) {
                (post_form(id, "start", "start", "inline-form", button("Start")))
            } @else {
                (disabled("Start", &why_not_start(d)))
            }
            @if has(Move::Release) {
                (post_form(id, "release", "release", "stack-form", html! {
                    label class="field" {
                        span class="meta" { "Handoff note (optional; blank keeps the current one)" }
                        textarea name="handoff" rows="3" {}
                    }
                    (button("Release"))
                }))
            } @else {
                (disabled("Release", "only an active ticket can be released"))
            }
            @if has(Move::Done) {
                (post_form(id, "done", "done", "stack-form", html! {
                    label class="field" {
                        span class="meta" { "Commits to link (one revision per line, optional)" }
                        textarea name="commits" rows="2" {}
                    }
                    @if has(Move::ForceDone) {
                        label class="check" {
                            input type="checkbox" name="force" value="1";
                            "Force: finish even with unchecked criteria"
                        }
                    }
                    (button("Done"))
                }))
            } @else {
                (disabled("Done", "ticket is closed"))
            }
            @if has(Move::Abandon) {
                (post_form(id, "abandon", "abandon", "stack-form", html! {
                    label class="field" {
                        span class="meta" { "Reason (required)" }
                        input type="text" name="reason";
                    }
                    (button("Abandon"))
                }))
            } @else {
                (disabled("Abandon", "ticket is closed"))
            }
            @if has(Move::Reopen) {
                (post_form(id, "reopen", "reopen", "inline-form", button("Reopen")))
            } @else {
                (disabled("Reopen", "ticket is open"))
            }
            (post_form(id, archive.0, "archive", "inline-form", button(archive.1)))
            @if closed { p class="meta" { "This ticket is closed." } }
            (post_form(id, "commits", "commits", "stack-form", html! {
                label class="field" {
                    span class="meta" { "Link commits (one revision per line)" }
                    textarea name="commits" rows="2" {}
                }
                (button("Link commits"))
            }))
        }
    }
}

fn criterion(id: &str, c: &Criterion) -> Markup {
    html! {
        li class="criterion" {
            span class=(if c.checked { "check-mark check-done" } else { "check-mark" }) {
                (if c.checked { "[x]" } else { "[ ]" })
            }
            " " span class="criterion-ref" { "#" (c.number) }
            " " span class="criterion-text" { (c.text) }
            @if let Some(ev) = &c.evidence {
                div class="criterion-evidence" {
                    span class="meta" { "Evidence: " }
                    (ev)
                }
            }
            @if c.checked {
                div class="meta" {
                    "checked"
                    @if let Some(by) = &c.checked_by { " by " (by.author) }
                    @if let Some(at) = &c.checked_at { " at " (at) }
                }
            }
            @let n = c.number.to_string();
            @let control = format!("criterion-{n}");
            @if c.checked {
                (post_form(id, &format!("criteria/{n}/uncheck"), &control, "inline-form", button("Uncheck")))
            } @else {
                (post_form(id, &format!("criteria/{n}/check"), &control, "inline-form", html! {
                    label class="field" {
                        span class="meta" { "Evidence (optional)" }
                        input type="text" name="evidence";
                    }
                    (button("Check"))
                }))
            }
            (post_form(id, &format!("criteria/{n}/remove"), &format!("criterion-rm-{n}"), "inline-form", button("Remove")))
        }
    }
}

fn comment(id: &str, expected: &str, c: &Comment) -> Markup {
    html! {
        li class="comment" id=(format!("comment-{}", c.id)) {
            div class="meta" {
                span class="comment-author" { (c.author) }
                @if let Some(r) = &c.role { " (" (r) ")" }
                @if let Some(s) = &c.session { " session " (s) }
                " " (c.created_at)
                @if let Some(e) = &c.edited_at { " - edited " (e) }
            }
            div class="prose" { (markdown::render(&c.body)) }
            details class="comment-edit" {
                summary { "Edit or delete" }
                // The ticket's updated_at as rendered here: the edit is
                // refused (409) when the ticket changed since.
                (post_form(id, &format!("comments/{}/edit", c.id), &format!("comment-{}", c.id), "stack-form", html! {
                    input type="hidden" name="expected_updated_at" value=(expected);
                    textarea name="body" rows="4" { (c.body) }
                    (button("Save"))
                }))
                (post_form(id, &format!("comments/{}/delete", c.id), &format!("comment-del-{}", c.id), "inline-form", button("Delete")))
            }
        }
    }
}

fn related(id: &str, status: &str, title: &str, worktree: Option<&str>, note: &str) -> Markup {
    html! {
        li class="related" {
            a class="row-id" href=(ticket_link(id)) { (id) }
            " " span class="meta" { (status) }
            " " span { (title) }
            @if let Some(w) = worktree { " " span class="meta" { "in " (dir_name(w)) } }
            @if !note.is_empty() { " " span class="meta" { (note) } }
        }
    }
}

/// Every section of the detail, as the live region.
pub fn region(conn: &Connection, id: &str, role: Role) -> Result<Markup> {
    let Some((d, blocks)) = read(conn, id)? else {
        return Ok(gone());
    };
    let t = &d.ticket;
    let checked = d.criteria.iter().filter(|c| c.checked).count();
    let worktree = t
        .worktree
        .as_deref()
        .map_or("root checkout".to_string(), dir_name);
    Ok(html! {
        div id="ticket" {
            header class="detail-head" {
                p class="meta" { a href="/" { "Tickets" } " / " (t.id) }
                h1 class="detail-title" { span class="row-id" { (t.id) } " " (t.title) }
                p class="detail-facts" {
                    span class=(format!("status status-{}", t.status)) { (t.status) }
                    " " span class="tag" { "for " (t.for_who) }
                    @if t.available { " " span class="tag" { "available" } }
                    @if t.blocked { " " span class="tag" { "blocked" } }
                    @if t.archived_at.is_some() { " " span class="tag" { "archived" } }
                }
                p class="meta" {
                    "Project " (t.project.key) ", " (worktree)
                    @if let Some(c) = &t.claimed_by {
                        " - claimed by " (c.author)
                        @if let Some(at) = &t.claim_touched_at { ", last touched " (at) }
                    }
                }
                p class="meta" {
                    "created " (t.created_at) ", updated " (t.updated_at)
                    @if let Some(at) = &t.done_at { ", done " (at) }
                    @if let Some(at) = &t.archived_at { ", archived " (at) }
                }
                @if let Some(p) = &t.parent {
                    p class="meta" { "Subtask of " a href=(ticket_link(p)) { (p) } }
                }
            }
            (actions(&d, role))
            (prose("Description", &d.description))
            (prose("Plan", &d.plan))
            (prose("Handoff", &d.handoff))
            section class="detail-section" {
                h2 { "Criteria" }
                @if d.criteria.is_empty() {
                    p class="meta" { "No criteria." }
                } @else {
                    p class="meta" { (checked) " of " (d.criteria.len()) " checked" }
                    ul class="plain" { @for c in &d.criteria { (criterion(&t.id, c)) } }
                }
                (post_form(&t.id, "criteria", "criterion-add", "stack-form", html! {
                    label class="field" {
                        span class="meta" { "Add a criterion" }
                        input type="text" name="text";
                    }
                    (button("Add criterion"))
                }))
            }
            @if !t.blocked_by.is_empty() || !t.blocking_decisions.is_empty() {
                section class="detail-section" {
                    h2 { "Blocked by" }
                    ul class="plain" {
                        @for b in &t.blocked_by {
                            (related(&b.id, &b.status, &b.title, b.worktree.as_deref(), ""))
                        }
                        @for dec in &t.blocking_decisions {
                            li class="related" {
                                span class="row-id" { (dec.id) }
                                " " span class="meta" { "open Decision" }
                                " " span { (dec.question) }
                            }
                        }
                    }
                }
            }
            @if !blocks.is_empty() {
                section class="detail-section" {
                    h2 { "Blocks" }
                    ul class="plain" {
                        @for b in &blocks {
                            (related(&b.id, &b.status, &b.title, b.worktree.as_deref(),
                                if b.holding { "" } else { "no longer holding" }))
                        }
                    }
                }
            }
            @if !t.subtasks.is_empty() {
                section class="detail-section" {
                    h2 { "Subtasks" }
                    ul class="plain" {
                        @for s in &t.subtasks { (related(&s.id, &s.status, &s.title, None, "")) }
                    }
                }
            }
            @if !d.decisions.is_empty() {
                section class="detail-section" {
                    h2 { "Decisions" }
                    ul class="plain" {
                        @for dec in &d.decisions {
                            li class="related" {
                                span class="row-id" { (dec.id) }
                                " " span class="meta" { (dec.status) }
                                " " span { (dec.question) }
                                @if let Some(a) = &dec.answer {
                                    div class="criterion-evidence" {
                                        span class="meta" { "Answer: " } (a)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            @if !d.commits.is_empty() {
                section class="detail-section" {
                    h2 { "Commits" }
                    ul class="plain" {
                        @for c in &d.commits {
                            li class="related" {
                                code { (c.sha.chars().take(10).collect::<String>()) }
                                @if let Some(b) = &c.branch { " " span class="meta" { (b) } }
                                " " span class="meta" { "linked " (c.linked_at) }
                            }
                        }
                    }
                }
            }
            section class="detail-section" {
                h2 { "Comments (" (d.comments.len()) ")" }
                @if d.comments.is_empty() {
                    p class="meta" { "No comments." }
                } @else {
                    ul class="plain" { @for c in &d.comments { (comment(&t.id, &t.updated_at, c)) } }
                }
                (post_form(&t.id, "comments", "comment-add", "stack-form", html! {
                    label class="field" {
                        span class="meta" { "Add a comment" }
                        textarea name="body" rows="3" {}
                    }
                    (button("Add comment"))
                }))
            }
        }
    })
}

/// The live-view arm for `/ticket/ID`: a gone ticket is a normal fragment so
/// an open page shows it in place.
pub fn live(db: &std::path::Path, id: &str) -> Result<String> {
    let conn = crate::db::open(db)?;
    Ok(region(&conn, id, Role::Human)?.into_string())
}

pub async fn show(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let db = state.db.clone();
    let role_kind = state.identity.role;
    let role = role_kind.as_str();
    let want = id.clone();
    let result = blocking(move || {
        let conn = crate::db::open(&db)?;
        let banner = view::banner_counts(&conn)?;
        let found = read(&conn, &want)?.is_some();
        let region = region(&conn, &want, role_kind)?;
        Ok((banner, found, region))
    })
    .await;
    let (status, banner, title, body) = match result {
        Ok((banner, found, region)) => {
            let status = if found {
                StatusCode::OK
            } else {
                StatusCode::NOT_FOUND
            };
            let title = if found {
                id.clone()
            } else {
                "Not found".into()
            };
            let view: String =
                form_urlencoded::byte_serialize(format!("/ticket/{id}").as_bytes()).collect();
            let body = html! {
                div id="ticket-live" data-init=(format!("@get('/stream?view={view}')")) {
                    (region)
                }
            };
            (status, banner, title, body)
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            "Error".into(),
            components::error("Could not read the database", &e.to_string()),
        ),
    };
    let ctx = PageCtx {
        route: Route::List,
        role,
        banner,
    };
    (status, Html(layout::page(&ctx, &title, body).into_string())).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{ForWho, NewStatus};
    use crate::ticket::NewTicket;

    fn db() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        crate::db::open(&path).unwrap();
        (dir, path)
    }

    fn add(c: &mut Connection, title: &str, desc: &str, plan: &str, crit: &[String]) -> String {
        let t = ticket::add(
            c,
            &crate::scope::Scope::Unscoped,
            &NewTicket {
                title,
                description: desc,
                plan,
                criteria: crit,
                for_who: ForWho::Agent,
                status: NewStatus::Ready,
                key: None,
                parent: None,
                blocked_by: &[],
            },
        )
        .unwrap();
        t.ticket.id
    }

    #[test]
    fn missing_ticket_is_the_gone_state() {
        let (_d, p) = db();
        for id in ["LOCAL-99", "nonsense", "", "a/b"] {
            let html = live(&p, id).unwrap();
            assert!(html.contains(GONE), "{id}: {html}");
        }
    }

    #[test]
    fn hostile_text_is_inert() {
        let (_d, p) = db();
        let mut c = crate::db::open(&p).unwrap();
        let evil = "<script>alert(1)</script> [x](javascript:alert(1))";
        let id = add(
            &mut c,
            "<script>alert(1)</script>",
            evil,
            evil,
            &["<img src=x onerror=alert(1)>".to_string()],
        );
        let html = live(&p, &id).unwrap();
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(!html.contains("href=\"javascript"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
    }

    #[test]
    fn a_plain_ticket_shows_its_header_and_criteria() {
        let (_d, p) = db();
        let mut c = crate::db::open(&p).unwrap();
        let id = add(
            &mut c,
            "Title here",
            "the description",
            "the plan",
            &["first".to_string(), "second".to_string()],
        );
        let html = live(&p, &id).unwrap();
        for want in [
            "Title here",
            "the description",
            "the plan",
            "0 of 2 checked",
            "first",
            "second",
            "root checkout",
            "Comments (0)",
        ] {
            assert!(html.contains(want), "{want}: {html}");
        }
        assert!(!html.contains("Blocked by"), "{html}");
    }
}
