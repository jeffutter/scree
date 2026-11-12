//! Sweep and archive tests against real throwaway repos and worktrees.

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

    fn run_with(&self, cwd: &Path, extra: &[(&str, &str)], args: &[&str]) -> Output {
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
        for (k, v) in extra {
            cmd.env(k, v);
        }
        cmd.current_dir(cwd)
            .arg("--db")
            .arg(&self.db)
            .args(["--session", "s1"])
            .args(args)
            .output()
            .unwrap()
    }

    fn run(&self, cwd: &Path, args: &[&str]) -> Output {
        self.run_with(cwd, &[], args)
    }

    fn ok(&self, cwd: &Path, args: &[&str]) -> Value {
        let out = self.run(cwd, args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        serde_json::from_slice(&out.stdout).unwrap()
    }

    fn stderr(out: &Output) -> String {
        String::from_utf8_lossy(&out.stderr).to_string()
    }

    fn q<T: rusqlite::types::FromSql>(&self, sql: &str) -> T {
        rusqlite::Connection::open(&self.db)
            .unwrap()
            .query_row(sql, [], |r| r.get(0))
            .unwrap()
    }

    fn status(&self, id: &str) -> String {
        self.ok(&self.base, &["show", id])["status"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn archived(&self, id: &str) -> bool {
        !self.ok(&self.base, &["show", id])["archived_at"].is_null()
    }

    /// A worktree `name` of `repo` holding: a done ticket, a ready ticket,
    /// an active ticket, and a ready subtask. Ids are MA-<n> in a repo
    /// named `my-app` that already holds one root-checkout ticket MA-1.
    fn populated(&self, repo: &Path, name: &str) -> PathBuf {
        let wt = self.worktree(repo, name);
        self.ok(repo, &["add", "root ticket", "--status", "ready"]);
        for (i, title) in ["done one", "ready one", "active one"].iter().enumerate() {
            self.ok(&wt, &["add", title, "--status", "ready"]);
            let id = format!("MA-{}", i + 2);
            match i {
                0 => {
                    self.ok(&wt, &["start", &id]);
                    self.ok(&wt, &["done", &id, "--force"]);
                }
                2 => {
                    self.ok(&wt, &["start", &id]);
                }
                _ => {}
            }
        }
        self.ok(
            &wt,
            &["add", "sub", "--parent", "MA-3", "--status", "ready"],
        );
        wt
    }
}

fn my_app(e: &Env) -> PathBuf {
    e.repo("my-app")
}

#[test]
fn removed_worktree_is_swept_on_list() {
    let e = env();
    let repo = my_app(&e);
    let wt = e.populated(&repo, "feat");
    git(
        &repo,
        &["worktree", "remove", "--force", wt.to_str().unwrap()],
    );
    let out = e.run(&repo, &["list"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        Env::stderr(&out).contains("swept 1 closed worktree: 1 archived, 3 abandoned"),
        "{}",
        Env::stderr(&out)
    );
    assert!(e.archived("MA-2"));
    assert_eq!(e.status("MA-2"), "done");
    for id in ["MA-3", "MA-4", "MA-3.1"] {
        assert!(e.archived(id), "{id}");
        assert_eq!(e.status(id), "abandoned", "{id}");
    }
    let n: i64 = e.q("SELECT COUNT(*) FROM comments WHERE author = 'scree'
         AND body LIKE 'Worktree % no longer exists'");
    assert_eq!(n, 3);
    // The root-checkout ticket is untouched.
    assert!(!e.archived("MA-1"));
    assert_eq!(e.status("MA-1"), "ready");
    // A second run has nothing to do.
    let out = e.run(&repo, &["list"]);
    assert!(!Env::stderr(&out).contains("swept"));
}

#[test]
fn prunable_is_closed_locked_and_merged_are_open() {
    let e = env();
    let repo = my_app(&e);
    let pruned = e.worktree(&repo, "gone");
    let locked = e.worktree(&repo, "locked");
    let merged = e.worktree(&repo, "merged");
    for wt in [&pruned, &locked, &merged] {
        e.ok(wt, &["add", "t", "--status", "ready"]);
    }
    std::fs::remove_dir_all(&pruned).unwrap();
    git(&repo, &["worktree", "lock", locked.to_str().unwrap()]);
    // `merged` shares HEAD with main, so its branch is merged, dir exists.
    e.ok(&repo, &["list"]);
    assert_eq!(e.status("MA-1"), "abandoned");
    assert!(e.archived("MA-1"));
    assert_eq!(e.status("MA-2"), "ready");
    assert_eq!(e.status("MA-3"), "ready");
    assert!(!e.archived("MA-2") && !e.archived("MA-3"));
}

#[test]
fn local_tickets_are_never_swept() {
    let e = env();
    let repo = my_app(&e);
    let outside = e.base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let t = e.ok(&outside, &["add", "local one", "--status", "ready"]);
    let id = t["id"].as_str().unwrap().to_string();
    e.ok(&repo, &["list"]);
    e.ok(&outside, &["list"]);
    let out = e.run(&outside, &["sweep"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!e.archived(&id));
}

#[test]
fn root_missing_skips_and_sweep_fails() {
    let e = env();
    let repo = my_app(&e);
    let wt = e.populated(&repo, "feat");
    git(
        &repo,
        &["worktree", "remove", "--force", wt.to_str().unwrap()],
    );
    let moved = e.base.join("moved");
    std::fs::rename(&repo, &moved).unwrap();
    let out = e.run(&e.base, &["--project", repo.to_str().unwrap(), "sweep"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["skipped"].as_array().unwrap().len(), 1);
    assert!(
        v["skipped"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("missing")
    );
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM tickets WHERE archived_at IS NOT NULL"),
        0
    );
    // All Projects, no --project: still skipped, still non-zero.
    assert_eq!(e.run(&e.base, &["sweep"]).status.code(), Some(1));
}

#[test]
fn git_failing_skips_with_warning() {
    let e = env();
    let repo = my_app(&e);
    let wt = e.populated(&repo, "feat");
    git(
        &repo,
        &["worktree", "remove", "--force", wt.to_str().unwrap()],
    );
    // An empty PATH makes git unrunnable for scree's sweep. Scope detection
    // also needs git, so name the Project with --project from elsewhere via
    // the stored root.
    let out = e.run_with(&e.base, &[("PATH", "")], &["sweep"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM tickets WHERE archived_at IS NOT NULL"),
        0
    );
    // A root that is no longer a git repo: also skipped.
    std::fs::remove_dir_all(repo.join(".git")).unwrap();
    let out = e.run(&e.base, &["sweep"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM tickets WHERE archived_at IS NOT NULL"),
        0
    );
}

fn setup_closed(e: &Env) -> (PathBuf, PathBuf) {
    let repo = my_app(e);
    let wt = e.populated(&repo, "feat");
    (repo, wt)
}

fn close(repo: &Path, wt: &Path) {
    git(
        repo,
        &["worktree", "remove", "--force", wt.to_str().unwrap()],
    );
}

#[test]
fn triggers() {
    for cmd in ["next", "list", "add", "start", "done"] {
        let e = env();
        let (repo, wt) = setup_closed(&e);
        e.ok(&repo, &["add", "trigger", "--status", "ready"]);
        if cmd == "done" {
            e.ok(&repo, &["start", "MA-1"]);
        }
        close(&repo, &wt);
        let args: Vec<&str> = match cmd {
            "next" => vec!["next"],
            "list" => vec!["list"],
            "add" => vec!["add", "another"],
            "start" => vec!["start", "--next"],
            _ => vec!["done", "MA-1", "--force"],
        };
        let out = e.run(&repo, &args);
        assert_eq!(out.status.code(), Some(0), "{cmd}: {out:?}");
        assert!(
            Env::stderr(&out).contains("swept 1"),
            "{cmd}: {}",
            Env::stderr(&out)
        );
        assert!(e.archived("MA-2"), "{cmd}");
    }
}

#[test]
fn show_all_projects_and_env_do_not_sweep() {
    let e = env();
    let (repo, wt) = setup_closed(&e);
    close(&repo, &wt);
    for args in [vec!["show", "MA-1"], vec!["list", "--all-projects"]] {
        let out = e.run(&repo, &args);
        assert_eq!(out.status.code(), Some(0));
        assert!(!e.archived("MA-2"), "{args:?}");
    }
    let out = e.run_with(&repo, &[("SCREE_NO_SWEEP", "1")], &["list"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!e.archived("MA-2"));
    let out = e.run(&repo, &["list"]);
    assert!(Env::stderr(&out).contains("swept"));
    assert!(e.archived("MA-2"));
}

#[test]
fn dry_run_reports_and_writes_nothing() {
    let e = env();
    let (repo, wt) = setup_closed(&e);
    close(&repo, &wt);
    let v = e.ok(&repo, &["sweep", "--dry-run"]);
    assert_eq!(v["dry_run"], true);
    let w = &v["worktrees"][0];
    assert_eq!(w["project"], "MA");
    assert_eq!(w["path"], wt.to_str().unwrap());
    assert_eq!(w["archived"], 1);
    assert_eq!(w["abandoned"], 3);
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM tickets WHERE archived_at IS NOT NULL"),
        0
    );
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM comments WHERE author = 'scree'"),
        0
    );
    // The real run, from outside any repo, via --project.
    let v = e.ok(&e.base, &["--project", repo.to_str().unwrap(), "sweep"]);
    assert_eq!(v["worktrees"][0]["abandoned"], 3);
    assert_eq!(
        e.q::<i64>("SELECT COUNT(*) FROM tickets WHERE archived_at IS NOT NULL"),
        4
    );
    let text = String::from_utf8(e.run(&repo, &["--text", "sweep"]).stdout).unwrap();
    assert!(text.contains("nothing to sweep"), "{text}");
}

#[test]
fn archive_and_unarchive() {
    let e = env();
    let repo = my_app(&e);
    e.ok(&repo, &["add", "parent", "--status", "ready"]);
    e.ok(
        &repo,
        &["add", "kid", "--parent", "MA-1", "--status", "ready"],
    );
    e.ok(&repo, &["start", "MA-1.1"]);
    e.ok(&repo, &["done", "MA-1.1", "--force"]);
    e.ok(&repo, &["start", "MA-1"]);
    e.ok(&repo, &["done", "MA-1", "--force"]);
    let t = e.ok(&repo, &["archive", "MA-1"]);
    assert!(!t["archived_at"].is_null());
    assert!(e.archived("MA-1.1"));
    let listed = |extra: &[&str]| {
        let mut a = vec!["list", "--status", "done"];
        a.extend_from_slice(extra);
        e.ok(&repo, &a)["tickets"].as_array().unwrap().len()
    };
    assert_eq!(listed(&[]), 0);
    assert_eq!(listed(&["--archived"]), 2);
    // Archiving twice is fine.
    e.ok(&repo, &["archive", "MA-1"]);
    // A subtask cannot leave an archived parent alone.
    assert_eq!(
        e.run(&repo, &["unarchive", "MA-1.1"]).status.code(),
        Some(4)
    );
    let t = e.ok(&repo, &["unarchive", "MA-1"]);
    assert!(t["archived_at"].is_null());
    assert!(!e.archived("MA-1.1"));
    assert_eq!(listed(&[]), 2);
    e.ok(&repo, &["unarchive", "MA-1"]);
    assert_eq!(e.run(&repo, &["archive", "MA-9"]).status.code(), Some(3));
}
