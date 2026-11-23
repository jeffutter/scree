//! `scree web`: a local web UI, server-rendered with maud (ADR 0003). This is
//! the server skeleton: one guarded router and a placeholder page. Handlers
//! hold no SQL; they will call the library like the CLI does.

mod assets;
pub mod config;
pub mod create;
mod guard;
pub mod view;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use maud::html;
use tokio::sync::watch;

use crate::error::{Error, Result};
use crate::identity::Identity;
use view::components;
use view::layout::{self, PageCtx, Route};

/// How long a signal waits for open requests before exiting anyway.
const DRAIN: Duration = Duration::from_secs(2);

/// Serve until SIGINT or SIGTERM. Prints `listening on http://ADDR` to stderr
/// once bound; port 0 picks a free port. The database must already exist.
pub fn run(db: std::path::PathBuf, identity: &Identity, cfg: config::Config) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let role = identity.role.as_str();
    runtime.block_on(serve(AppState { db, role }, cfg))
}

/// What every handler needs: where the database is and the resolved Role.
/// Handlers open a connection per request (local, single user).
#[derive(Clone)]
struct AppState {
    db: PathBuf,
    role: &'static str,
}

async fn serve(state: AppState, cfg: config::Config) -> Result<()> {
    let addr = cfg.addr;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| Error::Other(format!("cannot listen on {addr}: {e}")))?;
    let local = listener.local_addr()?;
    let hosts = Arc::new(config::allowed_hosts(local.port()));
    let router = Router::new()
        .route("/", get(index))
        .merge(assets::router())
        .layer(middleware::from_fn_with_state(hosts, guard::guard))
        .with_state(state);

    eprintln!("scree web: listening on http://{local}");
    let (stop_tx, mut stop) = watch::channel(false);
    let serving = axum::serve(listener, router).with_graceful_shutdown(async move {
        let _ = stop.wait_for(|s| *s).await;
    });
    let signalled = async {
        wait_for_signal().await;
        let _ = stop_tx.send(true);
        tokio::time::sleep(DRAIN).await;
    };
    tokio::select! {
        r = serving => r.map_err(Error::from),
        () = signalled => Ok(()),
    }
}

async fn wait_for_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let (Ok(mut term), Ok(mut int)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) else {
            return std::future::pending().await;
        };
        tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn index(State(state): State<AppState>) -> Response {
    let db = state.db.clone();
    let counts = tokio::task::spawn_blocking(move || view::banner_counts(&crate::db::open(&db)?))
        .await
        .map_err(|e| Error::Other(e.to_string()))
        .and_then(|r| r);
    let (status, ctx, body) = match counts {
        Ok(banner) => (
            StatusCode::OK,
            PageCtx {
                route: Route::List,
                role: state.role,
                banner,
            },
            html! {
                h1 { "Scree" }
                p { "The web UI is not built yet." }
            },
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            PageCtx {
                route: Route::List,
                role: state.role,
                banner: None,
            },
            components::error("Could not read the database", &e.to_string()),
        ),
    };
    (
        status,
        Html(layout::page(&ctx, "Scree", body).into_string()),
    )
        .into_response()
}
