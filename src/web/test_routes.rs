//! Routes that exist only under the `test-routes` feature, so integration
//! tests (and a person checking in a browser) can drive the error contract and
//! the live stream without a real screen. Never compiled into a release.

use std::path::Path;

use axum::Router;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use maud::{Markup, html};

use super::AppState;
use super::error::WebError;
use super::state::blocking;
use super::view::layout::{self, PageCtx, Route};
use crate::error::{Error, Result};

/// The live view the test page follows.
pub const LIVE_VIEW: &str = "/test/page";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/test/page", get(page))
        .route("/test/refuse", post(refuse))
        .route("/test/conflict", post(conflict))
}

/// The count of open tickets: changes when a ticket is added or closed, not
/// when a comment is added, which is what the hash-skip test needs.
pub fn live(db: &Path) -> Result<Markup> {
    let conn = crate::db::open(db)?;
    let o = crate::project::list_with_counts(&conn)?;
    let open: u32 = o.unscoped_open + o.projects.iter().map(|p| p.open).sum::<u32>();
    Ok(html! { div id="test-live" { "open: " (open) } })
}

async fn page(State(state): State<AppState>) -> Response {
    let db = state.db.clone();
    let live = match blocking(move || live(&db)).await {
        Ok(m) => m,
        Err(e) => return WebError::new(e, "page").into_response(),
    };
    let ctx = PageCtx {
        route: Route::List,
        role: state.identity.role.as_str(),
        banner: None,
    };
    let body = html! {
        h1 { "Test page" }
        div data-init="@get('/stream?view=/test/page')" { (live) }
        button type="button" id="refuse-btn" data-on:click="@post('/test/refuse')" { "Refuse" }
        div id="err-refuse" {}
        button type="button" id="conflict-btn" data-on:click="@post('/test/conflict')" { "Conflict" }
        div id="err-conflict" {}
    };
    Html(layout::page(&ctx, "Test", body).into_string()).into_response()
}

async fn refuse() -> WebError {
    WebError::new(Error::Refused("this is always refused".into()), "refuse")
}

async fn conflict() -> WebError {
    WebError::new(
        Error::Conflict("someone else got there first".into()),
        "conflict",
    )
}
