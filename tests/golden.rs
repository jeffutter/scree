//! Byte-exact JSON for show, list and next. Keys are alphabetical because
//! serde_json has no `preserve_order`; the other tests parse into `Value`,
//! which ignores key order, so only this file catches a reordering.
//! Regenerate with `GOLDEN_BLESS=1 cargo nextest run --test golden`.
use std::process::Command;

fn run(dir: &std::path::Path, args: &[&str]) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_scree"));
    for (k, _) in std::env::vars() {
        if k.starts_with("SCREE_")
            || k.starts_with("GIT_")
            || k == "CLAUDECODE"
            || k == "AI_AGENT"
            || k == "NO_COLOR"
            || k == "CLAUDE_CODE_SESSION_ID"
        {
            cmd.env_remove(k);
        }
    }
    let out = cmd
        .current_dir(dir)
        .arg("--db")
        .arg(dir.join("s.db"))
        .args(["--author", "bob", "--session", "sess1", "--role", "agent"])
        .args(args)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{args:?}: {:?}", out);
    redact(&String::from_utf8(out.stdout).unwrap())
}

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
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    run(
        d,
        &[
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
        ],
    );
    run(
        d,
        &[
            "add",
            "Waits",
            "--blocked-by",
            "LOCAL-1",
            "--status",
            "ready",
        ],
    );
    run(d, &["ready", "LOCAL-1"]);
    run(d, &["start", "LOCAL-1"]);
    run(d, &["check", "LOCAL-1#1", "--evidence", "ran it"]);
    run(d, &["comment", "add", "LOCAL-1", "a note"]);
    run(d, &["add", "Free", "--status", "ready"]);
    run(d, &["add", "Sub", "--parent", "LOCAL-3"]);
    run(d, &["add", "Open", "--status", "ready"]);

    check("show_detail.json", &run(d, &["show", "LOCAL-1"]));
    check("show_blocked.json", &run(d, &["show", "LOCAL-2"]));
    check("show_subtasks.json", &run(d, &["show", "LOCAL-3"]));
    check("list.json", &run(d, &["list"]));
    check("next.json", &run(d, &["next"]));
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
