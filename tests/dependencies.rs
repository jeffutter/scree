//! Dependencies and the computed Blocked and Available state, through the real binary against
//! throwaway repos and worktrees.

use std::path::{Path, PathBuf};
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
    let db = base.join("db").join("scree.db");
    Env {
        _dir: dir,
        base,
        db,
    }
}

fn git(dir: &Path, args: &[&str]) {
    let mut cmd = Command::new("git");
    for (k, _) in std::env::vars() {
        if k.starts_with("GIT_") {
            cmd.env_remove(k);
        }
    }
    let out = cmd
        .current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

impl Env {
    fn repo(&self, name: &str) -> PathBuf {
        let dir = self.base.join("repos").join(name);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
        dir
    }

    fn worktree(&self, repo: &Path, name: &str) -> PathBuf {
        let wt = self.base.join("wts").join(name);
        std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
        git(
            repo,
            &["worktree", "add", "-q", "-b", name, wt.to_str().unwrap()],
        );
        std::fs::canonicalize(wt).unwrap()
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
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
        cmd.current_dir(cwd)
            .arg("--db")
            .arg(&self.db)
            .args(args)
            .output()
            .unwrap()
    }

    fn ok(&self, cwd: &Path, args: &[&str]) -> Value {
        let out = self.run(cwd, args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn code(&self, cwd: &Path, args: &[&str]) -> i32 {
        self.run(cwd, args).status.code().unwrap()
    }

    /// Add as an agent-visible ticket in `ready` state; its id.
    fn add(&self, cwd: &Path, title: &str, extra: &[&str]) -> String {
        let mut a = vec!["--role", "human", "add", title, "--status", "ready"];
        a.extend_from_slice(extra);
        self.ok(cwd, &a)["id"].as_str().unwrap().to_string()
    }

    fn sub(&self, cwd: &Path, parent: &str, title: &str) -> String {
        self.add(cwd, title, &["--parent", parent])
    }

    fn ids(&self, cwd: &Path, args: &[&str]) -> Vec<String> {
        let mut a = vec!["list"];
        a.extend_from_slice(args);
        self.ok(cwd, &a)["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    }

    fn next(&self, cwd: &Path) -> Vec<String> {
        self.ok(cwd, &["--role", "agent", "next", "--limit", "10"])["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    }

    fn sql<T>(&self, q: &str, f: impl FnOnce(&rusqlite::Row) -> rusqlite::Result<T>) -> T {
        rusqlite::Connection::open(&self.db)
            .unwrap()
            .query_row(q, [], f)
            .unwrap()
    }
}

#[test]
fn dep_add_rm_and_refusals() {
    let e = env();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&repo, "b", &[]);
    let c = e.add(&repo, "c", &[]);

    let t = e.ok(&repo, &["dep", "add", &a, &b]);
    assert_eq!(t["blocked_by"][0]["id"], "SCR-2");
    // Idempotent.
    e.ok(&repo, &["dep", "add", &a, &b]);
    assert_eq!(
        e.ok(&repo, &["show", &a])["blocked_by"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Self edge and two-cycle.
    assert_eq!(e.code(&repo, &["dep", "add", &a, &a]), 4);
    assert_eq!(e.code(&repo, &["dep", "add", &b, &a]), 4);
    // Three-cycle: a <- b <- c, then c blocked by... a already waits on b.
    e.ok(&repo, &["dep", "add", &b, &c]);
    assert_eq!(e.code(&repo, &["dep", "add", &c, &a]), 4);
    // A refused edge leaves nothing behind.
    assert_eq!(
        e.ok(&repo, &["show", &c])["blocked_by"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // Unknown ticket.
    assert_eq!(e.code(&repo, &["dep", "add", &a, "SCR-99"]), 3);

    // Remove, and removing again is a no-op.
    e.ok(&repo, &["dep", "rm", &a, &b]);
    e.ok(&repo, &["dep", "rm", &a, &b]);
    assert!(
        e.ok(&repo, &["show", &a])["blocked_by"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // With the edge gone the former cycle is fine.
    e.ok(&repo, &["dep", "add", &b, &a]);
}

#[test]
fn cycles_count_parent_and_sibling_order() {
    let e = env();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let s1 = e.sub(&repo, &p, "s1");
    let s2 = e.sub(&repo, &p, "s2");
    // The parent waits on its subtasks, so a subtask cannot wait on it.
    assert_eq!(e.code(&repo, &["dep", "add", &s1, &p]), 4);
    // s2 waits on s1 by order, so s1 cannot wait on s2.
    assert_eq!(e.code(&repo, &["dep", "add", &s1, &s2]), 4);
    e.ok(&repo, &["dep", "add", &s2, &s1]);
}

#[test]
fn cross_project_refused_cross_worktree_allowed() {
    let e = env();
    let repo = e.repo("scree");
    let wa = e.worktree(&repo, "feat-a");
    let wb = e.worktree(&repo, "feat-b");
    let a = e.add(&wa, "a", &[]);
    let b = e.add(&wb, "b", &[]);
    e.ok(&wa, &["dep", "add", &a, &b]);

    let other = e.repo("other");
    let o = e.add(&other, "o", &[]);
    assert_eq!(e.code(&wa, &["dep", "add", &a, &o]), 4);
    assert_eq!(e.code(&wa, &["dep", "add", &o, &a]), 4);
}

#[test]
fn blocked_until_done_and_abandoned_still_blocks() {
    let e = env();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&repo, "b", &[]);
    e.ok(&repo, &["dep", "add", &a, &b]);

    assert_eq!(e.ids(&repo, &["--blocked"]), vec![a.clone()]);
    assert_eq!(e.ids(&repo, &["--available"]), vec![b.clone()]);
    assert_eq!(e.next(&repo), vec![b.clone()]);
    assert_eq!(e.code(&repo, &["--role", "agent", "start", &a]), 4);

    e.ok(&repo, &["abandon", &b, "nope"]);
    let shown = e.ok(&repo, &["show", &a]);
    assert_eq!(shown["blocked_by"][0]["status"], "abandoned");
    assert_eq!(e.ids(&repo, &["--blocked"]), vec![a.clone()]);
    assert!(e.ids(&repo, &["--available"]).is_empty());

    e.ok(&repo, &["reopen", &b]);
    e.ok(&repo, &["done", &b, "--force"]);
    assert!(e.ids(&repo, &["--blocked"]).is_empty());
    assert_eq!(e.ids(&repo, &["--available"]), vec![a.clone()]);
    assert!(
        e.ok(&repo, &["show", &a])["blocked_by"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Conflicting flags are a usage error.
    assert_eq!(e.code(&repo, &["list", "--blocked", "--available"]), 2);
}

#[test]
fn blocked_by_shape_text_and_next_pending() {
    let e = env();
    let repo = e.repo("scree");
    let wa = e.worktree(&repo, "feat-a");
    let wb = e.worktree(&repo, "feat-b");
    let a = e.add(&wa, "a", &[]);
    let b = e.add(&wb, "bee", &[]);
    e.ok(&wa, &["dep", "add", &a, &b]);

    let shown = e.ok(&wa, &["show", &a]);
    let bb = &shown["blocked_by"][0];
    assert_eq!(bb["id"], "SCR-2");
    assert_eq!(bb["title"], "bee");
    assert_eq!(bb["status"], "ready");
    assert_eq!(bb["worktree"], wb.to_str().unwrap());

    let out = e.run(&wa, &["--text", "list"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("[blocked by SCR-2 in feat-b]"), "{text}");

    let n = e.ok(&wa, &["--role", "agent", "next"]);
    assert!(n["tickets"].as_array().unwrap().is_empty());
    assert_eq!(n["blocked_pending"], 1);
}

#[test]
fn add_blocked_by_sets_edges_and_refuses_cross_project() {
    let e = env();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&repo, "b", &[]);
    let c = e.add(&repo, "c", &["--blocked-by", &a, "--blocked-by", &b]);
    let shown = e.ok(&repo, &["show", &c]);
    assert_eq!(shown["blocked_by"].as_array().unwrap().len(), 2);
    assert_eq!(e.ids(&repo, &["--blocked"]), [c]);

    let other = e.repo("other");
    let o = e.add(&other, "o", &[]);
    let before: i64 = e.sql("SELECT count(*) FROM tickets", |r| r.get(0));
    assert_eq!(e.code(&repo, &["add", "x", "--blocked-by", &o]), 4);
    assert_eq!(e.code(&repo, &["add", "x", "--blocked-by", "SCR-99"]), 3);
    let after: i64 = e.sql("SELECT count(*) FROM tickets", |r| r.get(0));
    assert_eq!(before, after, "a refused add creates no ticket");
}
