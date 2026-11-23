//! Every static asset `scree web` serves, embedded in the binary. Callers link
//! the `*_HREF` constants and mount `router()`; embedding, content type, ETag
//! and conditional GET live here and nowhere else.

use std::sync::OnceLock;

use axum::Router;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use sha2::{Digest, Sha256};

pub const TOKENS_HREF: &str = "/assets/tokens.css";
pub const APP_HREF: &str = "/assets/app.css";
pub const DATASTAR_HREF: &str = "/assets/datastar.js";
pub const SHELL_HREF: &str = "/assets/shell.js";
pub const ERRORS_HREF: &str = "/assets/errors.js";

const CSS: &str = "text/css; charset=utf-8";
const JS: &str = "text/javascript; charset=utf-8";

struct Asset {
    path: &'static str,
    content_type: &'static str,
    body: &'static str,
}

static ASSETS: [Asset; 5] = [
    Asset {
        path: TOKENS_HREF,
        content_type: CSS,
        body: include_str!("../../docs/design-system/tokens.css"),
    },
    Asset {
        path: APP_HREF,
        content_type: CSS,
        body: include_str!("assets/app.css"),
    },
    Asset {
        path: DATASTAR_HREF,
        content_type: JS,
        body: include_str!("assets/datastar.js"),
    },
    Asset {
        path: SHELL_HREF,
        content_type: JS,
        body: include_str!("assets/shell.js"),
    },
    Asset {
        path: ERRORS_HREF,
        content_type: JS,
        body: include_str!("assets/errors.js"),
    },
];

/// The strong ETag of asset `i`: quoted SHA-256 of its body, computed once.
fn etag(i: usize) -> &'static str {
    static TAGS: OnceLock<Vec<String>> = OnceLock::new();
    &TAGS.get_or_init(|| {
        ASSETS
            .iter()
            .map(|a| {
                let digest = Sha256::digest(a.body.as_bytes());
                let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
                format!("\"{hex}\"")
            })
            .collect()
    })[i]
}

/// RFC 9110 section 13.1.2: `*`, or a comma list of tags compared weakly.
fn matches(if_none_match: &str, tag: &str) -> bool {
    if_none_match.trim() == "*"
        || if_none_match
            .split(',')
            .map(|t| t.trim().trim_start_matches("W/"))
            .any(|t| t == tag)
}

fn serve(i: usize, headers: &HeaderMap) -> Response {
    let asset = &ASSETS[i];
    let tag = etag(i);
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| matches(v, tag));
    let common = [
        (header::ETAG, HeaderValue::from_str(tag).unwrap()),
        (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        (
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ),
    ];
    if fresh {
        (StatusCode::NOT_MODIFIED, common).into_response()
    } else {
        (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static(asset.content_type),
            )],
            common,
            asset.body,
        )
            .into_response()
    }
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    let mut router = Router::new();
    for (i, asset) in ASSETS.iter().enumerate() {
        router = router.route(
            asset.path,
            get(move |headers: HeaderMap| async move { serve(i, &headers) }),
        );
    }
    router
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_none_match_forms() {
        let t = "\"abc\"";
        assert!(matches("*", t));
        assert!(matches("\"abc\"", t));
        assert!(matches("W/\"abc\"", t));
        assert!(matches("\"x\", W/\"abc\"", t));
        assert!(!matches("\"x\"", t));
        assert!(!matches("", t));
    }
}
