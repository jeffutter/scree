//! The web UI's error contract and live updates, driven through the
//! `test-routes` feature (`cargo nextest run --features test-routes`).
#![cfg(feature = "test-routes")]

mod common;

use std::time::{Duration, Instant};

use common::Env;

const SECOND: Duration = Duration::from_secs(1);

#[test]
fn refusal_is_422_with_the_fragment_for_its_control() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let r = w.post("/test/refuse");
    assert_eq!(r.status, 422);
    assert!(r.header("content-type").unwrap().starts_with("text/html"));
    assert!(r.body.contains("id=\"err-refuse\""), "{}", r.body);
    assert!(r.body.contains("role=\"alert\""));
    assert!(r.body.contains("this is always refused"));
}

#[test]
fn conflict_is_409_and_no_response_carries_an_error_header() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let c = w.post("/test/conflict");
    assert_eq!(c.status, 409);
    assert!(c.body.contains("id=\"err-conflict\""), "{}", c.body);
    for r in [
        c,
        w.post("/test/refuse"),
        w.get("/test/page"),
        w.get("/stream?view=/nope"),
    ] {
        assert!(r.header("x-scree-error").is_none());
    }
}

/// The proof that the placement mechanism has something to place: the test
/// page ships an element with the id the fragment carries, and loads the
/// script that does the placing. The browser half is in docs/web-ui.md.
#[test]
fn test_page_has_the_target_elements_and_loads_errors_js() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let page = w.get("/test/page");
    assert_eq!(page.status, 200);
    assert!(
        page.body.contains("<div id=\"err-refuse\">"),
        "{}",
        page.body
    );
    assert!(page.body.contains("src=\"/assets/errors.js\""));
    assert!(page.body.contains("@post('/test/refuse')"));
}

#[test]
fn each_request_logs_one_line_with_method_route_status_and_kind() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    w.post("/test/refuse");
    w.post("/test/conflict");
    w.get("/test/page");
    w.get("/stream?view=/nope");
    let deadline = Instant::now() + Duration::from_secs(3);
    let lines = loop {
        let l = w.log();
        if l.iter().filter(|l| l.contains("/stream")).count() == 1 || Instant::now() > deadline {
            break l;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let count = |needle: &str| lines.iter().filter(|l| l.contains(needle)).count();
    assert_eq!(count("POST /test/refuse 422 refused"), 1, "{lines:?}");
    assert_eq!(count("POST /test/conflict 409 conflict"), 1, "{lines:?}");
    assert_eq!(count("GET /test/page 200"), 1, "{lines:?}");
    assert_eq!(count("GET /stream 404 not_found"), 1, "{lines:?}");
    assert_eq!(count("/test/refuse"), 1, "{lines:?}");
}

#[test]
fn unknown_or_missing_view_is_an_error_fragment() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let r = w.get("/stream?view=/etc/passwd");
    assert_eq!(r.status, 404);
    assert!(r.body.contains("id=\"err-stream\""), "{}", r.body);
    assert_eq!(w.get("/stream").status, 400);
    assert_eq!(w.get("/stream?view=test/page").status, 400);
}

#[test]
fn external_write_reaches_an_open_stream_and_an_unchanged_view_is_not_resent() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let mut s = w.stream("/stream?view=/test/page");
    assert_eq!(s.status, 200);
    let first = s.next_event(2 * SECOND).expect("first render");
    assert!(
        first.starts_with("event: datastar-patch-elements\n"),
        "{first}"
    );
    assert!(
        first.contains("data: elements <div id=\"test-live\">open: 0</div>"),
        "{first}"
    );

    let id = e.ok(&["add", "first"])["id"].as_str().unwrap().to_string();
    let t = Instant::now();
    let ev = s.next_event(2 * SECOND).expect("patch after a write");
    assert!(t.elapsed() < 2 * SECOND);
    assert!(ev.contains("open: 1"), "{ev}");

    // A commit that leaves the rendering the same sends nothing.
    e.ok(&["comment", "add", &id, "noted"]);
    assert!(
        s.next_event(Duration::from_millis(1500)).is_none(),
        "unchanged fragment was re-sent"
    );

    // ...and the stream is still alive.
    e.ok(&["add", "second"]);
    assert!(s.next_event(2 * SECOND).unwrap().contains("open: 2"));
}

#[test]
fn sigint_with_open_streams_exits_within_a_second() {
    let e = Env::new();
    let mut w = e.spawn_web(&[], &[]);
    let mut streams: Vec<_> = (0..4)
        .map(|_| w.stream("/stream?view=/test/page"))
        .collect();
    for s in &mut streams {
        s.next_event(2 * SECOND).expect("first render");
    }
    assert!(w.interrupt(SECOND));
}

#[test]
fn the_33rd_stream_is_refused_and_a_closed_one_frees_its_slot() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let mut streams: Vec<_> = (0..32)
        .map(|_| w.stream("/stream?view=/test/page"))
        .collect();
    assert!(streams.iter().all(|s| s.status == 200));
    let refused = w.get("/stream?view=/test/page");
    assert_eq!(refused.status, 503);
    assert!(refused.body.contains("id=\"err-stream\""));
    streams.pop();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        // The server notices the closed socket only when it next writes, so
        // nudge it with a change.
        e.ok(&["add", "nudge"]);
        std::thread::sleep(Duration::from_millis(900));
        let s = w.stream("/stream?view=/test/page");
        if s.status == 200 {
            break;
        }
        assert!(Instant::now() < deadline, "slot was never freed");
    }
}

#[test]
fn unreadable_database_is_a_generic_500_and_the_detail_is_logged() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let secret = "SECRET-NOT-A-DATABASE-MARKER";
    std::fs::write(e.db(), format!("{secret} {}", "x".repeat(200))).unwrap();
    let r = w.get("/");
    assert_eq!(r.status, 500, "{}", r.body);
    assert!(r.body.contains("Something went wrong"), "{}", r.body);
    assert!(!r.body.contains("not a database"), "{}", r.body);
    assert!(
        !r.body.contains(&e.db().display().to_string()),
        "{}",
        r.body
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let line = loop {
        let l = w.log().into_iter().find(|l| l.contains("GET / 500"));
        if l.is_some() || Instant::now() > deadline {
            break l;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let line = line.expect("a 500 log line");
    assert!(line.contains("not a database"), "{line}");
}
