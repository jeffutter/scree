//! Byte-exact JSON for show, list and next. Keys are alphabetical because
//! serde_json has no `preserve_order`; the other tests parse into `Value`,
//! which ignores key order, so only this file catches a reordering.
//! Regenerate with `GOLDEN_BLESS=1 cargo nextest run --test golden`.

mod common;

use common::Env;

/// Every golden command carries these globals.
const FLAGS: &[&str] = &["--author", "bob", "--session", "sess1", "--role", "agent"];

/// Replace every `YYYY-MM-DDTHH:MM:SSZ` with `TS`.
fn redact(s: &str) -> String {
    let b = s.as_bytes();
    let shape = b"dddd-dd-ddTdd:dd:ddZ";
    let mut out = String::new();
    let mut i = 0;
    while i < b.len() {
        let hit = i + shape.len() <= b.len()
            && shape.iter().enumerate().all(|(j, c)| match c {
                b'd' => b[i + j].is_ascii_digit(),
                c => b[i + j] == *c,
            });
        if hit {
            out.push_str("TS");
            i += shape.len();
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

fn check(name: &str, actual: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("GOLDEN_BLESS").is_some() {
        std::fs::write(&path, actual).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap();
    assert_eq!(actual, want, "{name}");
}

#[test]
fn json_key_order_is_stable() {
    let e = Env::with_flags(FLAGS);
    e.stdout(&[
        "add",
        "Parent",
        "--description",
        "desc",
        "--plan",
        "the plan",
        "--criterion",
        "first",
        "--criterion",
        "second",
    ]);
    e.stdout(&[
        "add",
        "Waits",
        "--blocked-by",
        "LOCAL-1",
        "--status",
        "ready",
    ]);
    e.stdout(&["ready", "LOCAL-1"]);
    e.stdout(&["start", "LOCAL-1"]);
    e.stdout(&["check", "LOCAL-1#1", "--evidence", "ran it"]);
    e.stdout(&["comment", "add", "LOCAL-1", "a note"]);
    e.stdout(&["add", "Free", "--status", "ready"]);
    e.stdout(&["add", "Sub", "--parent", "LOCAL-3"]);
    e.stdout(&["add", "Open", "--status", "ready"]);

    check("show_detail.json", &redact(&e.stdout(&["show", "LOCAL-1"])));
    check(
        "show_blocked.json",
        &redact(&e.stdout(&["show", "LOCAL-2"])),
    );
    check(
        "show_subtasks.json",
        &redact(&e.stdout(&["show", "LOCAL-3"])),
    );
    check("list.json", &redact(&e.stdout(&["list"])));
    check("next.json", &redact(&e.stdout(&["next"])));
}

#[test]
fn show_payload_round_trips_through_typed_model() {
    for name in [
        "show_detail.json",
        "show_blocked.json",
        "show_subtasks.json",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(name);
        let raw = std::fs::read_to_string(path).unwrap();
        let raw = raw.trim_end();
        let t: scree::model::TicketDetail = serde_json::from_str(raw).unwrap();
        assert_eq!(serde_json::to_string(&t).unwrap(), raw, "{name}");
    }
    let raw = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/next.json"),
    )
    .unwrap();
    let raw = raw.trim_end();
    let n: scree::model::NextResult = serde_json::from_str(raw).unwrap();
    assert_eq!(serde_json::to_string(&n).unwrap(), raw);
}
