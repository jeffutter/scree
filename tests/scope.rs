//! Scope, Project and ticket tests against real throwaway repos and worktrees.

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
    /// A repo at `<base>/<parent>/<name>` with one commit.
    fn repo(&self, parent: &str, name: &str) -> PathBuf {
        let dir = self.base.join(parent).join(name);
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

    /// Run expecting success; the JSON on stdout.
    fn ok(&self, cwd: &Path, args: &[&str]) -> Value {
        let out = self.run(cwd, args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn code(&self, cwd: &Path, args: &[&str]) -> i32 {
        self.run(cwd, args).status.code().unwrap()
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

    fn sql(&self, q: &str) {
        rusqlite::Connection::open(&self.db)
            .unwrap()
            .execute_batch(q)
            .unwrap();
    }

    fn counter(&self, key: &str) -> i64 {
        rusqlite::Connection::open(&self.db)
            .unwrap()
            .query_row(
                "SELECT next_ticket_number FROM projects WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .unwrap()
    }
}

#[test]
fn add_in_root_checkout() {
    let e = env();
    let repo = e.repo("a", "my-app");
    let t = e.ok(&repo, &["add", "First"]);
    assert_eq!(t["id"], "MA-1");
    assert_eq!(t["status"], "draft");
    assert_eq!(t["for"], "agent");
    assert_eq!(t["worktree"], Value::Null);
    assert_eq!(e.ok(&repo, &["add", "Second"])["id"], "MA-2");
    // From a subdirectory too.
    let sub = repo.join("src");
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(e.ok(&sub, &["add", "Third"])["id"], "MA-3");
}

#[test]
fn add_options_and_show() {
    let e = env();
    let repo = e.repo("a", "scree");
    let t = e.ok(
        &repo,
        &[
            "add",
            "Title",
            "--description",
            "desc",
            "--plan",
            "p",
            "--criterion",
            "one",
            "--criterion",
            "two",
            "--for",
            "human",
            "--status",
            "ready",
        ],
    );
    assert_eq!(t["id"], "SCR-1");
    assert_eq!(t["for"], "human");
    assert_eq!(t["status"], "ready");
    let shown = e.ok(&repo, &["show", "scr-1"]);
    assert_eq!(shown["description"], "desc");
    assert_eq!(shown["plan"], "p");
    let crit = shown["criteria"].as_array().unwrap();
    assert_eq!(crit.len(), 2);
    assert_eq!(crit[1]["number"], 2);
    assert_eq!(crit[1]["text"], "two");
    assert_eq!(crit[1]["checked"], false);
    assert_eq!(shown["comments"], serde_json::json!([]));
    let text = String::from_utf8(e.run(&repo, &["--text", "show", "SCR-1"]).stdout).unwrap();
    assert!(text.contains("[ ] #1 one"), "{text}");
}

#[test]
fn stdin_text_argument() {
    use std::io::Write;
    let e = env();
    let repo = e.repo("a", "scree");
    let mut child = Command::new(env!("CARGO_BIN_EXE_scree"))
        .current_dir(&repo)
        .env_remove("SCREE_DB")
        .arg("--db")
        .arg(&e.db)
        .args(["add", "T", "--description", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"from stdin")
        .unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    assert_eq!(e.ok(&repo, &["show", "SCR-1"])["description"], "from stdin");
}

#[test]
fn stdin_for_both_description_and_plan_is_usage_error() {
    let e = env();
    let repo = e.repo("a", "scree");
    let args = ["add", "T", "--description", "-", "--plan", "-"];
    assert_eq!(e.code(&repo, &args), 2);
    assert!(e.ids(&repo, &[]).is_empty(), "nothing stored");
}

#[test]
fn linked_worktree_shares_project_and_counter() {
    let e = env();
    let repo = e.repo("a", "my-app");
    let wt = e.worktree(&repo, "feat");
    assert_eq!(e.ok(&repo, &["add", "root one"])["id"], "MA-1");
    let t = e.ok(&wt, &["add", "wt one"]);
    assert_eq!(t["id"], "MA-2");
    assert_eq!(t["worktree"], wt.to_str().unwrap());

    assert_eq!(e.ids(&repo, &[]), ["MA-1"]);
    assert_eq!(e.ids(&wt, &[]), ["MA-2"]);
    let sub = wt.join("deep");
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(e.ids(&sub, &[]), ["MA-2"]);
    assert_eq!(e.ids(&repo, &["--all-worktrees"]), ["MA-1", "MA-2"]);
    assert_eq!(e.ids(&wt, &["--all-worktrees"]), ["MA-1", "MA-2"]);
}

#[test]
fn outside_repo_is_local_or_needs_project() {
    let e = env();
    let repo = e.repo("a", "my-app");
    let plain = e.base.join("plain");
    std::fs::create_dir(&plain).unwrap();

    assert_eq!(e.ok(&plain, &["add", "loose"])["id"], "LOCAL-1");
    assert_eq!(e.ids(&plain, &[]), ["LOCAL-1"]);

    let t = e.ok(
        &plain,
        &["--project", repo.to_str().unwrap(), "add", "remote"],
    );
    assert_eq!(t["id"], "MA-1");
    assert_eq!(t["worktree"], Value::Null);
    // A non-repo --project is refused.
    assert_eq!(
        e.code(&plain, &["--project", plain.to_str().unwrap(), "add", "y"]),
        4
    );
    // --project naming a linked worktree scopes to it.
    let wt = e.worktree(&repo, "feat");
    let t = e.ok(&plain, &["--project", wt.to_str().unwrap(), "add", "in wt"]);
    assert_eq!(t["worktree"], wt.to_str().unwrap());
    // Inside a repo, --project is ignored.
    let t = e.ok(&repo, &["--project", wt.to_str().unwrap(), "add", "here"]);
    assert_eq!(t["worktree"], Value::Null);
}

#[test]
fn key_only_at_first_sight() {
    let e = env();
    let repo = e.repo("a", "my-app");
    assert_eq!(e.ok(&repo, &["add", "x", "--key", "zz"])["id"], "ZZ-1");
    assert_eq!(e.code(&repo, &["add", "y", "--key", "QQ"]), 4);
    assert_eq!(e.ok(&repo, &["add", "y"])["id"], "ZZ-2");

    let other = e.repo("b", "other");
    assert_eq!(e.code(&other, &["add", "x", "--key", "ZZ"]), 4, "taken");
    assert_eq!(e.code(&other, &["add", "x", "--key", "toolong"]), 2);
    assert_eq!(e.code(&other, &["add", "x", "--key", "A"]), 2);
    assert_eq!(e.code(&other, &["add", "x", "--key", "local"]), 4);
    assert_eq!(e.ok(&other, &["add", "x", "--key", "A1"])["id"], "A1-1");
    let plain = e.base.join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(e.code(&plain, &["add", "x", "--key", "ZZ"]), 2);
}

#[test]
fn key_collisions_get_a_suffix() {
    let e = env();
    let a = e.repo("a", "my-app");
    let b = e.repo("b", "my-app");
    let c = e.repo("c", "my-app");
    assert_eq!(e.ok(&a, &["add", "x"])["id"], "MA-1");
    assert_eq!(e.ok(&b, &["add", "x"])["id"], "MA2-1");
    assert_eq!(e.ok(&c, &["add", "x"])["id"], "MA3-1");
    // A Project is its root path: the same repo keeps its key.
    assert_eq!(e.ok(&b, &["add", "y"])["id"], "MA2-2");
}

#[test]
fn relink_keeps_key_and_counter() {
    let e = env();
    let repo = e.repo("a", "my-app");
    e.ok(&repo, &["add", "x"]);
    e.ok(&repo, &["add", "y"]);
    let moved = e.base.join("a").join("moved");
    std::fs::rename(&repo, &moved).unwrap();
    // Old root is unknown to git now; the stored Project is untouched.
    let out = e.ok(
        &e.base,
        &[
            "project",
            "relink",
            repo.to_str().unwrap(),
            moved.to_str().unwrap(),
        ],
    );
    assert_eq!(out["project"]["key"], "MA");
    assert_eq!(e.ok(&moved, &["add", "z"])["id"], "MA-3");
    assert_eq!(e.ids(&moved, &[]), ["MA-1", "MA-2", "MA-3"]);

    assert_eq!(
        e.code(
            &e.base,
            &[
                "project",
                "relink",
                "/nope/nothing",
                moved.to_str().unwrap()
            ]
        ),
        3
    );
    // A target that is already a Project is refused.
    let other = e.repo("b", "other");
    e.ok(&other, &["add", "x"]);
    assert_eq!(
        e.code(
            &e.base,
            &[
                "project",
                "relink",
                moved.to_str().unwrap(),
                other.to_str().unwrap()
            ]
        ),
        4
    );
    // A target that is not a repo root is refused.
    let plain = e.base.join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(
        e.code(
            &e.base,
            &[
                "project",
                "relink",
                moved.to_str().unwrap(),
                plain.to_str().unwrap()
            ]
        ),
        4
    );
}

#[test]
fn ids_are_case_insensitive_and_bare_numbers_refused() {
    let e = env();
    let repo = e.repo("a", "scree");
    e.ok(&repo, &["add", "x"]);
    let plain = e.base.join("plain");
    std::fs::create_dir(&plain).unwrap();
    // Ids work from any directory.
    assert_eq!(e.ok(&plain, &["show", "scr-1"])["id"], "SCR-1");
    assert_eq!(e.ok(&plain, &["show", "ScR-1"])["id"], "SCR-1");
    assert_eq!(e.code(&plain, &["show", "1"]), 2);
    assert_eq!(e.code(&plain, &["show", "SCR-x"]), 2);
    assert_eq!(e.code(&plain, &["show", "SCR-9"]), 3);
    assert_eq!(e.code(&plain, &["show", "ZZZ-1"]), 3);
}

#[test]
fn counter_is_monotonic_and_failed_adds_take_no_number() {
    let e = env();
    let repo = e.repo("a", "scree");
    e.ok(&repo, &["add", "x"]);
    assert_eq!(e.counter("SCR"), 2);
    // Refused after the Project exists: nothing allocated.
    assert_eq!(e.code(&repo, &["add", "y", "--key", "QQ"]), 4);
    assert_eq!(e.code(&repo, &["add", "  "]), 2);
    assert_eq!(e.counter("SCR"), 2);
    // Removing the ticket row does not rewind the counter.
    e.sql("DELETE FROM tickets WHERE number = 1");
    assert_eq!(e.ok(&repo, &["add", "z"])["id"], "SCR-2");
}

#[test]
fn concurrent_adds_get_distinct_ids() {
    let e = env();
    let repo = e.repo("a", "scree");
    e.ok(&repo, &["add", "seed"]);
    let handles: Vec<_> = (0..6)
        .map(|i| {
            let (db, repo) = (e.db.clone(), repo.clone());
            std::thread::spawn(move || {
                let out = Command::new(env!("CARGO_BIN_EXE_scree"))
                    .current_dir(repo)
                    .arg("--db")
                    .arg(db)
                    .args(["add", &format!("t{i}")])
                    .output()
                    .unwrap();
                assert!(out.status.success(), "{out:?}");
                let v: Value = serde_json::from_slice(&out.stdout).unwrap();
                v["id"].as_str().unwrap().to_string()
            })
        })
        .collect();
    let mut ids: Vec<String> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 6);
    assert_eq!(e.counter("SCR"), 8);
}

#[test]
fn list_filters() {
    let e = env();
    let repo = e.repo("a", "scree");
    e.ok(&repo, &["add", "a", "--status", "ready"]);
    e.ok(&repo, &["add", "b", "--for", "human"]);
    e.ok(&repo, &["add", "c"]);
    e.ok(&repo, &["add", "d"]);
    e.sql(
        "UPDATE tickets SET status='active', claimed_by_author='agent', claimed_by_session='s1' WHERE number=3;
         UPDATE tickets SET status='done' WHERE number=4;",
    );
    assert_eq!(
        e.ids(&repo, &[]),
        ["SCR-1", "SCR-2", "SCR-3"],
        "default hides done"
    );
    assert_eq!(e.ids(&repo, &["--status", "done"]), ["SCR-4"]);
    assert_eq!(
        e.ids(&repo, &["--status", "done", "--status", "ready"]),
        ["SCR-1", "SCR-4"]
    );
    assert_eq!(e.ids(&repo, &["--for", "human"]), ["SCR-2"]);
    assert_eq!(e.ids(&repo, &["--limit", "2"]), ["SCR-1", "SCR-2"]);
    assert_eq!(e.ids(&repo, &["--mine", "--session", "s1"]), ["SCR-3"]);
    assert!(e.ids(&repo, &["--mine", "--session", "other"]).is_empty());
    assert_eq!(
        e.ids(&repo, &["--mine", "--session", "s1", "--author", "x"])
            .len(),
        0
    );
}

#[test]
fn list_scope_flags() {
    let e = env();
    let a = e.repo("a", "alpha");
    let b = e.repo("b", "beta");
    let plain = e.base.join("plain");
    std::fs::create_dir(&plain).unwrap();
    e.ok(&a, &["add", "1"]);
    e.ok(&b, &["add", "2"]);
    e.ok(&plain, &["add", "3"]);

    assert_eq!(e.ids(&a, &[]), ["ALP-1"]);
    assert_eq!(e.ids(&plain, &[]), ["LOCAL-1"]);
    assert_eq!(
        e.ids(&a, &["--all-projects"]),
        ["ALP-1", "BET-1", "LOCAL-1"]
    );
    assert_eq!(
        e.ids(&plain, &["--project-root", b.to_str().unwrap()]),
        ["BET-1"]
    );
    assert_eq!(e.code(&plain, &["list", "--project-root", "/no/where"]), 3);

    e.sql("UPDATE tickets SET archived_at = '2026-01-01T00:00:00Z' WHERE number = 1 AND project_id = (SELECT id FROM projects WHERE key='ALP')");
    assert!(e.ids(&a, &[]).is_empty());
    assert_eq!(e.ids(&a, &["--archived"]), ["ALP-1"]);

    for pair in [
        ["--all-worktrees", "--all-projects"],
        ["--all-projects", "--project-root"],
    ] {
        let mut args = vec!["list", pair[0], pair[1]];
        if pair[1] == "--project-root" {
            args.push("x");
        }
        assert_eq!(e.code(&a, &args), 2, "{pair:?}");
    }
}

#[test]
fn never_seen_repo_lists_nothing_and_creates_nothing() {
    let e = env();
    let repo = e.repo("a", "fresh");
    assert!(e.ids(&repo, &[]).is_empty());
    let n: i64 = rusqlite::Connection::open(&e.db)
        .unwrap()
        .query_row("SELECT count(*) FROM projects", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "only LOCAL");
}

#[test]
fn deferred_flags_are_refused() {
    let e = env();
    let repo = e.repo("a", "scree");
    assert_eq!(e.code(&repo, &["add", "x", "--parent", "SCR-1"]), 4);
    assert_eq!(e.code(&repo, &["add", "x", "--blocked-by", "SCR-1"]), 4);
}
