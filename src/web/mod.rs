//! `scree web`: a local web UI, server-rendered with maud (ADR 0003). This is
//! the server skeleton: one guarded router and a placeholder page. Handlers
//! hold no SQL; they will call the library like the CLI does.

mod assets;
pub mod config;
pub mod create;
mod guard;

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::middleware;
use axum::response::Html;
use axum::routing::get;
use maud::{DOCTYPE, html};
use tokio::sync::watch;

use crate::error::{Error, Result};
use crate::identity::Identity;

/// How long a signal waits for open requests before exiting anyway.
const DRAIN: Duration = Duration::from_secs(2);

/// Serve until SIGINT or SIGTERM. Prints `listening on http://ADDR` to stderr
/// once bound; port 0 picks a free port. The database must already exist.
pub fn run(_db: std::path::PathBuf, identity: &Identity, cfg: config::Config) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let role = identity.role.as_str();
    runtime.block_on(serve(role, cfg))
}

async fn serve(role: &'static str, cfg: config::Config) -> Result<()> {
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
        .with_state(role);

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

async fn index(State(role): State<&'static str>) -> Html<String> {
    Html(
        html! {
            (DOCTYPE)
            html lang="en" {
                head {
                    meta charset="utf-8";
                    title { "Scree" }
                    link rel="stylesheet" href=(assets::TOKENS_HREF);
                    link rel="stylesheet" href=(assets::APP_HREF);
                    script type="module" src=(assets::DATASTAR_HREF) {}
                }
                body {
                    h1 { "Scree" }
                    p { "The web UI is not built yet." }
                    footer { "role: " (role) }
                }
            }
        }
        .into_string(),
    )
}
