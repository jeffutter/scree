//! The error contract of the web UI: every failed request answers with a
//! non-2xx status and an HTML fragment `<div id="err-CONTROL" role="alert">`
//! that the client places next to the control that caused it (see
//! `assets/errors.js` and docs/web-ui.md). The status carries the kind of
//! failure; there is no custom error header.

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

use super::view::components;
use crate::error::Error;

/// HTTP status for a library error: 400 usage, 404 not found, 409 conflict,
/// 422 refused by a rule, 500 for database, I/O, schema and anything else.
pub fn status(e: &Error) -> StatusCode {
    match e {
        Error::Usage(_) => StatusCode::BAD_REQUEST,
        Error::NotFound(_) => StatusCode::NOT_FOUND,
        Error::Conflict(_) => StatusCode::CONFLICT,
        Error::Refused(_) => StatusCode::UNPROCESSABLE_ENTITY,
        Error::Db(_) | Error::Io(_) | Error::SchemaTooNew { .. } | Error::Other(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

/// What the request log line says about a failed response, set by
/// [`WebError`] as a response extension and read once by the guard.
#[derive(Clone, Debug)]
pub struct Logged {
    pub kind: &'static str,
    /// Detail withheld from the client (500s only); goes on the log line.
    pub detail: Option<String>,
}

/// A failed handler: which control to put the message next to, and why.
pub struct WebError {
    status: StatusCode,
    kind: &'static str,
    /// Shown to the person.
    message: String,
    /// Logged, not shown.
    detail: Option<String>,
    control: String,
}

impl WebError {
    /// A library error caused by the control `control` (the id suffix of the
    /// element that shows the message). A 500 shows a generic message and logs
    /// the real one.
    pub fn new(err: Error, control: &str) -> Self {
        let status = status(&err);
        let (message, detail) = if status == StatusCode::INTERNAL_SERVER_ERROR {
            (
                "Something went wrong. The terminal running scree web has the details.".to_string(),
                Some(err.to_string()),
            )
        } else {
            (err.to_string(), None)
        };
        WebError {
            status,
            kind: err.code_str(),
            message,
            detail,
            control: control.to_string(),
        }
    }

    /// 503: the server is up but will not take this request now.
    pub fn busy(message: &str, control: &str) -> Self {
        WebError {
            status: StatusCode::SERVICE_UNAVAILABLE,
            kind: "busy",
            message: message.to_string(),
            detail: None,
            control: control.to_string(),
        }
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> Response {
        let body = components::field_error(&self.control, &self.message).into_string();
        let mut resp = (
            self.status,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            body,
        )
            .into_response();
        resp.extensions_mut().insert(Logged {
            kind: self.kind,
            detail: self.detail,
        });
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_its_status() {
        let s = |e: Error| status(&e).as_u16();
        assert_eq!(s(Error::Usage("x".into())), 400);
        assert_eq!(s(Error::NotFound("x".into())), 404);
        assert_eq!(s(Error::Conflict("x".into())), 409);
        assert_eq!(s(Error::Refused("x".into())), 422);
        assert_eq!(s(Error::Db(rusqlite::Error::InvalidQuery)), 500);
        assert_eq!(s(Error::Io(std::io::Error::other("x"))), 500);
        assert_eq!(
            s(Error::SchemaTooNew {
                found: 2,
                supported: 1
            }),
            500
        );
        assert_eq!(s(Error::Other("x".into())), 500);
    }

    #[test]
    fn body_is_a_fragment_for_the_control_and_escapes() {
        let r = WebError::new(Error::Refused("<b>no</b>".into()), "refuse").into_response();
        assert_eq!(r.status(), 422);
        assert_eq!(r.extensions().get::<Logged>().unwrap().kind, "refused");
        assert!(r.headers().get("x-scree-error").is_none());
    }

    #[test]
    fn a_500_hides_the_detail_from_the_body_and_logs_it() {
        let r = WebError::new(Error::Other("secret path /x".into()), "c").into_response();
        assert_eq!(r.status(), 500);
        let logged = r.extensions().get::<Logged>().unwrap();
        assert_eq!(logged.detail.as_deref(), Some("secret path /x"));
    }

    #[test]
    fn busy_is_503() {
        let r = WebError::busy("full", "stream").into_response();
        assert_eq!(r.status(), 503);
        assert_eq!(r.extensions().get::<Logged>().unwrap().kind, "busy");
    }
}
