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

use crate::cli::Status;
use crate::error::{Error, Result};
use crate::identity::Role;
use crate::model::{Blocked, Blocker, Comment, Criterion, TicketDetail};
use crate::ticket::{self, Move, SiblingRef};
use crate::web::AppState;
use crate::web::error::{LoggedExt, unreadable};
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
    post_form_id("", id, action, control, class, inner)
}

/// [`post_form`] with an element id on the form (none when `form_id` is empty).
fn post_form_id(
    form_id: &str,
    id: &str,
    action: &str,
    control: &str,
    class: &str,
    inner: Markup,
) -> Markup {
    html! {
        form class=(class) id=[(!form_id.is_empty()).then_some(form_id)]
            data-on:submit=(format!("@post('{}/{}', {{contentType: 'form'}})", ticket_link(id), action)) {
            (inner)
        }
        div id=(format!("err-{control}")) {}
    }
}

fn button(label: &str) -> Markup {
    html! { button type="submit" class="btn" { (label) } }
}

/// A submit button whose accessible name differs from its visible text (the
/// visible text stays a prefix, so voice control still matches).
fn button_labeled(label: &str, aria: &str) -> Markup {
    html! { button type="submit" class="btn" aria-label=(aria) { (label) } }
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
                    textarea name="body" rows="4" aria-label=(format!("Edit comment {}", c.id)) { (c.body) }
                    (button_labeled("Save", &format!("Save comment {}", c.id)))
                }))
                (post_form(id, &format!("comments/{}/delete", c.id), &format!("comment-del-{}", c.id), "inline-form", button_labeled("Delete", &format!("Delete comment {}", c.id))))
            }
        }
    }
}

fn blocker_row(id: &str, b: &Blocker) -> Markup {
    let blocker = form_urlencoded::byte_serialize(b.id.as_bytes()).collect::<String>();
    html! {
        li class="related" {
            a class="row-id" href=(ticket_link(&b.id)) { (b.id) }
            " " span class="meta" { (b.status) }
            " " span { (b.title) }
            @if let Some(w) = b.worktree.as_deref() { " " span class="meta" { "in " (dir_name(w)) } }
            (post_form(id, &format!("blockers/{blocker}/remove"), &format!("blocker-rm-{}", b.id), "inline-form", button("Remove blocker")))
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
            (position_note(&d))
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
                            (blocker_row(&t.id, b))
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

/// The edit form. It is not part of [`region`]: the live stream re-patches
/// the region on every database change, and a form inside it would lose the
/// text being typed. `expected_updated_at` is the `updated_at` the form was
/// rendered with; the form is only re-rendered by the page and by a
/// successful edit, so a change made by anyone else in between makes the save
/// a 409 and the typed text stays in place.
pub fn edit_form(d: &TicketDetail) -> Markup {
    let t = &d.ticket;
    let who_option = |v: &str| {
        html! { option value=(v) selected[t.for_who == v] { (v) } }
    };
    html! {
        div id="ticket-edit" {
            h2 { "Edit" }
            (post_form(&t.id, "edit", "edit", "stack-form", html! {
                input type="hidden" name="expected_updated_at" value=(t.updated_at);
                label class="field" {
                    span class="meta" { "Title" }
                    input type="text" name="title" value=(t.title);
                }
                label class="field" {
                    span class="meta" { "Description (Markdown)" }
                    textarea name="description" rows="6" { (d.description) }
                }
                (preview_button(&t.id, "description"))
                div id="preview-description" {}
                label class="field" {
                    span class="meta" { "Plan (Markdown)" }
                    textarea name="plan" rows="8" { (d.plan) }
                }
                (preview_button(&t.id, "plan"))
                div id="preview-plan" {}
                label class="field" {
                    span class="meta" { "For" }
                    select name="for" { (who_option("agent")) (who_option("human")) }
                }
                label class="field" {
                    span class="meta" { "Parent (ticket id; blank keeps the current one)" }
                    input type="text" name="parent" value=(t.parent.as_deref().unwrap_or(""));
                }
                label class="check" {
                    input type="checkbox" name="detach" value="1";
                    "Make top-level (no parent)"
                }
                p class="meta" {
                    "Saved only if the ticket has not changed since this form was loaded. "
                    "If it has, your text stays here; copy it, then reload the page."
                }
                (button("Save"))
            }))
        }
    }
}

/// Renders the text of one field as Markdown beside the editor. Writes nothing.
fn preview_button(id: &str, field: &str) -> Markup {
    html! {
        button type="button" class="btn"
            data-on:click=(format!("@post('{}/preview?field={}', {{contentType: 'form'}})", ticket_link(id), field)) {
            "Preview " (field)
        }
    }
}

/// The preview fragment for one field.
pub fn preview(field: &str, src: &str) -> Markup {
    html! {
        div id=(format!("preview-{field}")) class="preview" {
            @if src.trim().is_empty() {
                p class="meta" { "Nothing to preview." }
            } @else {
                div class="prose" { (markdown::render(src)) }
            }
        }
    }
}

/// What Position means for this ticket. In the live region, because whether a
/// root ticket is Blocked changes while the page is open.
fn position_note(d: &TicketDetail) -> Markup {
    let t = &d.ticket;
    html! {
        p class="meta" id="position-note" {
            @if t.parent.is_some() {
                "Position is the running order among its siblings."
            } @else {
                "Position only orders Available root tickets."
                @if t.blocked {
                    " This ticket is Blocked, so moving it will not change what scree next picks until it is Available."
                }
            }
        }
    }
}

/// The "Move before/after" form, or the reason there is none. Rendered with
/// the tools (outside the live-patched region) so a chosen anchor survives.
/// Lists at most [`PICKER_ROWS`] siblings, in position order.
pub fn move_section(d: &TicketDetail, siblings: &[SiblingRef]) -> Markup {
    let t = &d.ticket;
    let id = t.id.as_str();
    html! {
        div id="ticket-move" {
            h2 { "Move before or after" }
            @if Status::is_settled(&t.status) {
                p class="meta" { "A " (t.status) " ticket keeps its Position and cannot be moved." }
            } @else if siblings.is_empty() {
                (components::empty("No siblings", "Nothing to move this ticket before or after."))
            } @else {
                (post_form(id, "move", "move", "stack-form", html! {
                    label class="field" {
                        span class="meta" { "Neighbour" }
                        select name="anchor" {
                            @for s in siblings.iter().take(PICKER_ROWS) {
                                option value=(s.id) { (s.id) " (" (s.status) ") " (s.title) }
                            }
                        }
                    }
                    @if siblings.len() > PICKER_ROWS {
                        p class="meta" { "showing " (PICKER_ROWS) " of " (siblings.len()) " siblings" }
                    }
                    fieldset class="filter-group" {
                        legend class="meta" { "Place" }
                        label class="check" { input type="radio" name="where" value="before" checked; " Before the neighbour" }
                        label class="check" { input type="radio" name="where" value="after"; " After the neighbour" }
                    }
                    (button_labeled("Move", &format!("Move {id}")))
                }))
            }
        }
    }
}

/// [`move_section`] from a fresh read; the answer to a successful move.
pub fn move_section_read(conn: &Connection, id: &str) -> Result<Markup> {
    let d = ticket::show(conn, id)?;
    let siblings = ticket::siblings(conn, id)?;
    Ok(move_section(&d, &siblings))
}

/// The add-subtask form; a fresh copy replaces it after a success so the
/// typed text is cleared. The form's id carries the number of subtasks: the
/// morph keeps what was typed into an element it matches by id, so a new id is
/// what makes it replace the old form instead of reusing it.
pub fn subtask_form(id: &str, existing: usize) -> Markup {
    html! {
        div id="ticket-subtask" {
            h2 { "Add a subtask" }
            (post_form_id(&format!("subtask-form-{existing}"), id, "subtasks", "subtask-add", "stack-form", html! {
                label class="field" {
                    span class="meta" { "Title" }
                    input type="text" name="title";
                }
                label class="field" {
                    span class="meta" { "Description (optional)" }
                    textarea name="description" rows="3" {}
                }
                label class="field" {
                    span class="meta" { "For" }
                    select name="for" { option value="agent" { "agent" } option value="human" { "human" } }
                }
                label class="field" {
                    span class="meta" { "Status" }
                    select name="status" { option value="draft" { "draft" } option value="ready" { "ready" } }
                }
                (button("Add subtask"))
            }))
        }
    }
}

/// Most candidates the blocker picker lists.
pub const PICKER_ROWS: usize = 20;

fn blocker_picker(id: &str) -> Markup {
    html! {
        div id="ticket-blockers" {
            h2 { "Add a blocker" }
            form class="inline-form"
                "data-on:input__debounce.250ms"=(format!("@get('{}/blocker-search', {{contentType: 'form'}})", ticket_link(id)))
                data-on:submit=(format!("@get('{}/blocker-search', {{contentType: 'form'}})", ticket_link(id))) {
                label class="field" {
                    span class="meta" { "Search open tickets in this Project" }
                    input type="search" name="q" autocomplete="off";
                }
            }
            div id="blocker-results" {}
            div id="err-blocker-add" {}
        }
    }
}

/// The candidate list for the blocker picker: the Project's open tickets that
/// match `candidates` (already filtered and capped by the caller), each with
/// an Add button. `total` counts every match, so "showing N of M" is honest.
pub fn blocker_results(id: &str, candidates: &[(String, String, String)], total: usize) -> Markup {
    html! {
        div id="blocker-results" {
            @if candidates.is_empty() {
                (components::empty("No matching tickets", "Only open tickets in this Project can block this one."))
            } @else {
                ul class="plain" {
                    @for (cid, status, title) in candidates {
                        li class="related" {
                            span class="row-id" { (cid) }
                            " " span class="meta" { (status) }
                            " " span { (title) }
                            form class="inline-form"
                                data-on:submit=(format!("@post('{}/blockers', {{contentType: 'form'}})", ticket_link(id))) {
                                input type="hidden" name="blocker" value=(cid);
                                (button("Add"))
                            }
                        }
                    }
                }
                @if total > candidates.len() {
                    p class="meta" { "showing " (candidates.len()) " of " (total) " matches; type more to narrow" }
                }
            }
        }
    }
}

/// The write forms that must survive a live update: edit, move, blocker picker
/// and add-subtask. Rendered once by the page, outside the patched region.
pub fn tools(d: &TicketDetail, siblings: &[SiblingRef]) -> Markup {
    let t = &d.ticket;
    html! {
        div id="ticket-tools" {
            section class="detail-section" { (edit_form(d)) }
            section class="detail-section" { (move_section(d, siblings)) }
            section class="detail-section" { (blocker_picker(&t.id)) }
            section class="detail-section" {
                @if t.parent.is_some() {
                    h2 { "Subtasks" }
                    p class="meta" { "This ticket is a subtask; tickets nest two levels deep." }
                } @else {
                    (subtask_form(&t.id, t.subtasks.len()))
                }
            }
        }
    }
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
        let found = read(&conn, &want)?;
        let region = region(&conn, &want, role_kind)?;
        let tools = match &found {
            Some((d, _)) => Some(tools(d, &ticket::siblings(&conn, &want)?)),
            None => None,
        };
        Ok((banner, found.is_some(), region, tools))
    })
    .await;
    let mut logged = None;
    let (status, banner, title, body) = match result {
        Ok((banner, found, region, tools)) => {
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
                @if let Some(t) = tools { (t) }
            };
            (status, banner, title, body)
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            "Error".into(),
            components::error("Could not read the database", &unreadable(&e, &mut logged)),
        ),
    };
    let ctx = PageCtx {
        route: Route::List,
        role,
        banner,
    };
    (status, Html(layout::page(&ctx, &title, body).into_string()))
        .into_response()
        .logged(logged)
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
    fn the_live_region_never_carries_the_write_forms() {
        let (_d, p) = db();
        let mut c = crate::db::open(&p).unwrap();
        let id = add(&mut c, "T", "d", "p", &[]);
        let html = live(&p, &id).unwrap();
        for form in [
            "ticket-edit",
            "ticket-blockers",
            "ticket-subtask",
            "ticket-move",
            "ticket-tools",
        ] {
            assert!(!html.contains(form), "{form}: {html}");
        }
        let d = ticket::show(&c, &id).unwrap();
        assert!(tools(&d, &[]).into_string().contains("ticket-edit"));
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
