//! POST handlers for the ticket detail (`/ticket/ID/...`): the lifecycle moves,
//! criteria and comments. Each one parses a urlencoded form, calls one
//! `actions::*` function as the web's human Identity inside `blocking`, and
//! answers with the refreshed `ticket` region (success) or a `WebError` naming
//! the control that caused the failure. No SQL lives here, and nothing sweeps
//! (docs/web-ui.md, "Ticket detail writes").

use std::path::PathBuf;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::post;
use rusqlite::Connection;

use super::AppState;
use super::error::WebError;
use super::pages::ticket::region;
use super::state::blocking;
use crate::actions;
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::scope;
use crate::ticket::{self, CommitLink};

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
    let db = state.db.clone();
    let who = state.identity.clone();
    let role = who.role;
    let result = blocking(move || {
        let mut conn = crate::db::open(&db)?;
        f(&mut conn, &id, &who)?;
        region(&conn, &id, role)
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
