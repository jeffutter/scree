//! Subtasks, hierarchy rules and `move`, through the real binary against
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

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

#[test]
fn subtask_ids_scope_and_counters() {
    let e = env();
    let repo = e.repo("scree");
    let wt = e.worktree(&repo, "feat");
    let a = e.add(&wt, "a", &[]);
    let b = e.add(&wt, "b", &[]);
    assert_eq!((a.as_str(), b.as_str()), ("SCR-1", "SCR-2"));

    // The parent's Scope wins over where the caller stands.
    let c = e.ok(&repo, &["add", "c", "--parent", "SCR-2"]);
    assert_eq!(s(&c["id"]), "SCR-2.1");
    assert_eq!(s(&c["parent"]), "SCR-2");
    assert_eq!(s(&c["worktree"]), wt.to_str().unwrap());
    let d = e.ok(&wt, &["add", "d", "--parent", "scr-2"]);
    assert_eq!(s(&d["id"]), "SCR-2.2");
    let first = e.ok(&wt, &["add", "e", "--parent", "SCR-1"]);
    assert_eq!(s(&first["id"]), "SCR-1.1", "numbers are per parent");

    // Subtasks do not consume the Project counter.
    assert_eq!(e.add(&wt, "f", &[]), "SCR-3");

    // Show works on subtask ids and lists subtasks on the parent.
    let shown = e.ok(&wt, &["show", "SCR-2.2"]);
    assert_eq!(s(&shown["title"]), "d");
    let parent = e.ok(&wt, &["show", "SCR-2"]);
    assert_eq!(parent["subtasks"].as_array().unwrap().len(), 2);
    assert_eq!(e.code(&wt, &["show", "SCR-2.9"]), 3);
    assert_eq!(e.code(&wt, &["show", "SCR-9.1"]), 3);
}

#[test]
fn third_level_and_bad_parents_are_refused() {
    let e = env();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let sub = e.sub(&repo, &a, "sub");
    assert_eq!(e.code(&repo, &["add", "x", "--parent", &sub]), 4);
    assert_eq!(e.code(&repo, &["show", "SCR-1.1.1"]), 2);
    assert_eq!(e.code(&repo, &["add", "x", "--parent", "SCR-99"]), 3);
    assert_eq!(
        e.code(&repo, &["add", "x", "--parent", &a, "--key", "ZZ"]),
        2
    );

    // A settled parent takes no new subtasks.
    let b = e.add(&repo, "b", &[]);
    e.ok(&repo, &["done", &b, "--force"]);
    assert_eq!(e.code(&repo, &["add", "x", "--parent", &b]), 4);

    // A parent in another Project than the one the caller names is refused.
    let other = e.repo("other");
    e.add(&other, "o", &[]);
    assert_eq!(e.code(&other, &["add", "x", "--parent", &a]), 4);
}

#[test]
fn parent_is_blocked_until_subtasks_settle() {
    let e = env();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let c1 = e.sub(&repo, &p, "c1");
    let c2 = e.sub(&repo, &p, "c2");

    let shown = e.ok(&repo, &["show", &p]);
    assert_eq!(shown["blocked"], true);
    assert_eq!(shown["available"], false);
    let by: Vec<&str> = shown["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| s(&b["id"]))
        .collect();
    assert_eq!(by, [c1.as_str(), c2.as_str()]);
    assert_eq!(e.code(&repo, &["--role", "agent", "start", &p]), 4);

    // done is refused, even forced, while any subtask is open.
    assert_eq!(e.code(&repo, &["done", &p, "--force"]), 4);
    e.ok(&repo, &["done", &c1, "--force"]);
    assert_eq!(e.code(&repo, &["done", &p, "--force"]), 4);
    e.ok(&repo, &["abandon", &c2, "no longer needed"]);
    let shown = e.ok(&repo, &["show", &p]);
    assert_eq!(shown["blocked"], false);
    assert_eq!(shown["available"], true);
    assert_eq!(s(&e.ok(&repo, &["done", &p, "--force"])["status"]), "done");
}

#[test]
fn abandoning_a_parent_abandons_open_subtasks_with_comments() {
    let e = env();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let done = e.sub(&repo, &p, "finished");
    let open = e.sub(&repo, &p, "open");
    let active = e.sub(&repo, &p, "active");
    e.ok(&repo, &["done", &done, "--force"]);
    e.ok(&repo, &["--role", "agent", "start", &active]);

    let t = e.ok(&repo, &["abandon", &p, "dropped"]);
    assert_eq!(s(&t["status"]), "abandoned");
    assert_eq!(s(&t["comments"][0]["body"]), "dropped");

    let o = e.ok(&repo, &["show", &open]);
    assert_eq!(s(&o["status"]), "abandoned");
    assert_eq!(s(&o["comments"][0]["body"]), "Parent SCR-1 abandoned");
    let a = e.ok(&repo, &["show", &active]);
    assert_eq!(s(&a["status"]), "abandoned");
    assert!(a["claimed_by"].is_null());
    assert_eq!(s(&e.ok(&repo, &["show", &done])["status"]), "done");

    // A subtask cannot be reopened under a settled parent.
    assert_eq!(e.code(&repo, &["reopen", &open]), 4);
    e.ok(&repo, &["reopen", &p]);
    assert_eq!(s(&e.ok(&repo, &["reopen", &open])["status"]), "ready");
}

#[test]
fn only_the_first_unsettled_sibling_is_available() {
    let e = env();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let c1 = e.sub(&repo, &p, "c1");
    let c2 = e.sub(&repo, &p, "c2");
    let c3 = e.sub(&repo, &p, "c3");

    assert_eq!(e.next(&repo), [c1.as_str()]);
    assert_eq!(e.ok(&repo, &["show", &c2])["available"], false);
    assert_eq!(e.ok(&repo, &["show", &c1])["available"], true);

    // Claimed, the first sibling still holds the line.
    e.ok(&repo, &["--role", "agent", "start", "--next"]);
    assert!(e.next(&repo).is_empty());

    e.ok(&repo, &["done", &c1, "--force"]);
    assert_eq!(e.next(&repo), [c2.as_str()]);
    e.ok(&repo, &["abandon", &c2, "skip"]);
    assert_eq!(e.next(&repo), [c3.as_str()]);

    // Reordering changes which sibling is first.
    let c4 = e.sub(&repo, &p, "c4");
    e.ok(&repo, &["reorder", &c4, "--first"]);
    assert_eq!(e.next(&repo), [c4.as_str()]);
    assert_eq!(
        e.ok(&repo, &["--role", "agent", "start", "--next"])["id"],
        c4.as_str()
    );

    // list --parent shows only that parent's subtasks.
    assert_eq!(
        e.ids(
            &repo,
            &["--parent", &p, "--status", "done", "--status", "abandoned"]
        ),
        [c1.as_str(), c2.as_str()]
    );
}

#[test]
fn list_parent_filter() {
    let e = env();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let q = e.add(&repo, "q", &[]);
    let c = e.sub(&repo, &p, "c");
    assert_eq!(e.ids(&repo, &["--parent", &p]), [c.as_str()]);
    assert!(e.ids(&repo, &["--parent", &q]).is_empty());
    // A parent held by its subtasks is blocked; the work is the subtask.
    assert_eq!(e.ok(&repo, &["show", &p])["blocked"], true);
    assert_eq!(e.next(&repo), [c.as_str(), q.as_str()]);
}

#[test]
fn move_takes_the_whole_tree_and_places_it_last() {
    let e = env();
    let repo = e.repo("scree");
    let wt = e.worktree(&repo, "feat");
    let p = e.add(&repo, "p", &[]);
    let c = e.sub(&repo, &p, "c");
    let other = e.add(&wt, "other", &[]);

    let t = e.ok(&repo, &["move", &p, "--worktree", wt.to_str().unwrap()]);
    assert_eq!(s(&t["worktree"]), wt.to_str().unwrap());
    assert_eq!(
        s(&e.ok(&repo, &["show", &c])["worktree"]),
        wt.to_str().unwrap()
    );
    // The tree sits after `other` among top-level tickets.
    let mut moved = e.ids(&wt, &[]);
    moved.sort();
    let mut want = vec![p.clone(), c.clone(), other.clone()];
    want.sort();
    assert_eq!(moved, want);
    assert!(e.ids(&repo, &[]).is_empty());
    let pos: i64 = e.sql(
        "SELECT position FROM tickets WHERE number = 1 AND parent_id IS NULL",
        |r| r.get(0),
    );
    assert_eq!(pos, 2000);

    // Back to the root checkout, by path and with --root.
    let t = e.ok(&repo, &["move", &p, "--root"]);
    assert!(t["worktree"].is_null());
    assert!(e.ok(&repo, &["show", &c])["worktree"].is_null());
    e.ok(&repo, &["move", &p, "--worktree", wt.to_str().unwrap()]);
    let t = e.ok(&wt, &["move", &p, "--worktree", repo.to_str().unwrap()]);
    assert!(t["worktree"].is_null());

    // Done trees may move; the move rewrites a label only.
    e.ok(&repo, &["done", &c, "--force"]);
    e.ok(&repo, &["done", &p, "--force"]);
    e.ok(&repo, &["move", &p, "--worktree", wt.to_str().unwrap()]);
    assert_eq!(s(&e.ok(&wt, &["show", &p])["status"]), "done");
}

#[test]
fn move_refusals() {
    let e = env();
    let repo = e.repo("scree");
    let wt = e.worktree(&repo, "feat");
    let other = e.repo("other");
    let other_wt = e.worktree(&other, "o");
    let p = e.add(&repo, "p", &[]);
    let c = e.sub(&repo, &p, "c");

    // A subtask moves with its parent only.
    assert_eq!(
        e.code(&repo, &["move", &c, "--worktree", wt.to_str().unwrap()]),
        4
    );
    // Another Project's checkout or worktree is refused, and nothing changes.
    e.add(&other, "o", &[]);
    for target in [&other, &other_wt] {
        assert_eq!(
            e.code(&repo, &["move", &p, "--worktree", target.to_str().unwrap()]),
            4
        );
    }
    assert!(e.ok(&repo, &["show", &p])["worktree"].is_null());
    // Not a repository.
    let plain = e.base.join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    assert_eq!(
        e.code(&repo, &["move", &p, "--worktree", plain.to_str().unwrap()]),
        4
    );
    // Exactly one target.
    assert_eq!(e.code(&repo, &["move", &p]), 2);
    assert_eq!(
        e.code(
            &repo,
            &["move", &p, "--root", "--worktree", wt.to_str().unwrap()]
        ),
        2
    );
    // LOCAL tickets have no worktrees.
    let local = e.base.join("plain");
    let l = e.add(&local, "l", &[]);
    assert_eq!(e.code(&local, &["move", &l, "--root"]), 2);
    assert_eq!(
        e.code(&local, &["move", &l, "--worktree", wt.to_str().unwrap()]),
        4
    );
}
