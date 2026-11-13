//! Criteria and comments, run through the real binary.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

struct Env {
    dir: tempfile::TempDir,
}

fn env() -> Env {
    Env {
        dir: tempfile::tempdir().unwrap(),
    }
}

impl Env {
    fn db(&self) -> PathBuf {
        self.dir.path().join("scree.db")
    }

    fn cmd(&self, args: &[&str]) -> Command {
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
        cmd.current_dir(self.dir.path())
            .arg("--db")
            .arg(self.db())
            .args(args);
        cmd
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    fn stdin(&self, args: &[&str], input: &str) -> Value {
        use std::io::Write;
        let mut child = self
            .cmd(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn code(&self, args: &[&str]) -> i32 {
        self.run(args).status.code().unwrap()
    }
}

fn numbers(t: &Value) -> Vec<i64> {
    t["criteria"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["number"].as_i64().unwrap())
        .collect()
}

fn new_ticket(e: &Env) -> String {
    e.ok(&[
        "--role",
        "human",
        "add",
        "t",
        "--status",
        "ready",
        "--criterion",
        "a",
        "--criterion",
        "b",
    ])["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn numbers_are_never_reused() {
    let e = env();
    let id = new_ticket(&e);
    let t = e.ok(&["criteria", "add", &id, "c"]);
    assert_eq!(numbers(&t), [1, 2, 3]);
    assert_eq!(t["criteria"][2]["ref"], format!("{id}#3"));
    let t = e.ok(&["criteria", "rm", &format!("{id}#2")]);
    assert_eq!(numbers(&t), [1, 3]);
    let t = e.ok(&["criteria", "add", &id, "d"]);
    assert_eq!(numbers(&t), [1, 3, 4]);
    assert_eq!(e.code(&["criteria", "add", &id, "  "]), 2);
}

#[test]
fn list_and_next_carry_a_criteria_summary() {
    let e = env();
    let id = new_ticket(&e);
    let bare = e.ok(&["--role", "human", "add", "bare", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    e.ok(&["check", &format!("{id}#1"), "--evidence", "ok"]);
    let summary = |v: &Value, want: &str| {
        v["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == want)
            .unwrap()["criteria"]
            .clone()
    };
    for cmd in ["list", "next"] {
        let v = e.ok(&[cmd]);
        assert_eq!(
            summary(&v, &id),
            serde_json::json!({"checked": 1, "total": 2})
        );
        if cmd == "list" {
            assert_eq!(
                summary(&v, &bare),
                serde_json::json!({"checked": 0, "total": 0})
            );
        }
    }
    let shown = e.ok(&["show", &id]);
    assert_eq!(shown["criteria"].as_array().unwrap().len(), 2);
}

#[test]
fn check_and_uncheck() {
    let e = env();
    let id = new_ticket(&e);
    let r1 = format!("{id}#1");
    let t = e.ok(&[
        "check",
        &r1,
        "--evidence",
        "ran it",
        "--author",
        "bob",
        "--session",
        "s1",
    ]);
    let c = &t["criteria"][0];
    assert_eq!(c["checked"], true);
    assert_eq!(c["evidence"], "ran it");
    assert_eq!(c["checked_by"]["author"], "bob");
    assert_eq!(c["checked_by"]["session"], "s1");
    assert!(c["checked_at"].is_string());

    let t = e.stdin(
        &["check", &format!("{id}#2"), "--evidence", "-"],
        "from stdin",
    );
    assert_eq!(t["criteria"][1]["evidence"], "from stdin");

    let id3 = new_ticket(&e);
    let t = e.stdin(&["check", &format!("{id3}#1"), "--evidence", "-"], "ok\n");
    assert_eq!(t["criteria"][0]["evidence"], "ok");
    assert_eq!(e.code(&["check", &format!("{id3}#2"), "--evidence", ""]), 2);
    assert_eq!(
        e.code(&["check", &format!("{id3}#2"), "--evidence", "  "]),
        2
    );

    let t = e.ok(&["uncheck", &r1]);
    let c = &t["criteria"][0];
    assert_eq!(c["checked"], false);
    assert!(c["evidence"].is_null());
    assert!(c["checked_by"].is_null());
    assert!(c["checked_at"].is_null());
}

#[test]
fn done_needs_checked_criteria() {
    let e = env();
    let id = new_ticket(&e);
    assert_eq!(e.code(&["done", &id]), 4);
    e.ok(&["check", &format!("{id}#1")]);
    assert_eq!(e.code(&["done", &id]), 4);
    e.ok(&["check", &format!("{id}#2")]);
    assert_eq!(e.code(&["done", &id]), 0);

    let id = new_ticket(&e);
    assert_eq!(e.code(&["done", &id, "--force"]), 0);
}

#[test]
fn comments() {
    let e = env();
    let id = new_ticket(&e);
    let t = e.ok(&["comment", "add", &id, "hello", "--author", "bob"]);
    assert_eq!(t["comments"][0]["body"], "hello");
    assert_eq!(t["comments"][0]["author"], "bob");
    let t = e.stdin(&["comment", "add", &id, "-"], "piped");
    assert_eq!(t["comments"][1]["body"], "piped");
    let t = e.stdin(&["comment", "add", &id, "-"], "note\n");
    assert_eq!(t["comments"][2]["body"], "note");
    let first_id = t["comments"][2]["id"].to_string();
    e.ok(&["comment", "rm", &id, &first_id]);
    assert_eq!(e.code(&["comment", "add", &id, ""]), 2);

    let first = t["comments"][0]["id"].to_string();
    let t = e.ok(&["comment", "rm", &id, &first]);
    assert_eq!(t["comments"].as_array().unwrap().len(), 1);

    // A comment scree wrote on abandon can be deleted too.
    let t = e.ok(&["abandon", &id, "nope"]);
    let abandon = t["comments"][1]["id"].to_string();
    let t = e.ok(&["comment", "rm", &id, &abandon]);
    assert_eq!(t["comments"].as_array().unwrap().len(), 1);

    assert_eq!(e.code(&["comment", "rm", &id, "9999"]), 3);
}

#[test]
fn bad_refs() {
    let e = env();
    let id = new_ticket(&e);
    assert_eq!(e.code(&["check", &id]), 2);
    assert_eq!(e.code(&["check", &format!("{id}#x")]), 2);
    assert_eq!(e.code(&["check", &format!("{id}#9")]), 3);
    assert_eq!(e.code(&["uncheck", &format!("{id}#9")]), 3);
    assert_eq!(e.code(&["criteria", "rm", &format!("{id}#9")]), 3);
    assert_eq!(e.code(&["check", "ZZZ-9#1"]), 3);
}
