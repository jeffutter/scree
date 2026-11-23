//! The one middleware every request passes through. It blocks DNS rebinding
//! (`Host`) and cross-site requests (`Origin`, `Sec-Fetch-Site`), and sets the
//! Content-Security-Policy from ADR 0003 on every response.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::config::HostRule;
use super::error::Logged;

/// Datastar compiles `data-*` expressions with `new Function`, so `script-src`
/// needs `'unsafe-eval'` (ADR 0003). Nothing is inline.
pub const CSP: &str = "default-src 'none'; script-src 'self' 'unsafe-eval'; style-src 'self'; \
    connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

#[derive(Debug, PartialEq, Eq)]
pub enum Reason {
    Host,
    Origin,
    FetchSite,
    MissingOrigin,
}

impl Reason {
    fn text(&self) -> &'static str {
        match self {
            Reason::Host => "forbidden: unexpected Host",
            Reason::Origin => "forbidden: unexpected Origin",
            Reason::FetchSite => "forbidden: cross-site request",
            Reason::MissingOrigin => "forbidden: writes need an Origin",
        }
    }
}

fn header(h: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    h.get(name).and_then(|v| v.to_str().ok())
}

/// Decide whether a request may proceed. `Only` is an allowlist of `Host`
/// values (`host:port`); `Any` accepts every `Host` but still requires that an
/// `Origin` be same-origin with it.
pub fn check(headers: &HeaderMap, method: &Method, rule: &HostRule) -> Result<(), Reason> {
    let host = header(headers, header::HOST).ok_or(Reason::Host)?;
    let host_ok = |h: &str| match rule {
        HostRule::Only(hosts) => hosts.iter().any(|a| a == h),
        HostRule::Any => true,
    };
    if !host_ok(host) {
        return Err(Reason::Host);
    }
    // A header that is present but not valid text is hostile, not absent.
    if headers.contains_key(header::ORIGIN) {
        let origin = header(headers, header::ORIGIN).ok_or(Reason::Origin)?;
        let origin_ok = match rule {
            HostRule::Only(hosts) => hosts.iter().any(|h| origin == format!("http://{h}")),
            HostRule::Any => origin == format!("http://{host}"),
        };
        if !origin_ok {
            return Err(Reason::Origin);
        }
    }
    if let Some(site) = headers.get("sec-fetch-site") {
        match site.to_str() {
            Ok("same-origin" | "none") => {}
            _ => return Err(Reason::FetchSite),
        }
    }
    let read_only = matches!(*method, Method::GET | Method::HEAD);
    if !read_only && !headers.contains_key(header::ORIGIN) {
        return Err(Reason::MissingOrigin);
    }
    Ok(())
}

pub async fn guard(State(rule): State<Arc<HostRule>>, req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let route = req.uri().path().to_string();
    let mut resp = match check(req.headers(), &method, &rule) {
        Ok(()) => next.run(req).await,
        Err(reason) => (StatusCode::FORBIDDEN, reason.text()).into_response(),
    };
    resp.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CSP),
    );
    // One line per request. A failed handler adds its kind, and for a 500 the
    // detail the client was not shown.
    let status = resp.status().as_u16();
    match resp.extensions().get::<Logged>() {
        Some(Logged {
            kind,
            detail: Some(d),
        }) => eprintln!(
            "scree web: {method} {route} {status} {kind}: {}",
            d.replace('\n', " ")
        ),
        Some(Logged { kind, detail: None }) => {
            eprintln!("scree web: {method} {route} {status} {kind}")
        }
        None => eprintln!("scree web: {method} {route} {status}"),
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hosts() -> HostRule {
        HostRule::Only(vec!["127.0.0.1:9".into(), "localhost:9".into()])
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn rules() {
        let get = Method::GET;
        let post = Method::POST;
        let ok = [("host", "127.0.0.1:9")];
        assert_eq!(check(&headers(&ok), &get, &hosts()), Ok(()));
        assert_eq!(
            check(&headers(&[("host", "evil.test:9")]), &get, &hosts()),
            Err(Reason::Host)
        );
        assert_eq!(check(&HeaderMap::new(), &get, &hosts()), Err(Reason::Host));
        for origin in ["http://evil.test", "null", "http://127.0.0.1:10"] {
            assert_eq!(
                check(
                    &headers(&[("host", "localhost:9"), ("origin", origin)]),
                    &get,
                    &hosts()
                ),
                Err(Reason::Origin),
                "{origin}"
            );
        }
        for site in ["same-site", "cross-site"] {
            assert_eq!(
                check(
                    &headers(&[("host", "localhost:9"), ("sec-fetch-site", site)]),
                    &get,
                    &hosts()
                ),
                Err(Reason::FetchSite)
            );
        }
        for site in ["same-origin", "none"] {
            assert_eq!(
                check(
                    &headers(&[("host", "localhost:9"), ("sec-fetch-site", site)]),
                    &get,
                    &hosts()
                ),
                Ok(())
            );
        }
        assert_eq!(
            check(&headers(&ok), &post, &hosts()),
            Err(Reason::MissingOrigin)
        );
        assert_eq!(
            check(
                &headers(&[("host", "127.0.0.1:9"), ("origin", "http://127.0.0.1:9")]),
                &post,
                &hosts()
            ),
            Ok(())
        );
    }

    #[test]
    fn bound_address_rule_accepts_only_itself() {
        let rule = HostRule::Only(vec!["192.0.2.1:9".into()]);
        let get = Method::GET;
        assert_eq!(
            check(&headers(&[("host", "192.0.2.1:9")]), &get, &rule),
            Ok(())
        );
        assert_eq!(
            check(&headers(&[("host", "127.0.0.1:9")]), &get, &rule),
            Err(Reason::Host)
        );
    }

    #[test]
    fn any_rule_accepts_every_host_but_needs_one_and_a_matching_origin() {
        let any = HostRule::Any;
        let (get, post) = (Method::GET, Method::POST);
        assert_eq!(
            check(&headers(&[("host", "evil.test")]), &get, &any),
            Ok(())
        );
        assert_eq!(check(&HeaderMap::new(), &get, &any), Err(Reason::Host));
        assert_eq!(
            check(
                &headers(&[("host", "evil.test"), ("origin", "http://evil.test")]),
                &post,
                &any
            ),
            Ok(())
        );
        assert_eq!(
            check(
                &headers(&[("host", "a.test"), ("origin", "http://b.test")]),
                &post,
                &any
            ),
            Err(Reason::Origin)
        );
        assert_eq!(
            check(&headers(&[("host", "a.test")]), &post, &any),
            Err(Reason::MissingOrigin)
        );
        assert_eq!(
            check(
                &headers(&[("host", "a.test"), ("sec-fetch-site", "cross-site")]),
                &get,
                &any
            ),
            Err(Reason::FetchSite)
        );
    }
}
