//! The unfinished statuses as one declared set: what a bare `scree list`
//! returns, which subtasks hold a parent open, and which dependencies block.
//! Run through the real binary against a throwaway database outside any repo.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

struct Env {
    _dir: tempfile::TempDir,
    base: PathBuf,
    db: PathBuf,
}

fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let db = base.join("scree.db");
    Env {
        _dir: dir,
        base,
        db,
    }
}

impl Env {
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
        // A directory that is no repository: the tickets are unscoped.
        cmd.current_dir(&self.base)
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }

    /// Exit code of a command, with stdout and stderr dropped.
    fn code(&self, args: &[&str]) -> i32 {
        self.run(args).status.code().unwrap()
    }

    fn add(&self, title: &str, extra: &[&str]) -> String {
        let mut a = vec!["--json", "add", title];
        a.extend_from_slice(extra);
        self.ok(&a)["id"].as_str().unwrap().to_string()
    }

    /// One ticket per status, named by it. Returns the id of each.
    fn one_per_status(&self) -> Vec<(String, &'static str)> {
        let mut made = vec![];
        made.push((self.add("draft one", &[]), "draft"));
        made.push((self.add("ready one", &["--status", "ready"]), "ready"));
        let active = self.add("active one", &["--status", "ready"]);
        self.ok(&["--json", "start", &active]);
        made.push((active, "active"));
        let done = self.add("done one", &["--status", "ready"]);
        self.ok(&["--json", "done", &done]);
        made.push((done, "done"));
        let abandoned = self.add("abandoned one", &["--status", "ready"]);
        self.ok(&["--json", "abandon", &abandoned, "not happening"]);
        made.push((abandoned, "abandoned"));
        made
    }

    fn ids(&self, args: &[&str]) -> Vec<String> {
        let mut a = vec!["--json", "list"];
        a.extend_from_slice(args);
        self.ok(&a)["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    }

    fn status_of(&self, id: &str) -> String {
        self.ok(&["--json", "show", id])["status"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn blocker_ids(&self, id: &str) -> Vec<String> {
        self.ok(&["--json", "show", id])["blocked_by"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["id"].as_str().unwrap().to_string())
            .collect()
    }
}

#[test]
fn bare_list_returns_exactly_the_unfinished_tickets() {
    let e = env();
    let all = e.one_per_status();
    let names: Vec<&str> = all.iter().map(|(_, st)| *st).collect();
    assert_eq!(names, ["draft", "ready", "active", "done", "abandoned"]);
    let open: Vec<&str> = vec![all[0].0.as_str(), all[1].0.as_str(), all[2].0.as_str()];

    let mut got = e.ids(&[]);
    got.sort();
    let mut want = open.clone();
    want.sort();
    assert_eq!(got, want, "a bare list must show the unfinished ones only");

    // Naming the settled statuses still reaches them, so the default filter is
    // a filter and not a hard-coded exclusion.
    for (id, st) in &all {
        let mut named = e.ids(&["--status", *st]);
        named.sort();
        assert!(
            named.contains(id),
            "{st} ticket {id} missing from --status {st}"
        );
    }
}

#[test]
fn a_parent_waits_for_every_unfinished_subtask() {
    let e = env();
    let parent = e.add("parent", &["--status", "ready"]);
    // The active subtask comes first: only the first unsettled sibling can be
    // started, so creation order decides which one can reach `active`.
    let active = e.add("sub active", &["--parent", &parent, "--status", "ready"]);
    e.ok(&["--json", "start", &active]);
    let draft = e.add("sub draft", &["--parent", &parent]);
    let ready = e.add("sub ready", &["--parent", &parent, "--status", "ready"]);
    let done = e.add("sub done", &["--parent", &parent, "--status", "ready"]);
    e.ok(&["--json", "done", &done]);
    let abandoned = e.add("sub abandoned", &["--parent", &parent, "--status", "ready"]);
    e.ok(&["--json", "abandon", &abandoned, "dropped"]);

    let mut want = vec![active.clone(), draft.clone(), ready.clone()];
    want.sort();
    let mut blockers = e.blocker_ids(&parent);
    blockers.sort();
    assert_eq!(blockers, want, "only unfinished subtasks hold the parent");
    assert_eq!(e.code(&["--json", "done", &parent]), 4);

    for id in [&active, &draft, &ready] {
        if e.status_of(id) == "draft" {
            e.ok(&["--json", "ready", id]);
        }
        e.ok(&["--json", "done", id]);
    }
    assert!(e.blocker_ids(&parent).is_empty());
    assert_eq!(e.code(&["--json", "done", &parent]), 0);
    assert_eq!(e.status_of(&parent), "done");
}

#[test]
fn abandoning_a_parent_abandons_its_unfinished_subtasks() {
    let e = env();
    let parent = e.add("cascade parent", &["--status", "ready"]);
    let active = e.add(
        "cascade active",
        &["--parent", &parent, "--status", "ready"],
    );
    e.ok(&["--json", "start", &active]);
    let draft = e.add("cascade draft", &["--parent", &parent]);
    let done = e.add("cascade done", &["--parent", &parent, "--status", "ready"]);
    e.ok(&["--json", "done", &done]);

    e.ok(&["--json", "abandon", &parent, "superseded"]);
    assert_eq!(e.status_of(&active), "abandoned");
    assert_eq!(e.status_of(&draft), "abandoned");
    assert_eq!(e.status_of(&done), "done", "a settled subtask stays done");
}

#[test]
fn an_unfinished_dependency_blocks_and_a_done_one_does_not() {
    let e = env();
    let waiter = e.add("waiter", &["--status", "ready"]);
    let dep_draft = e.add("dep draft", &[]);
    let dep_active = e.add("dep active", &["--status", "ready"]);
    e.ok(&["--json", "start", &dep_active]);
    let dep_done = e.add("dep done", &["--status", "ready"]);
    e.ok(&["--json", "done", &dep_done]);
    let dep_abandoned = e.add("dep abandoned", &["--status", "ready"]);
    e.ok(&["--json", "abandon", &dep_abandoned, "dropped"]);

    for d in [&dep_draft, &dep_active, &dep_done, &dep_abandoned] {
        e.ok(&["--json", "dep", "add", &waiter, d]);
    }
    let detail = e.ok(&["--json", "show", &waiter]);
    assert_eq!(detail["available"], Value::Bool(false));
    let mut blockers = e.blocker_ids(&waiter);
    blockers.sort();
    let mut want = vec![dep_draft.clone(), dep_active.clone(), dep_abandoned.clone()];
    want.sort();
    assert_eq!(
        blockers, want,
        "an abandoned blocker still blocks; a done one does not"
    );

    e.ok(&["--json", "done", &dep_draft]);
    e.ok(&["--json", "done", &dep_active]);
    let detail = e.ok(&["--json", "show", &waiter]);
    assert_eq!(
        detail["available"],
        Value::Bool(false),
        "the abandoned blocker still holds the ticket"
    );

    e.ok(&["--json", "dep", "rm", &waiter, &dep_abandoned]);
    let detail = e.ok(&["--json", "show", &waiter]);
    assert_eq!(detail["available"], Value::Bool(true));
    assert!(e.blocker_ids(&waiter).is_empty());
}

#[test]
fn each_status_reads_back_its_own_name() {
    let e = env();
    for (id, st) in e.one_per_status() {
        assert_eq!(e.status_of(&id), st, "{id} should read as {st}");
    }
    // A dry sweep of a Project with no Worktrees touches nothing, so it cannot
    // be where an unfinished status hides.
    let report = e.ok(&["--json", "sweep", "--dry-run"]);
    assert_eq!(report["worktrees"], Value::Array(vec![]));
}
