//! POST handlers for the ticket detail (`/ticket/ID/...`): the lifecycle moves,
//! criteria and comments. Each one parses a urlencoded form, calls one
//! `actions::*` function as the web's human Identity inside `blocking`, and
//! answers with the refreshed `ticket` region (success) or a `WebError` naming
//! the control that caused the failure. No SQL lives here, and nothing sweeps
//! (docs/web-ui.md, "Ticket detail writes").

use std::path::PathBuf;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use maud::{Markup, html};
use rusqlite::Connection;

use super::AppState;
use super::error::WebError;
use super::pages::ticket::{self as page, region};
use super::state::blocking;
use crate::actions;
use crate::cli::{ForWho, ListArgs, NewStatus};
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::scope;
use crate::ticket::{self, CommitLink, EditFields, NewTicket, ParentChange, ScopeFilter};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ticket/{id}/ready", post(ready))
        .route("/ticket/{id}/draft", post(draft))
        .route("/ticket/{id}/start", post(start))
        .route("/ticket/{id}/release", post(release))
        .route("/ticket/{id}/done", post(done))
        .route("/ticket/{id}/abandon", post(abandon))
        .route("/ticket/{id}/reopen", post(reopen))
        .route("/ticket/{id}/archive", post(archive))
        .route("/ticket/{id}/unarchive", post(unarchive))
        .route("/ticket/{id}/commits", post(commits))
        .route("/ticket/{id}/criteria", post(criterion_add))
        .route("/ticket/{id}/criteria/{n}/check", post(criterion_check))
        .route("/ticket/{id}/criteria/{n}/uncheck", post(criterion_uncheck))
        .route("/ticket/{id}/criteria/{n}/remove", post(criterion_remove))
        .route("/ticket/{id}/comments", post(comment_add))
        .route("/ticket/{id}/comments/{cid}/edit", post(comment_edit))
        .route("/ticket/{id}/comments/{cid}/delete", post(comment_delete))
        .route("/ticket/{id}/blockers", post(blocker_add))
        .route(
            "/ticket/{id}/blockers/{blocker}/remove",
            post(blocker_remove),
        )
        .route("/ticket/{id}/blocker-search", get(blocker_search))
        .route("/ticket/{id}/edit", post(edit))
        .route("/ticket/{id}/preview", post(preview))
        .route("/ticket/{id}/subtasks", post(subtask_add))
}

/// A parsed urlencoded body. An absent field and an empty one read the same
/// (an unchecked checkbox sends nothing).
struct Form(Vec<(String, String)>);

impl Form {
    fn parse(body: &Bytes) -> Form {
        Form(
            form_urlencoded::parse(body)
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
        )
    }

    fn get(&self, name: &str) -> &str {
        self.0
            .iter()
            .find(|(k, _)| k == name)
            .map_or("", |(_, v)| v.as_str())
    }

    /// The field, or `None` when blank.
    fn opt(&self, name: &str) -> Option<String> {
        let v = self.get(name);
        (!v.trim().is_empty()).then(|| v.to_string())
    }

    fn has(&self, name: &str) -> bool {
        self.0.iter().any(|(k, _)| k == name)
    }

    fn flag(&self, name: &str) -> bool {
        !self.get(name).is_empty()
    }
}

/// A number in the path; anything else is `Usage`.
fn number(raw: &str, what: &str) -> Result<i64> {
    raw.parse::<i64>()
        .map_err(|_| Error::Usage(format!("{raw:?} is not a {what} number")))
}

/// Run one write as the human Identity, then render the region from a fresh
/// read so the answer shows what is in the database now.
async fn exec(
    state: AppState,
    id: String,
    control: String,
    f: impl FnOnce(&mut Connection, &str, &Identity) -> Result<()> + Send + 'static,
) -> Response {
    exec_with(state, id, control, no_extra, f).await
}

fn no_extra(_: &Connection, _: &str) -> Result<Markup> {
    Ok(html! {})
}

/// [`exec`] that also answers with a form freshly rendered by `extra`, for a
/// form that must be reset or re-based after its own success (the edit form
/// carries the `updated_at` it was loaded with; the subtask form is cleared).
/// Other forms stay out of the answer, so text typed in them is kept.
async fn exec_with(
    state: AppState,
    id: String,
    control: String,
    extra: fn(&Connection, &str) -> Result<Markup>,
    f: impl FnOnce(&mut Connection, &str, &Identity) -> Result<()> + Send + 'static,
) -> Response {
    let db = state.db.clone();
    let who = state.identity.clone();
    let role = who.role;
    let result = blocking(move || {
        let mut conn = crate::db::open(&db)?;
        f(&mut conn, &id, &who)?;
        let region = region(&conn, &id, role)?;
        Ok(html! { (region) (extra(&conn, &id)?) })
    })
    .await;
    match result {
        Ok(m) => Html(m.into_string()).into_response(),
        Err(e) => WebError::new(e, &control).into_response(),
    }
}

/// Resolve one revision per line to a full sha. The directory is the ticket's
/// Worktree when it is set and exists, else its Project root; the web never
/// reads the process cwd. A directory that is gone is `Refused`, as in
/// `web::create`. This is the only git call and it is not a sweep.
fn resolve_links(conn: &Connection, id: &str, revs: &str) -> Result<Vec<CommitLink>> {
    let revs: Vec<&str> = revs
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .collect();
    if revs.is_empty() {
        return Ok(vec![]);
    }
    let d = ticket::show(conn, id)?;
    let worktree = d
        .ticket
        .worktree
        .as_deref()
        .filter(|w| std::path::Path::new(w).is_dir());
    let dir = match (worktree, d.ticket.project.root.as_deref()) {
        (Some(w), _) => PathBuf::from(w),
        (None, Some(r)) => PathBuf::from(r),
        (None, None) => {
            return Err(Error::Refused(format!(
                "{id} has no checkout to look commits up in"
            )));
        }
    };
    if !dir.is_dir() {
        return Err(Error::Refused(format!(
            "directory no longer exists: {}",
            dir.display()
        )));
    }
    revs.into_iter()
        .map(|rev| {
            scope::resolve_commit(&dir, rev)
                .map(|sha| CommitLink { sha, branch: None })
                .ok_or_else(|| Error::Usage(format!("{rev} is not a commit in this repository")))
        })
        .collect()
}

macro_rules! simple {
    ($name:ident, $control:literal, $action:path) => {
        async fn $name(State(state): State<AppState>, Path(id): Path<String>) -> Response {
            exec(state, id, $control.into(), |c, id, who| {
                $action(c, id, who).map(drop)
            })
            .await
        }
    };
}

simple!(ready, "ready", actions::ready);
simple!(draft, "draft", actions::draft);
simple!(start, "start", actions::start);
simple!(reopen, "reopen", actions::reopen);

async fn release(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let handoff = Form::parse(&body).opt("handoff");
    exec(state, id, "release".into(), move |c, id, who| {
        actions::release(c, id, handoff, who).map(drop)
    })
    .await
}

async fn done(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let f = Form::parse(&body);
    let force = f.flag("force");
    let revs = f.get("commits").to_string();
    exec(state, id, "done".into(), move |c, id, who| {
        let links = resolve_links(c, id, &revs)?;
        actions::done(c, id, force, links, who).map(drop)
    })
    .await
}

async fn abandon(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    // A blank reason goes to the library, which answers Usage (400).
    let reason = Form::parse(&body).get("reason").to_string();
    exec(state, id, "abandon".into(), move |c, id, who| {
        actions::abandon(c, id, reason, who).map(drop)
    })
    .await
}

async fn archive(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    exec(state, id, "archive".into(), |c, id, _| {
        actions::archive(c, id, true).map(drop)
    })
    .await
}

async fn unarchive(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    exec(state, id, "archive".into(), |c, id, _| {
        actions::archive(c, id, false).map(drop)
    })
    .await
}

async fn commits(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let revs = Form::parse(&body).get("commits").to_string();
    exec(state, id, "commits".into(), move |c, id, _| {
        let links = resolve_links(c, id, &revs)?;
        if links.is_empty() {
            return Err(Error::Usage("give at least one commit".into()));
        }
        actions::link_commits(c, id, &links).map(drop)
    })
    .await
}

async fn criterion_add(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let text = Form::parse(&body).get("text").to_string();
    exec(state, id, "criterion-add".into(), move |c, id, who| {
        actions::criterion_add(c, id, &text, who).map(drop)
    })
    .await
}

async fn criterion_check(
    State(state): State<AppState>,
    Path((id, n)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let evidence = Form::parse(&body).opt("evidence");
    exec(state, id, format!("criterion-{n}"), move |c, id, who| {
        let n = number(&n, "criterion")?;
        actions::criterion_check(c, id, n, evidence.as_deref(), who).map(drop)
    })
    .await
}

async fn criterion_uncheck(
    State(state): State<AppState>,
    Path((id, n)): Path<(String, String)>,
) -> Response {
    exec(state, id, format!("criterion-{n}"), move |c, id, who| {
        actions::criterion_uncheck(c, id, number(&n, "criterion")?, who).map(drop)
    })
    .await
}

async fn criterion_remove(
    State(state): State<AppState>,
    Path((id, n)): Path<(String, String)>,
) -> Response {
    exec(state, id, format!("criterion-{n}"), move |c, id, who| {
        actions::criterion_remove(c, id, number(&n, "criterion")?, who).map(drop)
    })
    .await
}

async fn comment_add(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let text = Form::parse(&body).get("body").to_string();
    exec(state, id, "comment-add".into(), move |c, id, who| {
        actions::comment_add(c, id, &text, who).map(drop)
    })
    .await
}

/// `expected_updated_at` is the hidden field the page rendered with the
/// comment; it is never re-read here, or the stale check would mean nothing.
async fn comment_edit(
    State(state): State<AppState>,
    Path((id, cid)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let f = Form::parse(&body);
    let text = f.get("body").to_string();
    let expected = f.opt("expected_updated_at");
    exec(state, id, format!("comment-{cid}"), move |c, id, who| {
        let cid = number(&cid, "comment")?;
        actions::comment_edit(c, id, cid, &text, expected.as_deref(), who).map(drop)
    })
    .await
}

async fn comment_delete(
    State(state): State<AppState>,
    Path((id, cid)): Path<(String, String)>,
) -> Response {
    exec(state, id, format!("comment-{cid}"), move |c, id, who| {
        actions::comment_remove(c, id, number(&cid, "comment")?, who).map(drop)
    })
    .await
}

async fn blocker_add(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let blocker = Form::parse(&body).get("blocker").trim().to_string();
    exec(state, id, "blocker-add".into(), move |c, id, who| {
        if blocker.is_empty() {
            return Err(Error::Usage("pick a ticket to add as a blocker".into()));
        }
        // Self, cycle and cross-Project edges are Refused by the library.
        actions::dep_add(c, id, &blocker, who).map(drop)
    })
    .await
}

async fn blocker_remove(
    State(state): State<AppState>,
    Path((id, blocker)): Path<(String, String)>,
) -> Response {
    exec(
        state,
        id,
        format!("blocker-rm-{blocker}"),
        move |c, id, who| actions::dep_remove(c, id, &blocker, who).map(drop),
    )
    .await
}

/// Candidates for the blocker picker: open tickets of the ticket's Project
/// that match the library's search, minus the ticket itself and its current
/// blockers, at most [`page::PICKER_ROWS`].
async fn blocker_search(
    State(state): State<AppState>,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
) -> Response {
    let q = form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find(|(k, _)| k == "q")
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default();
    let db = state.db.clone();
    let who = state.identity.clone();
    let want = id.clone();
    let result = blocking(move || {
        let conn = crate::db::open(&db)?;
        let d = ticket::show(&conn, &want)?;
        let t = &d.ticket;
        let excluded: Vec<&str> = std::iter::once(t.id.as_str())
            .chain(t.blocked_by.iter().map(|b| b.id.as_str()))
            .collect();
        let filter = ScopeFilter::Project {
            key: t.project.key.clone(),
        };
        // No status filter: the library's default is the open states.
        let args = ListArgs {
            limit: Some(u32::try_from(page::PICKER_ROWS + excluded.len()).unwrap_or(u32::MAX)),
            search: (!q.is_empty()).then(|| q.clone()),
            ..ListArgs::default()
        };
        let found = ticket::list_in(&conn, &filter, &args, &who)?;
        let window = page::PICKER_ROWS + excluded.len();
        let fetched = found.tickets.len();
        let mut rows: Vec<(String, String, String)> = found
            .tickets
            .into_iter()
            .filter(|c| !excluded.contains(&c.id.as_str()))
            .map(|c| (c.id, c.status, c.title))
            .collect();
        // `count_in` counts this ticket and its blockers when they match.
        // When every match was fetched the exact number is the filtered
        // length; otherwise the excluded tickets inside the fetched window are
        // subtracted (one beyond the window is not, an overcount of at most
        // the number of current blockers).
        let total = if fetched < window {
            rows.len()
        } else {
            let all = ticket::count_in(
                &conn,
                &filter,
                &ListArgs {
                    limit: None,
                    ..args
                },
                &who,
            )?;
            usize::try_from(all)
                .unwrap_or(usize::MAX)
                .saturating_sub(fetched - rows.len())
        };
        rows.truncate(page::PICKER_ROWS);
        Ok(page::blocker_results(&want, &rows, total))
    })
    .await;
    match result {
        Ok(m) => Html(m.into_string()).into_response(),
        Err(e) => WebError::new(e, "blocker-add").into_response(),
    }
}

fn for_who(raw: &str) -> Result<ForWho> {
    match raw {
        "agent" => Ok(ForWho::Agent),
        "human" => Ok(ForWho::Human),
        other => Err(Error::Usage(format!(
            "for must be agent or human, not {other:?}"
        ))),
    }
}

/// Browsers send a newline in a textarea as CRLF in some encodings; the
/// database keeps LF.
fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Save the edit form. `expected_updated_at` is the hidden field the form was
/// rendered with and is required: it is what turns a concurrent change into a
/// 409 instead of a silent overwrite. Only fields that differ from the stored
/// ticket are sent to the library.
async fn edit(State(state): State<AppState>, Path(id): Path<String>, body: Bytes) -> Response {
    let f = Form::parse(&body);
    exec_with(
        state,
        id,
        "edit".into(),
        |c, id| Ok(page::edit_form(&ticket::show(c, id)?)),
        move |c, id, who| {
            let Some(expected) = f.opt("expected_updated_at") else {
                return Err(Error::Usage(
                    "the form is missing the version it was loaded with; reload the page".into(),
                ));
            };
            let cur = ticket::show(c, id)?;
            let t = &cur.ticket;
            let changed = |name: &str, old: &str| -> Option<String> {
                f.has(name).then(|| lf(f.get(name))).filter(|v| v != old)
            };
            let title = changed("title", &t.title);
            let description = changed("description", &cur.description);
            let plan = changed("plan", &cur.plan);
            let for_new = if f.has("for") {
                Some(for_who(f.get("for"))?).filter(|w| w.as_str() != t.for_who)
            } else {
                None
            };
            let parent_raw = f.get("parent").trim().to_string();
            let parent = if f.flag("detach") {
                t.parent.is_some().then_some(ParentChange::Detach)
            } else if !parent_raw.is_empty() && t.parent.as_deref() != Some(parent_raw.as_str()) {
                Some(ParentChange::Under(parent_raw.as_str()))
            } else {
                None
            };
            let fields = EditFields {
                title: title.as_deref(),
                description: description.as_deref(),
                plan: plan.as_deref(),
                append: false,
                for_who: for_new,
                parent,
                expected_updated_at: Some(expected.as_str()),
            };
            if fields.title.is_none()
                && fields.description.is_none()
                && fields.plan.is_none()
                && fields.for_who.is_none()
                && fields.parent.is_none()
            {
                return Err(Error::Usage("nothing was changed".into()));
            }
            actions::edit(c, id, &fields, who).map(drop)
        },
    )
    .await
}

/// Render one field's text as escaped Markdown. Writes nothing and reads no
/// ticket. The field comes from the query (`?field=plan`) or the form; its
/// text is the form value `body`, or the one named like the field.
async fn preview(Path(_id): Path<String>, RawQuery(query): RawQuery, body: Bytes) -> Response {
    let f = Form::parse(&body);
    let from_query = form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find(|(k, _)| k == "field")
        .map(|(_, v)| v.into_owned());
    let field = from_query.unwrap_or_else(|| f.get("field").to_string());
    if field != "description" && field != "plan" {
        return WebError::new(
            Error::Usage("preview field must be description or plan".into()),
            "preview",
        )
        .into_response();
    }
    let text = if f.has("body") {
        f.get("body")
    } else {
        f.get(&field)
    };
    Html(page::preview(&field, &lf(text)).into_string()).into_response()
}

async fn subtask_add(
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let f = Form::parse(&body);
    exec_with(
        state,
        id,
        "subtask-add".into(),
        |c, id| {
            Ok(page::subtask_form(
                id,
                ticket::show(c, id)?.ticket.subtasks.len(),
            ))
        },
        move |c, id, _| {
            let who = for_who(if f.has("for") { f.get("for") } else { "agent" })?;
            let status = match f.get("status") {
                "" | "draft" => NewStatus::Draft,
                "ready" => NewStatus::Ready,
                other => {
                    return Err(Error::Usage(format!(
                        "status must be draft or ready, not {other:?}"
                    )));
                }
            };
            let title = f.get("title").to_string();
            let description = lf(f.get("description"));
            // A subtask takes its parent's Project and Worktree, so no scope is
            // named and the server's cwd is never read.
            ticket::add(
                c,
                &scope::Scope::Unscoped,
                &NewTicket {
                    title: &title,
                    description: &description,
                    plan: "",
                    criteria: &[],
                    for_who: who,
                    status,
                    key: None,
                    parent: Some(id),
                    blocked_by: &[],
                },
            )
            .map(drop)
        },
    )
    .await
}
