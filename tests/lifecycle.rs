//! Status transitions, claim and edit, run through the real binary.

mod common;

use std::path::Path;

use common::{Env, scree_cmd};

impl Env {
    /// A ready ticket in the LOCAL Project; its id.
    fn ready(&self, extra: &[&str]) -> String {
        let mut a = vec!["--role", "human", "add", "t", "--status", "ready"];
        a.extend_from_slice(extra);
        self.ok(&a)["id"].as_str().unwrap().to_string()
    }
}

#[test]
fn happy_path() {
    let e = Env::new();
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
fn draft_goes_back_from_ready_only() {
    let e = Env::new();
    let id = e.ready(&[]);
    let t = e.ok(&["draft", &id]);
    assert_eq!(t["status"], "draft");
    assert_eq!(t["available"], false);
    assert_eq!(e.code(&["start", &id]), 4);
    assert_eq!(e.code(&["draft", &id]), 4);
    assert_eq!(e.ok(&["ready", &id])["status"], "ready");

    e.ok(&["start", &id]);
    assert_eq!(e.code(&["draft", &id]), 4);
    e.ok(&["done", &id]);
    assert_eq!(e.code(&["draft", &id]), 4);
    e.ok(&["reopen", &id]);
    e.ok(&["abandon", &id, "no"]);
    assert_eq!(e.code(&["draft", &id]), 4);
}

#[test]
fn abandon_stores_reason_comment() {
    let e = Env::new();
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
    let e = Env::new();
    let id = e.ready(&[]);
    let t = e.ok(&["start", &id, "--author", "bob"]);
    assert_eq!(t["status"], "active");
    assert_eq!(t["claimed_by"]["author"], "bob");
    assert!(t["claimed_by"]["session"].is_null());
    assert_eq!(e.ok(&["show", &id])["claimed_by"]["author"], "bob");
}

#[test]
fn start_exit_codes() {
    let e = Env::new();
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
    e.exec_batch(&format!(
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
    let e = Env::new();
    let id = e.ready(&["--criterion", "works"]);
    assert_eq!(e.code(&["done", &id]), 4);
    assert_eq!(e.ok(&["done", &id, "--force"])["status"], "done");
}

#[test]
fn edit_fields_and_append() {
    let e = Env::new();
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
    let e = Env::new();
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
    let e = Env::new();
    let id = e.ready(&[]);
    let db = e.db();
    let cwd: &Path = e.path();
    let codes: Vec<i32> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..8)
            .map(|i| {
                let id = id.clone();
                let db = db.clone();
                s.spawn(move || {
                    scree_cmd(&db, cwd)
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
    let e = Env::new();
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

#[test]
fn hyphen_leading_text_is_data() {
    let e = Env::new();
    let h = ["--role", "human"];
    let run = |rest: &[&str]| -> serde_json::Value {
        let a: Vec<&str> = h.iter().copied().chain(rest.iter().copied()).collect();
        e.ok(&a)
    };

    // add: title, description, plan and criterion, all hyphen-leading, in the space form.
    let t = run(&[
        "add",
        "- dash title",
        "--description",
        "- d",
        "--plan",
        "- p",
        "--criterion",
        "- c one",
        "--status",
        "ready",
    ]);
    let id = t["id"].as_str().unwrap().to_string();
    assert_eq!(t["title"], "- dash title");
    assert_eq!(t["description"], "- d");
    assert_eq!(t["plan"], "- p");
    assert_eq!(t["criteria"][0]["text"], "- c one");

    // edit: title, description, plan.
    let t = run(&[
        "edit",
        &id,
        "--title",
        "- T2",
        "--description",
        "- d2",
        "--plan",
        "- p2",
    ]);
    assert_eq!(t["title"], "- T2");
    assert_eq!(t["description"], "- d2");
    assert_eq!(t["plan"], "- p2");

    // criteria add, comment body, check evidence.
    let t = run(&["criteria", "add", &id, "- c two"]);
    assert_eq!(t["criteria"][1]["text"], "- c two");
    let t = run(&["comment", "add", &id, "- note"]);
    assert_eq!(
        t["comments"].as_array().unwrap().last().unwrap()["body"],
        "- note"
    );
    let t = run(&["check", &format!("{id}#1"), "--evidence", "- ev"]);
    assert_eq!(t["criteria"][0]["evidence"], "- ev");

    // A description built by command substitution that starts with a bullet list.
    let multi = "- first\n- second";
    let t = run(&["add", "T", "--description", multi]);
    assert_eq!(t["description"], multi);

    // abandon reason.
    let t = run(&["abandon", &id, "- gave up"]);
    let last = t["comments"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["body"], "- gave up");
}

#[test]
fn bare_dash_title_and_criterion_read_stdin() {
    let e = Env::new();
    let h = ["--role", "human"];
    let t = e.stdin_ok(&[h[0], h[1], "add", "-"], "piped title\n");
    assert_eq!(t["title"], "piped title");
    let id = t["id"].as_str().unwrap().to_string();

    let t = e.stdin_ok(&[h[0], h[1], "criteria", "add", &id, "-"], "hello\n");
    assert_eq!(t["criteria"][0]["text"], "hello");

    // An empty read is a usage error naming the argument, and adds nothing.
    let (code, err) = e.stdin_result(&["--role", "human", "criteria", "add", &id, "-"], "");
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("criterion text"), "{err}");
    let (code, err) = e.stdin_result(&["--role", "human", "add", "-"], "");
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("title"), "{err}");
    assert_eq!(
        e.ok(&["show", &id])["criteria"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn option_value_never_swallows_the_next_flag() {
    let e = Env::new();
    let id = e.ready(&["--plan", "keep"]);
    let before = e.ok(&["show", &id]);
    let out = e.run(&[
        "--role",
        "human",
        "edit",
        &id,
        "--description",
        "--plan",
        "p",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let after = e.ok(&["show", &id]);
    assert_eq!(after["description"], before["description"]);
    assert_eq!(after["plan"], before["plan"]);
    assert_eq!(after["updated_at"], before["updated_at"]);
}
