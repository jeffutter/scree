//! `GET /stream?view=/path`: a Server-Sent Events stream that keeps one view
//! current. It sends the view once, then again whenever the data changed and
//! the rendering differs from the last one sent. Events are Datastar's
//! `datastar-patch-elements`, framed by hand (no crate: the framing is a few
//! lines). Streams end when the server shuts down.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use futures_util::StreamExt;
use futures_util::stream;
use tokio::sync::watch;

use super::AppState;
use super::error::WebError;
use super::state::{DEBOUNCE, Live, Slot};
use crate::error::Error;

/// The control id errors from `/stream` are placed under.
const CONTROL: &str = "stream";

/// Render the live view named by `view` (a path starting with `/`) to HTML.
/// This is the registry: a screen ticket adds its view as an arm. Anything
/// unlisted is [`Error::NotFound`], so `view` can never name a file or a
/// route that is not a live view.
pub fn render_view(db: &Path, view: &str) -> crate::error::Result<String> {
    let _ = db;
    match view {
        "/" => super::pages::list::live(db, ""),
        v if v.starts_with("/?") => super::pages::list::live(db, &v[2..]),
        #[cfg(feature = "test-routes")]
        super::test_routes::LIVE_VIEW => Ok(super::test_routes::live(db)?.into_string()),
        _ => Err(Error::NotFound(format!("no live view {view}"))),
    }
}

/// One `datastar-patch-elements` event. Each line of the HTML is its own
/// `data: elements` line, as the protocol requires.
fn frame(html: &str) -> Bytes {
    let mut out = String::from("event: datastar-patch-elements\n");
    for line in html.lines() {
        out.push_str("data: elements ");
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
    Bytes::from(out)
}

async fn stopped(rx: &mut watch::Receiver<bool>) {
    let _ = rx.wait_for(|s| *s).await;
}

/// What the stream carries between events.
struct Follow {
    live: Arc<Live>,
    state: AppState,
    view: String,
    rx: watch::Receiver<u64>,
    stop: watch::Receiver<bool>,
    last: [u8; 32],
    _slot: Slot,
}

impl Follow {
    /// The next patch to send, or `None` when the stream is over.
    async fn next(&mut self) -> Option<Bytes> {
        loop {
            tokio::select! {
                changed = self.rx.changed() => changed.ok()?,
                () = stopped(&mut self.stop) => return None,
            }
            // Let a burst of writes settle, then look at the latest.
            tokio::select! {
                () = tokio::time::sleep(DEBOUNCE) => {}
                () = stopped(&mut self.stop) => return None,
            }
            self.rx.borrow_and_update();
            // A failed render keeps the stream open for the next change.
            if let Ok(r) = self.live.render(&self.state, &self.view).await
                && r.hash != self.last
            {
                self.last = r.hash;
                return Some(frame(&r.html));
            }
        }
    }
}

pub async fn stream(
    State(state): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let Some(view) = q.get("view").filter(|v| v.starts_with('/')).cloned() else {
        return WebError::new(
            Error::Usage("view must be a path starting with /".into()),
            CONTROL,
        )
        .into_response();
    };
    let live = state.live.clone();
    let Some(slot) = live.try_slot() else {
        return WebError::busy("Too many live views are open. Close one.", CONTROL).into_response();
    };
    // Subscribe first so a change during the first render is not missed.
    let rx = live.subscribe();
    let stop = live.stopping();
    let first = match live.render(&state, &view).await {
        Ok(r) => r,
        Err(e) => return WebError::new(e, CONTROL).into_response(),
    };
    let first_frame = frame(&first.html);
    let follow = Follow {
        live,
        state,
        view,
        rx,
        stop,
        last: first.hash,
        _slot: slot,
    };
    let rest = stream::unfold(follow, |mut f| async move {
        let bytes = f.next().await?;
        Some((bytes, f))
    });
    let body = stream::once(async move { first_frame })
        .chain(rest)
        .map(Ok::<_, Infallible>);
    (
        [
            (header::CONTENT_TYPE, "text/event-stream"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        Body::from_stream(body),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_one_data_line_per_html_line() {
        assert_eq!(
            frame("<p>a</p>\n<p>b</p>"),
            Bytes::from(
                "event: datastar-patch-elements\ndata: elements <p>a</p>\ndata: elements <p>b</p>\n\n"
            )
        );
    }

    #[test]
    fn unknown_views_are_not_found() {
        let e = render_view(Path::new("/nowhere.db"), "/etc/passwd").unwrap_err();
        assert!(matches!(e, Error::NotFound(_)));
    }
}
