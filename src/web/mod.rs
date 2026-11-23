//! `scree web`: a local web UI, server-rendered with maud (ADR 0003). This is
//! the server skeleton: one guarded router and a placeholder page. Handlers
//! hold no SQL; they will call the library like the CLI does.

mod assets;
pub mod config;
pub mod create;
mod error;
mod guard;
mod pages;
pub mod scope;
mod sse;
mod state;
#[cfg(feature = "test-routes")]
mod test_routes;
pub mod view;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::Router;
use axum::middleware;
use axum::routing::get;
use tokio::sync::watch;

use crate::error::{Error, Result};
use crate::identity::Identity;
use state::Live;

/// How long a signal waits for open requests before exiting anyway.
const DRAIN: Duration = Duration::from_secs(2);

/// Serve until SIGINT or SIGTERM. Prints `listening on http://ADDR` to stderr
/// once bound; port 0 picks a free port. The database must already exist.
pub fn run(db: std::path::PathBuf, identity: &Identity, cfg: config::Config) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let role = identity.role.as_str();
    let live = Live::new();
    runtime.block_on(serve(AppState { db, role, live }, cfg))
}

/// What every handler needs: where the database is, the resolved Role and the
/// live-update state. Handlers open a connection per request (local, single
/// user).
#[derive(Clone)]
struct AppState {
    db: PathBuf,
    role: &'static str,
    live: Arc<Live>,
}

async fn serve(state: AppState, cfg: config::Config) -> Result<()> {
    let addr = cfg.addr;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| Error::Other(format!("cannot listen on {addr}: {e}")))?;
    let local = listener.local_addr()?;
    let hosts = Arc::new(config::allowed_hosts(local.port()));
    // The watcher thread publishes "the database changed" to every stream. It
    // is not joined at exit: it only polls, and the process ending stops it.
    let watcher_stop = Arc::new(AtomicBool::new(false));
    let publisher = state.live.clone();
    crate::watch::spawn_with(state.db.clone(), watcher_stop.clone(), move || {
        publisher.bump();
        true
    })?;
    let live = state.live.clone();
    let router = Router::new()
        .route("/", get(pages::list::index))
        .route("/stream", get(sse::stream))
        .merge(assets::router());
    #[cfg(feature = "test-routes")]
    let router = router.merge(test_routes::router());
    let router = router
        .layer(middleware::from_fn_with_state(hosts, guard::guard))
        .with_state(state);

    eprintln!("scree web: listening on http://{local}");
    let (stop_tx, mut stop) = watch::channel(false);
    let serving = axum::serve(listener, router).with_graceful_shutdown(async move {
        let _ = stop.wait_for(|s| *s).await;
    });
    let signalled = async {
        wait_for_signal().await;
        // Open streams never end on their own; end them so the graceful
        // shutdown is quick. DRAIN is only the backstop for slow requests.
        live.stop_streams();
        let _ = stop_tx.send(true);
        tokio::time::sleep(DRAIN).await;
    };
    let result = tokio::select! {
        r = serving => r.map_err(Error::from),
        () = signalled => Ok(()),
    };
    watcher_stop.store(true, Ordering::Relaxed);
    result
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
