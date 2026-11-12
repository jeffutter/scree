//! Position, `reorder`, `next` and `start --next`, through the real binary.

use std::path::PathBuf;
use std::process::{Command, Output};

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

    fn run(&self, args: &[&str]) -> Output {
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
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn code(&self, args: &[&str]) -> i32 {
        self.run(args).status.code().unwrap()
    }

    /// Add a ticket as a human; its id.
    fn add(&self, title: &str, extra: &[&str]) -> String {
        let mut a = vec!["--role", "human", "add", title];
        a.extend_from_slice(extra);
        self.ok(&a)["id"].as_str().unwrap().to_string()
    }

    fn ids(&self, args: &[&str]) -> Vec<String> {
        let mut a = vec!["list"];
        a.extend_from_slice(args);
        self.ok(&a)["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    }

    fn positions(&self) -> Vec<i64> {
        let conn = rusqlite::Connection::open(self.db()).unwrap();
        let mut st = conn
            .prepare("SELECT position FROM tickets ORDER BY position, number")
            .unwrap();
        st.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn sql(&self, q: &str) {
        rusqlite::Connection::open(self.db())
            .unwrap()
            .execute_batch(q)
            .unwrap();
    }

    fn next_ids(&self, args: &[&str]) -> (Vec<String>, Value) {
        let mut a = vec!["next"];
        a.extend_from_slice(args);
        let v = self.ok(&a);
        let ids = v["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect();
        (ids, v)
    }
}

#[test]
fn new_tickets_step_by_1000() {
    let e = env();
    e.add("a", &[]);
    e.add("b", &[]);
    e.add("c", &[]);
    assert_eq!(e.positions(), [1000, 2000, 3000]);
}

#[test]
fn reorder_first_last_before_after() {
    let e = env();
    let a = e.add("a", &[]);
    let b = e.add("b", &[]);
    let c = e.add("c", &[]);
    let d = e.add("d", &[]);
    assert_eq!(e.ids(&[]), [a.as_str(), b.as_str(), c.as_str(), d.as_str()]);
    e.ok(&["reorder", &d, "--first"]);
    assert_eq!(e.ids(&[]), [d.as_str(), a.as_str(), b.as_str(), c.as_str()]);
    e.ok(&["reorder", &d, "--last"]);
    assert_eq!(e.ids(&[]), [a.as_str(), b.as_str(), c.as_str(), d.as_str()]);
    e.ok(&["reorder", &c, "--before", &a]);
    assert_eq!(e.ids(&[]), [c.as_str(), a.as_str(), b.as_str(), d.as_str()]);
    let t = e.ok(&["reorder", &c, "--after", &b]);
    assert_eq!(t["id"].as_str().unwrap(), c);
    assert_eq!(e.ids(&[]), [a.as_str(), b.as_str(), c.as_str(), d.as_str()]);
}

#[test]
fn repeated_first_exhausts_the_gap_and_renumbers() {
    let e = env();
    let mut expected = vec![e.add("t0", &[])];
    for i in 1..16 {
        let id = e.add(&format!("t{i}"), &[]);
        e.ok(&["reorder", &id, "--first"]);
        expected.insert(0, id);
        assert_eq!(e.ids(&[]), expected);
        let p = e.positions();
        assert!(p.windows(2).all(|w| w[0] < w[1]) && p[0] > 0, "{p:?}");
    }
    // 1000 halves to 1 in ten steps, so a renumber has happened by now.
    assert!(e.positions().iter().all(|p| p % 1000 == 0 || *p > 1));
}

#[test]
fn reorder_renumbers_in_steps_of_1000() {
    let e = env();
    let a = e.add("a", &[]);
    let b = e.add("b", &[]);
    let c = e.add("c", &[]);
    e.sql(
        "UPDATE tickets SET position = 1 WHERE title = 'a';
           UPDATE tickets SET position = 2 WHERE title = 'b';
           UPDATE tickets SET position = 3 WHERE title = 'c'",
    );
    e.ok(&["reorder", &c, "--before", &b]);
    assert_eq!(e.positions(), [1000, 2000, 3000]);
    assert_eq!(e.ids(&[]), [a.as_str(), c.as_str(), b.as_str()]);
}

#[test]
fn reorder_refusals() {
    let e = env();
    let a = e.add("a", &[]);
    let b = e.add("b", &[]);
    let done = e.add("done", &["--status", "ready"]);
    e.ok(&["done", &done]);
    let gone = e.add("gone", &[]);
    e.ok(&["abandon", &gone, "no"]);
    assert_eq!(e.code(&["reorder", &done, "--first"]), 4);
    assert_eq!(e.code(&["reorder", &gone, "--last"]), 4);
    assert_eq!(e.code(&["reorder", &a, "--before", &a]), 4);
    assert_eq!(e.code(&["reorder", &a, "--before", "XX-99"]), 3);
    // Done siblings still hold a place and may be a target.
    assert_eq!(e.code(&["reorder", &b, "--after", &done]), 0);
    // Usage errors: no placement, or more than one.
    assert_eq!(e.code(&["reorder", &a]), 2);
    assert_eq!(e.code(&["reorder", &a, "--first", "--last"]), 2);
    assert_eq!(e.code(&["reorder", &a, "--first", "--before", &b]), 2);
}

#[test]
fn reorder_refuses_a_target_in_another_sibling_set() {
    let e = env();
    let a = e.add("a", &[]);
    let b = e.add("b", &[]);
    e.sql("UPDATE tickets SET worktree_path = '/elsewhere' WHERE title = 'b'");
    assert_eq!(e.code(&["reorder", &a, "--before", &b]), 4);
}

#[test]
fn next_skips_unavailable_and_reports_empty() {
    let e = env();
    let (ids, v) = e.next_ids(&[]);
    assert!(ids.is_empty());
    assert_eq!(v["human_pending"], 0);
    assert_eq!(v["blocked_pending"], 0);

    let draft = e.add("draft", &[]);
    let active = e.add("active", &["--status", "ready"]);
    e.ok(&["start", &active]);
    let done = e.add("done", &["--status", "ready"]);
    e.ok(&["done", &done]);
    let _ = draft;
    assert!(e.next_ids(&[]).0.is_empty());

    let ready = e.add("ready", &["--status", "ready"]);
    let later = e.add("later", &["--status", "ready"]);
    let (ids, v) = e.next_ids(&[]);
    assert_eq!(ids, [ready.as_str()]);
    assert!(v.get("blocked_pending").is_none());
    assert_eq!(
        e.next_ids(&["--limit", "5"]).0,
        [ready.as_str(), later.as_str()]
    );
    e.ok(&["reorder", &later, "--first"]);
    assert_eq!(e.next_ids(&[]).0, [later.as_str()]);
}

#[test]
fn next_excludes_blocked_and_counts_them() {
    let e = env();
    let blocker = e.add("blocker", &[]);
    let waits = e.add("waits", &["--status", "ready"]);
    e.sql(
        "INSERT INTO dependencies (ticket_id, blocked_by_id)
           SELECT w.id, b.id FROM tickets w, tickets b
           WHERE w.title = 'waits' AND b.title = 'blocker'",
    );
    let (ids, v) = e.next_ids(&[]);
    assert!(ids.is_empty());
    assert_eq!(v["blocked_pending"], 1);
    assert_eq!(e.code(&["start", &waits]), 4);
    assert_eq!(e.code(&["start", "--next"]), 3);
    e.ok(&["ready", &blocker]);
    e.ok(&["start", &blocker]);
    e.ok(&["done", &blocker]);
    assert_eq!(e.next_ids(&[]).0, [waits.as_str()]);
}

#[test]
fn next_respects_role_and_for() {
    let e = env();
    let human = e.add("h", &["--status", "ready", "--for", "human"]);
    let agent = e.add("a", &["--status", "ready"]);
    let (ids, v) = e.next_ids(&["--role", "agent"]);
    assert_eq!(ids, [agent.as_str()]);
    assert_eq!(v["human_pending"], 1);
    e.ok(&["start", &agent]);
    let (ids, v) = e.next_ids(&["--role", "agent"]);
    assert!(ids.is_empty());
    assert_eq!(v["human_pending"], 1);
    let (ids, v) = e.next_ids(&["--role", "agent", "--for", "human"]);
    assert_eq!(ids, [human.as_str()]);
    assert_eq!(v["human_pending"], 0);
    assert_eq!(e.next_ids(&["--role", "human"]).0, [human.as_str()]);
    // An agent may not claim a human ticket, by --for or otherwise.
    assert_eq!(
        e.code(&["--role", "agent", "start", "--next", "--for", "human"]),
        4
    );
    assert_eq!(e.code(&["--role", "agent", "start", "--next"]), 3);
    assert_eq!(
        e.ok(&["--role", "human", "start", "--next"])["id"],
        human.as_str()
    );
}

#[test]
fn next_text_mentions_hidden_human_tickets() {
    let e = env();
    e.add("h", &["--status", "ready", "--for", "human"]);
    let out = e.run(&["--text", "--role", "agent", "next"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("1 ticket(s) for a human not shown"), "{text}");
}

#[test]
fn start_next_claims_in_order() {
    let e = env();
    let a = e.add("a", &["--status", "ready"]);
    let b = e.add("b", &["--status", "ready"]);
    let t = e.ok(&["start", "--next", "--author", "bob", "--session", "s"]);
    assert_eq!(t["id"], a.as_str());
    assert_eq!(t["status"], "active");
    assert_eq!(t["claimed_by"]["author"], "bob");
    assert_eq!(e.ok(&["start", "--next"])["id"], b.as_str());
    assert_eq!(e.code(&["start", "--next"]), 3);
}

#[test]
fn start_next_argument_rules() {
    let e = env();
    let a = e.add("a", &["--status", "ready"]);
    assert_eq!(e.code(&["start"]), 2);
    assert_eq!(e.code(&["start", &a, "--next"]), 2);
    assert_eq!(e.code(&["start", &a, "--for", "agent"]), 2);
}

#[test]
fn concurrent_start_next_claims_distinct_tickets() {
    let e = env();
    for i in 0..4 {
        e.add(&format!("t{i}"), &["--status", "ready"]);
    }
    let db = e.db();
    let dir = e.dir.path().to_path_buf();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let (db, dir) = (db.clone(), dir.clone());
            std::thread::spawn(move || {
                let out = Command::new(env!("CARGO_BIN_EXE_scree"))
                    .current_dir(dir)
                    .env_remove("SCREE_DB")
                    .arg("--db")
                    .arg(db)
                    .args(["start", "--next"])
                    .output()
                    .unwrap();
                assert_eq!(out.status.code(), Some(0), "{out:?}");
                let v: Value = serde_json::from_slice(&out.stdout).unwrap();
                v["id"].as_str().unwrap().to_string()
            })
        })
        .collect();
    let mut got: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    got.sort();
    got.dedup();
    assert_eq!(got.len(), 4);
}
