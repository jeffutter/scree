//! Status transitions, claim and edit, run through the real binary.

use std::io::Write;
use std::path::{Path, PathBuf};
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

    /// Run with `input` on stdin; returns the exit code.
    fn stdin_code(&self, args: &[&str], input: &str) -> i32 {
        self.stdin_result(args, input).0
    }

    /// Run with `input` on stdin; returns (exit code, stderr string).
    fn stdin_result(&self, args: &[&str], input: &str) -> (i32, String) {
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
        (
            out.status.code().unwrap(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn code(&self, args: &[&str]) -> i32 {
        self.run(args).status.code().unwrap()
    }

    /// A ready ticket in the LOCAL Project; its id.
    fn ready(&self, extra: &[&str]) -> String {
        let mut a = vec!["--role", "human", "add", "t", "--status", "ready"];
        a.extend_from_slice(extra);
        self.ok(&a)["id"].as_str().unwrap().to_string()
    }

    fn sql(&self, q: &str) {
        rusqlite::Connection::open(self.db())
            .unwrap()
            .execute_batch(q)
            .unwrap();
    }
}

#[test]
fn happy_path() {
    let e = env();
    let id = e.ok(&["--role", "human", "add", "t"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(e.code(&["ready", &id, "--role", "agent"]), 0);
    assert_eq!(e.code(&["ready", &id]), 4);
    let t = e.ok(&["start", &id, "--author", "bob", "--session", "s1"]);
    assert_eq!(t["status"], "active");
    assert_eq!(t["claimed_by"]["author"], "bob");
    assert_eq!(t["claimed_by"]["session"], "s1");
    assert!(t["criteria"].is_array() && t["comments"].is_array());
    assert!(t.get("description").is_some() && t.get("plan").is_some());
    let t = e.ok(&["release", &id]);
    assert_eq!(t["status"], "ready");
    assert!(t["claimed_by"].is_null());
    assert_eq!(e.code(&["release", &id]), 4);
    e.ok(&["start", &id]);
    let t = e.ok(&["done", &id]);
    assert_eq!(t["status"], "done");
    assert!(t["done_at"].is_string());
    assert!(t["claimed_by"].is_null());
    assert_eq!(e.code(&["done", &id]), 4);
    let t = e.ok(&["reopen", &id]);
    assert_eq!(t["status"], "ready");
    assert!(t["done_at"].is_null());
    assert_eq!(e.code(&["reopen", &id]), 4);
}

#[test]
fn abandon_stores_reason_comment() {
    let e = env();
    let id = e.ready(&[]);
    assert_eq!(e.code(&["abandon", &id, " "]), 2);
    let t = e.ok(&["abandon", &id, "not needed", "--author", "ann"]);
    assert_eq!(t["status"], "abandoned");
    assert_eq!(t["comments"][0]["body"], "not needed");
    assert_eq!(t["comments"][0]["author"], "ann");
    assert_eq!(e.code(&["abandon", &id, "again"]), 4);
    assert_eq!(e.ok(&["reopen", &id])["status"], "ready");
}

#[test]
fn start_without_session_reads_null() {
    let e = env();
    let id = e.ready(&[]);
    let t = e.ok(&["start", &id, "--author", "bob"]);
    assert_eq!(t["status"], "active");
    assert_eq!(t["claimed_by"]["author"], "bob");
    assert!(t["claimed_by"]["session"].is_null());
    assert_eq!(e.ok(&["show", &id])["claimed_by"]["author"], "bob");
}

#[test]
fn start_exit_codes() {
    let e = env();
    let draft = e.ok(&["--role", "human", "add", "d"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(e.code(&["start", &draft]), 4);

    let a = e.ready(&[]);
    e.ok(&["start", &a]);
    assert_eq!(e.code(&["start", &a]), 5);
    e.ok(&["done", &a]);
    assert_eq!(e.code(&["start", &a]), 4);

    let blocker = e.ready(&[]);
    let blocked = e.ready(&[]);
    // LOCAL-numbered ids: ticket numbers equal row ids here.
    let n = |id: &str| id.rsplit('-').next().unwrap().to_string();
    e.sql(&format!(
        "INSERT INTO dependencies (ticket_id, blocked_by_id) VALUES ({}, {})",
        n(&blocked),
        n(&blocker)
    ));
    assert_eq!(e.code(&["start", &blocked]), 4);
    e.ok(&["start", &blocker]);
    e.ok(&["done", &blocker]);
    assert_eq!(e.code(&["start", &blocked]), 0);

    let h = e.ready(&["--for", "human"]);
    assert_eq!(e.code(&["--role", "agent", "start", &h]), 4);
    assert_eq!(e.code(&["--role", "human", "start", &h]), 0);
    assert_eq!(e.code(&["start", "LOCAL-999"]), 3);
}

#[test]
fn done_needs_criteria_checked_or_force() {
    let e = env();
    let id = e.ready(&["--criterion", "works"]);
    assert_eq!(e.code(&["done", &id]), 4);
    assert_eq!(e.ok(&["done", &id, "--force"])["status"], "done");
}

#[test]
fn edit_fields_and_append() {
    let e = env();
    let id = e.ready(&["--description", "one"]);
    assert_eq!(e.code(&["edit", &id]), 2);
    assert_eq!(e.code(&["edit", &id, "--append"]), 2);
    assert_eq!(e.code(&["edit", &id, "--append", "--title", "x"]), 2);
    assert_eq!(e.code(&["edit", &id, "--title", " "]), 2);

    // Regression: --append with empty or whitespace-only text must exit 2 (SCR-21).
    let before = e.ok(&["show", &id]);
    assert_eq!(e.code(&["edit", &id, "--append", "--description", ""]), 2);
    assert_eq!(e.code(&["edit", &id, "--append", "--plan", "  "]), 2);
    assert_eq!(
        e.stdin_code(&["edit", &id, "--append", "--description", "-"], ""),
        2
    );
    let after = e.ok(&["show", &id]);
    assert_eq!(after["description"], before["description"]);
    assert_eq!(after["plan"], before["plan"]);

    let t = e.ok(&["edit", &id, "--title", " new ", "--plan", "p1"]);
    assert_eq!(t["title"], "new");
    assert_eq!(t["plan"], "p1");
    let t = e.ok(&["edit", &id, "--description", "two", "--append"]);
    assert_eq!(t["description"], "one\n\ntwo");
    let t = e.ok(&["edit", &id, "--description", "three"]);
    assert_eq!(t["description"], "three");
    let t = e.ok(&["edit", &id, "--plan", "p2", "--append"]);
    assert_eq!(t["plan"], "p1\n\np2");

    // Append to an empty description: no leading blank line.
    let id2 = e.ready(&[]);
    let t = e.ok(&["edit", &id2, "--description", "d", "--append"]);
    assert_eq!(t["description"], "d");

    // Append to an empty plan: no leading blank line.
    let id3 = e.ready(&[]);
    let t = e.ok(&["edit", &id3, "--plan", "p", "--append"]);
    assert_eq!(t["plan"], "p");

    // Replacing with an empty string still clears the field (escape hatch preserved).
    let t = e.ok(&["edit", &id, "--description", ""]);
    assert_eq!(t["description"], "");
}

#[test]
fn edit_for_follows_identity_rules() {
    let e = env();
    let id = e.ready(&[]);
    assert_eq!(
        e.code(&["--role", "human", "edit", &id, "--for", "human"]),
        0
    );
    // An agent may not hand a ticket back to agents.
    assert_eq!(
        e.code(&["--role", "agent", "edit", &id, "--for", "agent"]),
        4
    );
    assert_eq!(
        e.code(&["--role", "human", "edit", &id, "--for", "agent"]),
        0
    );
    assert_eq!(
        e.code(&["--role", "agent", "edit", &id, "--for", "human"]),
        0
    );
    // Reassigning an active ticket releases the claim.
    let id = e.ready(&[]);
    e.ok(&["start", &id, "--author", "bob"]);
    let t = e.ok(&["--role", "agent", "edit", &id, "--for", "human"]);
    assert_eq!(t["for"], "human");
    assert_eq!(t["status"], "ready");
    assert!(t["claimed_by"].is_null());
    // Same value on an active ticket keeps the claim.
    let id = e.ready(&[]);
    e.ok(&["start", &id]);
    let t = e.ok(&["edit", &id, "--for", "agent"]);
    assert_eq!(t["status"], "active");
}

#[test]
fn concurrent_start_has_one_winner() {
    let e = env();
    let id = e.ready(&[]);
    let exe = env!("CARGO_BIN_EXE_scree");
    let db = e.db();
    let cwd: &Path = e.dir.path();
    let codes: Vec<i32> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..8)
            .map(|i| {
                let id = id.clone();
                let db = db.clone();
                s.spawn(move || {
                    Command::new(exe)
                        .current_dir(cwd)
                        .env_remove("SCREE_DB")
                        .arg("--db")
                        .arg(db)
                        .args([
                            "--role",
                            "agent",
                            "--author",
                            &format!("a{i}"),
                            "start",
                            &id,
                        ])
                        .output()
                        .unwrap()
                        .status
                        .code()
                        .unwrap()
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    assert_eq!(codes.iter().filter(|c| **c == 0).count(), 1, "{codes:?}");
    assert_eq!(codes.iter().filter(|c| **c == 5).count(), 7, "{codes:?}");
    assert_eq!(e.ok(&["show", &id])["status"], "active");
}

#[test]
fn empty_stdin_is_refused() {
    // Regression (SCR-28): every `-` flag must exit 2 on an empty stdin read,
    // naming the flag in the error message, and leaving stored data unchanged.
    let e = env();
    let id = e.ready(&[
        "--description",
        "keep",
        "--plan",
        "pkeep",
        "--criterion",
        "works",
    ]);

    // Snapshot before: used to verify byte-identity after each failed command.
    let before = e.ok(&["show", &id]);

    // edit --description -
    let (code, err) = e.stdin_result(&["edit", &id, "--description", "-"], "");
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("--description"), "should name flag: {err}");
    assert!(err.contains("read nothing from stdin"), "{err}");

    // edit --plan -
    let (code, err) = e.stdin_result(&["edit", &id, "--plan", "-"], "");
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("--plan"), "should name flag: {err}");

    // comment add ID -
    let (code, err) = e.stdin_result(&["comment", "add", &id, "-"], "");
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("comment body"), "should name arg: {err}");

    // abandon ID -
    let (code, err) = e.stdin_result(&["abandon", &id, "-"], "");
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("abandon reason"), "should name arg: {err}");

    // check REF --evidence -
    let (code, err) = e.stdin_result(&["check", &format!("{id}#1"), "--evidence", "-"], "");
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("--evidence"), "should name flag: {err}");

    // After all five failures the ticket is byte-identical with updated_at unchanged.
    let after = e.ok(&["show", &id]);
    assert_eq!(after["description"], before["description"]);
    assert_eq!(after["plan"], before["plan"]);
    assert_eq!(after["updated_at"], before["updated_at"]);
    assert_eq!(after["status"], before["status"]);
    assert_eq!(after["criteria"][0]["checked"], false);

    // add T --description - with empty stdin also exits 2 (no ticket created).
    let count_before = e.ok(&["list"])["tickets"].as_array().unwrap().len();
    let (code, err) = e.stdin_result(
        &["--role", "human", "add", "new-ticket", "--description", "-"],
        "",
    );
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("--description"), "should name flag: {err}");
    let count_after = e.ok(&["list"])["tickets"].as_array().unwrap().len();
    assert_eq!(count_after, count_before, "no ticket should be created");
}
