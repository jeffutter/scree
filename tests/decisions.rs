//! The `decision` command group, through the real binary against throwaway repos.

mod common;

use std::path::{Path, PathBuf};

use common::{Env, git_cmd};

impl Env {
    fn repo(&self, name: &str) -> PathBuf {
        let dir = self.path().join("repos").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for args in [
            &["init", "-q"][..],
            &["commit", "-q", "--allow-empty", "-m", "i"],
        ] {
            assert!(git_cmd(&dir).args(args).status().unwrap().success());
        }
        std::fs::canonicalize(dir).unwrap()
    }

    fn ticket(&self, cwd: &Path, title: &str) -> String {
        self.ok_at(cwd, &["--role", "human", "add", title, "--status", "ready"])["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn human(&self, cwd: &Path, args: &[&str]) -> i32 {
        let mut a = vec!["--role", "human", "--author", "jeff"];
        a.extend_from_slice(args);
        self.code_at(cwd, &a)
    }
}

const KEYS: [&str; 13] = [
    "answer",
    "created_at",
    "decided_at",
    "decided_by",
    "id",
    "number",
    "options",
    "question",
    "rationale",
    "recommendation",
    "status",
    "superseded_by",
    "tickets",
];

#[test]
fn add_link_unlink_answer_supersede_emit_typed_json() {
    let e = Env::new();
    let repo = e.repo("scree");
    let (a, b) = (e.ticket(&repo, "a"), e.ticket(&repo, "b"));

    let d = e.ok_at(
        &repo,
        &[
            "decision",
            "add",
            &a,
            "Which store?",
            "--option",
            "sqlite",
            "--option",
            "postgres",
            "--recommend",
            "2",
        ],
    );
    let keys: Vec<&str> = d.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, KEYS);
    assert_eq!(d["id"], "D-1");
    assert_eq!(d["status"], "open");
    assert_eq!(d["options"], serde_json::json!(["sqlite", "postgres"]));
    assert_eq!(d["recommendation"], "postgres");
    assert_eq!(d["tickets"], serde_json::json!([a]));

    let d = e.ok_at(&repo, &["decision", "link", "D-1", &b]);
    assert_eq!(d["tickets"], serde_json::json!([a, b]));
    // Idempotent.
    e.ok_at(&repo, &["decision", "link", "D-1", &b]);
    assert_eq!(e.ok_at(&repo, &["show", &b])["decisions"][0]["id"], "D-1");

    let d = e.ok_at(&repo, &["decision", "unlink", "D-1", &b]);
    assert_eq!(d["tickets"], serde_json::json!([a]));
    e.ok_at(&repo, &["decision", "unlink", "D-1", &b]);

    let d = e.ok_at(
        &repo,
        &[
            "--role",
            "human",
            "--author",
            "jeff",
            "decision",
            "answer",
            "D-1",
            "sqlite",
            "--rationale",
            "no server",
        ],
    );
    assert_eq!(d["status"], "decided");
    assert_eq!(d["answer"], "sqlite");
    assert_eq!(d["rationale"], "no server");

    e.ok_at(&repo, &["decision", "add", &a, "Which format?"]);
    let d = e.ok_at(&repo, &["decision", "supersede", "D-1", "--by", "D-2"]);
    assert_eq!(d["superseded_by"], "D-2");
    assert_eq!(
        e.ok_at(&repo, &["show", &a])["decisions"][0]["superseded_by"],
        "D-2"
    );
}

#[test]
fn agent_cannot_answer_human_records_author_and_time() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.ticket(&repo, "a");
    e.ok_at(&repo, &["decision", "add", &a, "Q?"]);

    assert_eq!(
        e.code_at(
            &repo,
            &["--role", "agent", "decision", "answer", "D-1", "x"]
        ),
        4
    );
    // An agent is refused before the id is even looked up.
    assert_eq!(
        e.code_at(
            &repo,
            &["--role", "agent", "decision", "answer", "D-9", "x"]
        ),
        4
    );
    assert_eq!(
        e.ok_at(&repo, &["show", &a])["decisions"][0]["status"],
        "open"
    );

    let d = e.ok_at(
        &repo,
        &[
            "--role", "human", "--author", "jeff", "decision", "answer", "D-1", "yes",
        ],
    );
    assert_eq!(d["decided_by"], "jeff");
    assert!(d["decided_at"].as_str().unwrap().ends_with('Z'));
    // Already decided.
    assert_eq!(e.human(&repo, &["decision", "answer", "D-1", "no"]), 5);
}

#[test]
fn accept_needs_a_recommendation() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.ticket(&repo, "a");
    e.ok_at(&repo, &["decision", "add", &a, "No rec?", "--option", "x"]);
    e.ok_at(
        &repo,
        &[
            "decision",
            "add",
            &a,
            "Rec?",
            "--option",
            "x",
            "--option",
            "y",
            "--recommend",
            "1",
        ],
    );

    assert_eq!(
        e.human(&repo, &["decision", "answer", "D-1", "--accept"]),
        2
    );
    assert_eq!(
        e.human(&repo, &["decision", "answer", "D-2", "t", "--accept"]),
        2
    );
    assert_eq!(e.human(&repo, &["decision", "answer", "D-2"]), 2);
    let d = e.ok_at(
        &repo,
        &["--role", "human", "decision", "answer", "D-2", "--accept"],
    );
    assert_eq!(d["answer"], "x");
    // Bad --recommend values.
    assert_eq!(
        e.code_at(&repo, &["decision", "add", &a, "Q", "--recommend", "1"]),
        2
    );
    assert_eq!(
        e.code_at(
            &repo,
            &[
                "decision",
                "add",
                &a,
                "Q",
                "--option",
                "x",
                "--recommend",
                "2"
            ]
        ),
        2
    );
    assert_eq!(
        e.code_at(
            &repo,
            &["decision", "add", &a, "Q", "--option", "x", "--option", "x"]
        ),
        2
    );
}

#[test]
fn cross_project_link_refused_and_missing_is_not_found() {
    let e = Env::new();
    let (r1, r2) = (e.repo("scree"), e.repo("other"));
    let a = e.ticket(&r1, "a");
    let b = e.ticket(&r2, "b");
    e.ok_at(&r1, &["decision", "add", &a, "Q?"]);

    assert_eq!(e.code_at(&r1, &["decision", "link", "D-1", &b]), 4);
    assert_eq!(e.code_at(&r1, &["decision", "link", "D-99", &a]), 3);
    assert_eq!(e.code_at(&r1, &["decision", "link", "D-1", "SCR-99"]), 3);
    assert_eq!(e.code_at(&r1, &["decision", "link", "nonsense", &a]), 2);
    // A qualified id reaches another Project from anywhere.
    let d = e.ok_at(&r2, &["decision", "link", "SCR-D-1", &a]);
    assert_eq!(d["tickets"], serde_json::json!([a]));
}

#[test]
fn supersede_rules() {
    let e = Env::new();
    let (r1, r2) = (e.repo("scree"), e.repo("other"));
    let a = e.ticket(&r1, "a");
    let b = e.ticket(&r2, "b");
    for q in ["one", "two", "three"] {
        e.ok_at(&r1, &["decision", "add", &a, q]);
    }
    e.ok_at(&r2, &["decision", "add", &b, "other"]);

    // Open old decision.
    assert_eq!(
        e.code_at(&r1, &["decision", "supersede", "D-1", "--by", "D-2"]),
        4
    );
    for d in ["D-1", "D-2"] {
        assert_eq!(e.human(&r1, &["decision", "answer", d, "ok"]), 0);
    }
    assert_eq!(
        e.code_at(&r1, &["decision", "supersede", "D-1", "--by", "D-1"]),
        4
    );
    assert_eq!(
        e.code_at(&r1, &["decision", "supersede", "D-1", "--by", "OTH-D-1"]),
        4
    );
    assert_eq!(
        e.code_at(&r1, &["decision", "supersede", "D-1", "--by", "D-2"]),
        0
    );
    // Already superseded, and a cycle.
    assert_eq!(
        e.code_at(&r1, &["decision", "supersede", "D-1", "--by", "D-3"]),
        4
    );
    assert_eq!(
        e.code_at(&r1, &["decision", "supersede", "D-2", "--by", "D-1"]),
        4
    );
    // A superseded decision cannot be answered.
    assert_eq!(e.human(&r1, &["decision", "answer", "D-1", "again"]), 4);
}

#[test]
fn numbers_are_per_project() {
    let e = Env::new();
    let (r1, r2) = (e.repo("scree"), e.repo("other"));
    let (a, b) = (e.ticket(&r1, "a"), e.ticket(&r2, "b"));
    assert_eq!(e.ok_at(&r1, &["decision", "add", &a, "1"])["id"], "D-1");
    assert_eq!(e.ok_at(&r1, &["decision", "add", &a, "2"])["id"], "D-2");
    assert_eq!(e.ok_at(&r2, &["decision", "add", &b, "1"])["id"], "D-1");
}

#[test]
fn open_decision_blocks_start_and_lists_it() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.ticket(&repo, "a");
    e.ok_at(&repo, &["decision", "add", &a, "Which store?"]);

    let shown = e.ok_at(&repo, &["show", &a]);
    assert_eq!(shown["blocked"], true);
    assert_eq!(shown["available"], false);
    assert_eq!(shown["blocked_by"], serde_json::json!([]));
    assert_eq!(
        shown["blocking_decisions"],
        serde_json::json!([{"id": "D-1", "question": "Which store?"}])
    );
    assert_eq!(e.code_at(&repo, &["start", &a]), 4);

    let blocked = e.ok_at(&repo, &["list", "--blocked"]);
    assert_eq!(blocked["tickets"][0]["id"], a.as_str());
    assert_eq!(blocked["tickets"][0]["blocking_decisions"][0]["id"], "D-1");

    // Answering the only open decision frees it with no other change.
    assert_eq!(e.human(&repo, &["decision", "answer", "D-1", "sqlite"]), 0);
    let shown = e.ok_at(&repo, &["show", &a]);
    assert_eq!(shown["blocked"], false);
    assert_eq!(shown["available"], true);
    assert_eq!(shown["blocking_decisions"], serde_json::json!([]));
    assert_eq!(e.code_at(&repo, &["start", &a]), 0);
}

#[test]
fn one_decision_blocks_several_tickets_and_two_decisions_need_both() {
    let e = Env::new();
    let repo = e.repo("scree");
    let (a, b, c) = (
        e.ticket(&repo, "a"),
        e.ticket(&repo, "b"),
        e.ticket(&repo, "c"),
    );
    e.ok_at(&repo, &["decision", "add", &a, "one"]);
    e.ok_at(&repo, &["decision", "link", "D-1", &b]);
    e.ok_at(&repo, &["decision", "add", &c, "two"]);
    e.ok_at(&repo, &["decision", "link", "D-2", &a]);
    let avail = |t: &str| e.ok_at(&repo, &["show", t])["available"].clone();

    assert_eq!(avail(&a), false);
    assert_eq!(avail(&b), false);
    assert_eq!(avail(&c), false);
    assert_eq!(
        e.ok_at(&repo, &["show", &a])["blocking_decisions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // D-1 unblocks b (its only decision) but a still waits on D-2.
    assert_eq!(e.human(&repo, &["decision", "answer", "D-1", "x"]), 0);
    assert_eq!(avail(&a), false);
    assert_eq!(avail(&b), true);
    assert_eq!(avail(&c), false);
    assert_eq!(e.human(&repo, &["decision", "answer", "D-2", "y"]), 0);
    assert_eq!(avail(&a), true);
    assert_eq!(avail(&c), true);
}

#[test]
fn next_counts_decision_blocked_tickets_as_pending() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.ticket(&repo, "a");
    e.ok_at(&repo, &["decision", "add", &a, "Q?"]);

    let n = e.ok_at(&repo, &["--role", "agent", "next"]);
    assert_eq!(n["tickets"], serde_json::json!([]));
    assert_eq!(n["blocked_pending"], 1);

    assert_eq!(e.human(&repo, &["decision", "answer", "D-1", "x"]), 0);
    let n = e.ok_at(&repo, &["--role", "agent", "next"]);
    assert_eq!(n["tickets"][0]["id"], a.as_str());
}

#[test]
fn superseded_decision_does_not_block() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.ticket(&repo, "a");
    e.ok_at(&repo, &["decision", "add", &a, "old"]);
    e.ok_at(&repo, &["decision", "add", &a, "new"]);
    for d in ["D-1", "D-2"] {
        assert_eq!(e.human(&repo, &["decision", "answer", d, "ok"]), 0);
    }
    assert_eq!(
        e.code_at(&repo, &["decision", "supersede", "D-1", "--by", "D-2"]),
        0
    );
    assert_eq!(e.ok_at(&repo, &["show", &a])["available"], true);
}
