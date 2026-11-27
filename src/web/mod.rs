//! `scree web`: a local web UI, server-rendered with maud (ADR 0003). One
//! guarded router serves every page. Handlers hold no SQL; they call the
//! library like the CLI does.

mod assets;
pub mod config;
pub mod control;
pub mod create;
mod error;
mod guard;
mod handlers;
mod pages;
pub mod scope;
mod sse;
mod state;
#[cfg(feature = "test-routes")]
mod test_routes;
pub mod view;

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::Router;
use axum::middleware;
use axum::routing::{get, post};
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::watch;

use crate::error::{Error, Result};
use crate::identity::Identity;
use state::Live;

/// How long a signal waits for open requests before exiting anyway.
const DRAIN: Duration = Duration::from_secs(2);

/// Serve until SIGINT or SIGTERM. Prints `listening on http://ADDR` to stderr
/// once bound; port 0 picks a free port. The address is also answered on the
/// database's control socket, and a second server on one database is refused.
/// The database must already exist.
pub fn run(db: std::path::PathBuf, identity: &Identity, cfg: config::Config) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let live = Live::new();
    let identity = identity.clone();
    runtime.block_on(serve(AppState { db, identity, live }, cfg))
}

/// What every handler needs: where the database is, the Identity writes run
/// as (always the human Role) and the live-update state. Handlers open a connection per request (local, single
/// user).
#[derive(Clone)]
struct AppState {
    db: PathBuf,
    identity: Identity,
    live: Arc<Live>,
}

async fn serve(state: AppState, cfg: config::Config) -> Result<()> {
    // Held until this returns: it makes this the database's only server.
    let claim = control::Claim::acquire(&state.db)?;
    let addr = cfg.addr;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| Error::Other(format!("cannot listen on {addr}: {e}")))?;
    let local = listener.local_addr()?;
    let rule = Arc::new(config::host_rule(local.ip(), local.port()));
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
        .route(
            "/new",
            get(pages::create::new_page).post(pages::create::create),
        )
        .route("/ticket/{id}", get(pages::ticket::show))
        .route("/decisions", get(pages::decisions::index))
        .route("/decisions/{id}/accept", post(pages::decisions::accept))
        .route("/decisions/{id}/answer", post(pages::decisions::answer))
        .route("/board", get(pages::board::page))
        .route("/board/{id}/{mv}", post(pages::board::move_card))
        .route("/graph", get(pages::graph::page))
        .route("/ledger", get(pages::ledger::index))
        .route("/ledger/{id}/supersede", post(pages::ledger::supersede))
        .route("/stream", get(sse::stream))
        .merge(handlers::router())
        .merge(assets::router());
    #[cfg(feature = "test-routes")]
    let router = router.merge(test_routes::router());
    let router = router
        .layer(middleware::from_fn_with_state(rule, guard::guard))
        .with_state(state);

    if cfg.open {
        view::layout::set_network_open();
        eprintln!(
            "scree web: WARNING: listening on a network address; anyone on the network can \
             read and change tickets, there is no login. Use only on a network you trust."
        );
    }
    claim.serve(control::Status::of(local))?;
    // Before the listening line: a caller may signal as soon as it reads it.
    let signal = shutdown_signal()?;
    eprintln!("scree web: listening on http://{local}");
    let (stop_tx, mut stop) = watch::channel(false);
    let serving = axum::serve(listener, router).with_graceful_shutdown(async move {
        let _ = stop.wait_for(|s| *s).await;
    });
    let signalled = async {
        signal.await;
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

/// Registers the handlers now; the returned future ends on SIGINT or SIGTERM.
fn shutdown_signal() -> Result<impl Future<Output = ()>> {
    let handler =
        |kind| signal(kind).map_err(|e| Error::Other(format!("cannot handle signals: {e}")));
    let mut term = handler(SignalKind::terminate())?;
    let mut int = handler(SignalKind::interrupt())?;
    Ok(async move {
        tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
    })
}
