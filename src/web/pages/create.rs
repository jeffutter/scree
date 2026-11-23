//! Create a ticket from the web: the quick form (title, parent, status, for)
//! on the List, and the full form (also description, plan, criteria and
//! blockers) at `/new`. Both post to `POST /new`. The Project and Worktree come
//! from the form's hidden `p` and `w` (the page's scope, never the cwd); a
//! parent decides the scope of a subtask. Nothing here sweeps.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use maud::{Markup, html};

use crate::error::{Error, Result};
use crate::scope::Scope;
use crate::ticket::{self, NewTicket, ScopeFilter};
use crate::web::AppState;
use crate::web::create::create_in;
use crate::web::error::{LoggedExt, WebError, unreadable};
use crate::web::handlers::{Form, for_who, lf, new_status};
use crate::web::scope::{self, Scoped};
use crate::web::state::blocking;
use crate::web::view::components;
use crate::web::view::layout::{self, PageCtx, Route};
use crate::web::view::{self, picker};

/// Which of the two forms.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Quick,
    Full,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Quick => "quick",
            Mode::Full => "full",
        }
    }
}

/// One text line per entry, blanks dropped.
fn lines(raw: &str) -> Vec<String> {
    lf(raw)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Display ids separated by whitespace or commas.
fn ids(raw: &str) -> Vec<String> {
    raw.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The create form for `filter`, or the "pick a Project" state for all
/// Projects. `created` is the id of the ticket this form just made: it shows a
/// link and gives the form a new element id, so the browser replaces it with an
/// empty one instead of keeping the typed text.
pub fn form(filter: &ScopeFilter, mode: Mode, created: Option<&str>) -> Markup {
    let m = mode.as_str();
    let container = format!("create-{m}");
    if matches!(filter, ScopeFilter::All) {
        return html! {
            div id=(container) {
                (components::empty(
                    "Pick a Project to create a ticket",
                    "Choose a Project (and Worktree) above, then create the ticket there."))
            }
        };
    }
    let form_id = match created {
        Some(id) => format!("create-form-{m}-{id}"),
        None => format!("create-form-{m}"),
    };
    html! {
        div id=(container) {
            @if let Some(id) = created {
                p class="notice-hint" role="status" {
                    "Created " a href=(format!("/ticket/{id}")) { (id) } "."
                }
            }
            form class="stack-form" id=(form_id)
                data-on:submit="@post('/new', {contentType: 'form'})" {
                input type="hidden" name="mode" value=(m);
                @for (k, v) in scope::pairs(filter) {
                    input type="hidden" name=(k) value=(v);
                }
                label class="field" {
                    span class="meta" { "Title" }
                    input type="text" name="title";
                }
                @if mode == Mode::Full {
                    label class="field" {
                        span class="meta" { "Description (optional)" }
                        textarea name="description" rows="4" {}
                    }
                    label class="field" {
                        span class="meta" { "Plan (optional)" }
                        textarea name="plan" rows="4" {}
                    }
                    label class="field" {
                        span class="meta" { "Acceptance criteria (one per line)" }
                        textarea name="criteria" rows="3" {}
                    }
                    label class="field" {
                        span class="meta" { "Blocked by (ticket ids, separated by spaces or commas)" }
                        input type="text" name="blockers" autocomplete="off";
                    }
                } @else {
                    label class="field" {
                        span class="meta" { "Parent (optional; a subtask takes its parent's Project and Worktree)" }
                        input type="text" name="parent" autocomplete="off";
                    }
                }
                label class="field" {
                    span class="meta" { "For" }
                    select name="for" { option value="agent" { "agent" } option value="human" { "human" } }
                }
                label class="field" {
                    span class="meta" { "Status" }
                    select name="status" { option value="draft" { "draft" } option value="ready" { "ready" } }
                }
                button type="submit" class="btn" { "Create ticket" }
            }
            div id="err-create" {}
        }
    }
}

/// `GET /new`: the full form for the picked scope.
pub async fn new_page(State(state): State<AppState>, Scoped(scope): Scoped) -> Response {
    let db = state.db.clone();
    let role = state.identity.role.as_str();
    let result = blocking(move || view::banner_counts(&crate::db::open(&db)?)).await;
    let mut logged = None;
    let (status, banner, body) = match result {
        Ok(banner) => (
            StatusCode::OK,
            banner,
            html! {
                h1 { "New ticket" }
                (picker::picker(&scope, "/new"))
                (form(&scope.filter, Mode::Full, None))
                p class="more" { a href="/" { "Back to the list" } }
            },
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            None,
            components::error("Could not read the database", &unreadable(&e, &mut logged)),
        ),
    };
    let ctx = PageCtx {
        route: Route::List,
        role,
        banner,
    };
    (
        status,
        Html(layout::page(&ctx, "New ticket", body).into_string()),
    )
        .into_response()
        .logged(logged)
}

/// `POST /new`: create one ticket and answer with the form, emptied.
pub async fn create(State(state): State<AppState>, body: Bytes) -> Response {
    let f = Form::parse(&body);
    let mode = if f.get("mode") == "full" {
        Mode::Full
    } else {
        Mode::Quick
    };
    let db = state.db.clone();
    let result = blocking(move || -> Result<Markup> {
        let who = for_who(if f.opt("for").is_some() {
            f.get("for")
        } else {
            "agent"
        })?;
        let status = new_status(f.get("status"))?;
        let title = f.get("title").to_string();
        let description = lf(f.get("description"));
        let plan = lf(f.get("plan"));
        let criteria = lines(f.get("criteria"));
        let blockers = ids(f.get("blockers"));
        let parent = f.opt("parent").map(|p| p.trim().to_string());
        let filter = if f.has("p") {
            scope::parse(Some(f.get("p")), f.opt("w").as_deref())?
        } else {
            ScopeFilter::All
        };
        if parent.is_none() && matches!(filter, ScopeFilter::All) {
            return Err(Error::Usage(
                "pick a Project to create the ticket in (p is missing)".into(),
            ));
        }
        let t = NewTicket {
            title: &title,
            description: &description,
            plan: &plan,
            criteria: &criteria,
            for_who: who,
            status,
            key: None,
            parent: parent.as_deref(),
            blocked_by: &blockers,
        };
        let mut conn = crate::db::open(&db)?;
        let detail = match &filter {
            // A parent decides Project and Worktree, and the unscoped Project
            // has no directory: neither names a scope, so the server's cwd is
            // never read.
            _ if parent.is_some() => ticket::add(&mut conn, &Scope::Unscoped, &t)?,
            ScopeFilter::Project { key } if key == "LOCAL" => {
                ticket::add(&mut conn, &Scope::Unscoped, &t)?
            }
            ScopeFilter::Project { key } => create_in(&mut conn, key, None, &t)?,
            ScopeFilter::Worktree { key, path } => create_in(&mut conn, key, path.as_deref(), &t)?,
            ScopeFilter::All => unreachable!("refused above"),
        };
        Ok(form(&filter, mode, Some(&detail.ticket.id)))
    })
    .await;
    match result {
        Ok(m) => Html(m.into_string()).into_response(),
        Err(e) => WebError::new(e, "create").into_response(),
    }
}
