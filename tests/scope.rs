//! Scope, Project and ticket tests against real throwaway repos and worktrees.

mod common;

use std::path::{Path, PathBuf};

use common::{Env, git_cmd, scree_cmd};
use serde_json::Value;

// Repo and worktree builders on top of the shared harness. The environment's
// directory is not a repository, so every command here names the cwd it runs in.
impl Env {
    /// A repo at `<dir>/<parent>/<name>` with one commit.
    fn repo(&self, parent: &str, name: &str) -> PathBuf {
        let dir = self.path().join(parent).join(name);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
        dir
    }

    fn worktree(&self, repo: &Path, name: &str) -> PathBuf {
        let wt = self.path().join("wts").join(name);
        std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
        git(
            repo,
            &["worktree", "add", "-q", "-b", name, wt.to_str().unwrap()],
        );
        std::fs::canonicalize(wt).unwrap()
    }

    /// Run expecting success; the JSON on stdout.
    fn counter(&self, key: &str) -> i64 {
        rusqlite::Connection::open(self.db())
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
    let e = Env::new();
    let repo = e.repo("a", "my-app");
    let t = e.ok_at(&repo, &["add", "First"]);
    assert_eq!(t["id"], "MA-1");
    assert_eq!(t["status"], "draft");
    assert_eq!(t["for"], "agent");
    assert_eq!(t["worktree"], Value::Null);
    assert_eq!(e.ok_at(&repo, &["add", "Second"])["id"], "MA-2");
    // From a subdirectory too.
    let sub = repo.join("src");
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(e.ok_at(&sub, &["add", "Third"])["id"], "MA-3");
}

#[test]
fn add_options_and_show() {
    let e = Env::new();
    let repo = e.repo("a", "scree");
    let t = e.ok_at(
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
    let shown = e.ok_at(&repo, &["show", "scr-1"]);
    assert_eq!(shown["description"], "desc");
    assert_eq!(shown["plan"], "p");
    let crit = shown["criteria"].as_array().unwrap();
    assert_eq!(crit.len(), 2);
    assert_eq!(crit[1]["number"], 2);
    assert_eq!(crit[1]["text"], "two");
    assert_eq!(crit[1]["checked"], false);
    assert_eq!(shown["comments"], serde_json::json!([]));
    let text = String::from_utf8(e.run_at(&repo, &["--text", "show", "SCR-1"]).stdout).unwrap();
    assert!(text.contains("[ ] #1 one"), "{text}");
}

#[test]
fn stdin_text_argument() {
    let e = Env::new();
    let repo = e.repo("a", "scree");
    let child = e.spawn_stdin(&repo, &["add", "T", "--description", "-"], "from stdin");
    assert!(child.wait_with_output().unwrap().status.success());
    assert_eq!(
        e.ok_at(&repo, &["show", "SCR-1"])["description"],
        "from stdin"
    );
}

#[test]
fn stdin_for_both_description_and_plan_is_usage_error() {
    let e = Env::new();
    let repo = e.repo("a", "scree");
    let args = ["add", "T", "--description", "-", "--plan", "-"];
    assert_eq!(e.code_at(&repo, &args), 2);
    assert!(e.ids_at(&repo, &[]).is_empty(), "nothing stored");
}

#[test]
fn linked_worktree_shares_project_and_counter() {
    let e = Env::new();
    let repo = e.repo("a", "my-app");
    let wt = e.worktree(&repo, "feat");
    assert_eq!(e.ok_at(&repo, &["add", "root one"])["id"], "MA-1");
    let t = e.ok_at(&wt, &["add", "wt one"]);
    assert_eq!(t["id"], "MA-2");
    assert_eq!(t["worktree"], wt.to_str().unwrap());

    assert_eq!(e.ids_at(&repo, &[]), ["MA-1"]);
    assert_eq!(e.ids_at(&wt, &[]), ["MA-2"]);
    let sub = wt.join("deep");
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(e.ids_at(&sub, &[]), ["MA-2"]);
    assert_eq!(e.ids_at(&repo, &["--all-worktrees"]), ["MA-1", "MA-2"]);
    assert_eq!(e.ids_at(&wt, &["--all-worktrees"]), ["MA-1", "MA-2"]);
}

#[test]
fn outside_repo_is_local_or_needs_project() {
    let e = Env::new();
    let repo = e.repo("a", "my-app");
    let plain = e.path().join("plain");
    std::fs::create_dir(&plain).unwrap();

    assert_eq!(e.ok_at(&plain, &["add", "loose"])["id"], "LOCAL-1");
    assert_eq!(e.ids_at(&plain, &[]), ["LOCAL-1"]);

    let t = e.ok_at(
        &plain,
        &["--project", repo.to_str().unwrap(), "add", "remote"],
    );
    assert_eq!(t["id"], "MA-1");
    assert_eq!(t["worktree"], Value::Null);
    // A non-repo --project is refused.
    assert_eq!(
        e.code_at(&plain, &["--project", plain.to_str().unwrap(), "add", "y"]),
        4
    );
    // --project naming a linked worktree scopes to it.
    let wt = e.worktree(&repo, "feat");
    let t = e.ok_at(&plain, &["--project", wt.to_str().unwrap(), "add", "in wt"]);
    assert_eq!(t["worktree"], wt.to_str().unwrap());
    // Inside a repo, --project is ignored.
    let t = e.ok_at(&repo, &["--project", wt.to_str().unwrap(), "add", "here"]);
    assert_eq!(t["worktree"], Value::Null);
}

#[test]
fn key_only_at_first_sight() {
    let e = Env::new();
    let repo = e.repo("a", "my-app");
    assert_eq!(e.ok_at(&repo, &["add", "x", "--key", "zz"])["id"], "ZZ-1");
    assert_eq!(e.code_at(&repo, &["add", "y", "--key", "QQ"]), 4);
    assert_eq!(e.ok_at(&repo, &["add", "y"])["id"], "ZZ-2");

    let other = e.repo("b", "other");
    assert_eq!(e.code_at(&other, &["add", "x", "--key", "ZZ"]), 4, "taken");
    assert_eq!(e.code_at(&other, &["add", "x", "--key", "toolong"]), 2);
    assert_eq!(e.code_at(&other, &["add", "x", "--key", "A"]), 2);
    assert_eq!(e.code_at(&other, &["add", "x", "--key", "local"]), 4);
    assert_eq!(e.ok_at(&other, &["add", "x", "--key", "A1"])["id"], "A1-1");
    let plain = e.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(e.code_at(&plain, &["add", "x", "--key", "ZZ"]), 2);
}

#[test]
fn key_collisions_get_a_suffix() {
    let e = Env::new();
    let a = e.repo("a", "my-app");
    let b = e.repo("b", "my-app");
    let c = e.repo("c", "my-app");
    assert_eq!(e.ok_at(&a, &["add", "x"])["id"], "MA-1");
    assert_eq!(e.ok_at(&b, &["add", "x"])["id"], "MA2-1");
    assert_eq!(e.ok_at(&c, &["add", "x"])["id"], "MA3-1");
    // A Project is its root path: the same repo keeps its key.
    assert_eq!(e.ok_at(&b, &["add", "y"])["id"], "MA2-2");
}

#[test]
fn relink_keeps_key_and_counter() {
    let e = Env::new();
    let repo = e.repo("a", "my-app");
    e.ok_at(&repo, &["add", "x"]);
    e.ok_at(&repo, &["add", "y"]);
    let moved = e.path().join("a").join("moved");
    std::fs::rename(&repo, &moved).unwrap();
    // Old root is unknown to git now; the stored Project is untouched.
    let out = e.ok_at(
        e.path(),
        &[
            "project",
            "relink",
            repo.to_str().unwrap(),
            moved.to_str().unwrap(),
        ],
    );
    assert_eq!(out["project"]["key"], "MA");
    assert_eq!(e.ok_at(&moved, &["add", "z"])["id"], "MA-3");
    assert_eq!(e.ids_at(&moved, &[]), ["MA-1", "MA-2", "MA-3"]);

    assert_eq!(
        e.code_at(
            e.path(),
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
    e.ok_at(&other, &["add", "x"]);
    assert_eq!(
        e.code_at(
            e.path(),
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
    let plain = e.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    assert_eq!(
        e.code_at(
            e.path(),
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
    let e = Env::new();
    let repo = e.repo("a", "scree");
    e.ok_at(&repo, &["add", "x"]);
    let plain = e.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    // Ids work from any directory.
    assert_eq!(e.ok_at(&plain, &["show", "scr-1"])["id"], "SCR-1");
    assert_eq!(e.ok_at(&plain, &["show", "ScR-1"])["id"], "SCR-1");
    assert_eq!(e.code_at(&plain, &["show", "1"]), 2);
    assert_eq!(e.code_at(&plain, &["show", "SCR-x"]), 2);
    assert_eq!(e.code_at(&plain, &["show", "SCR-9"]), 3);
    assert_eq!(e.code_at(&plain, &["show", "ZZZ-1"]), 3);
}

#[test]
fn counter_is_monotonic_and_failed_adds_take_no_number() {
    let e = Env::new();
    let repo = e.repo("a", "scree");
    e.ok_at(&repo, &["add", "x"]);
    assert_eq!(e.counter("SCR"), 2);
    // Refused after the Project exists: nothing allocated.
    assert_eq!(e.code_at(&repo, &["add", "y", "--key", "QQ"]), 4);
    assert_eq!(e.code_at(&repo, &["add", "  "]), 2);
    assert_eq!(e.counter("SCR"), 2);
    // Removing the ticket row does not rewind the counter.
    e.exec_batch("DELETE FROM tickets WHERE number = 1");
    assert_eq!(e.ok_at(&repo, &["add", "z"])["id"], "SCR-2");
}

#[test]
fn concurrent_adds_get_distinct_ids() {
    let e = Env::new();
    let repo = e.repo("a", "scree");
    e.ok_at(&repo, &["add", "seed"]);
    let handles: Vec<_> = (0..6)
        .map(|i| {
            let (db, repo) = (e.db(), repo.clone());
            std::thread::spawn(move || {
                let out = scree_cmd(&db, &repo)
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
    let e = Env::new();
    let repo = e.repo("a", "scree");
    e.ok_at(&repo, &["add", "a", "--status", "ready"]);
    e.ok_at(&repo, &["add", "b", "--for", "human"]);
    e.ok_at(&repo, &["add", "c"]);
    e.ok_at(&repo, &["add", "d"]);
    e.exec_batch(
        "UPDATE tickets SET status='active', claimed_by_author='agent', claimed_by_session='s1' WHERE number=3;
         UPDATE tickets SET status='done' WHERE number=4;",
    );
    assert_eq!(
        e.ids_at(&repo, &[]),
        ["SCR-1", "SCR-2", "SCR-3"],
        "default hides done"
    );
    assert_eq!(e.ids_at(&repo, &["--status", "done"]), ["SCR-4"]);
    assert_eq!(
        e.ids_at(&repo, &["--status", "done", "--status", "ready"]),
        ["SCR-1", "SCR-4"]
    );
    assert_eq!(e.ids_at(&repo, &["--for", "human"]), ["SCR-2"]);
    assert_eq!(e.ids_at(&repo, &["--limit", "2"]), ["SCR-1", "SCR-2"]);
    assert_eq!(e.ids_at(&repo, &["--mine", "--session", "s1"]), ["SCR-3"]);
    assert!(
        e.ids_at(&repo, &["--mine", "--session", "other"])
            .is_empty()
    );
    assert_eq!(
        e.ids_at(&repo, &["--mine", "--session", "s1", "--author", "x"])
            .len(),
        0
    );
}

#[test]
fn list_scope_flags() {
    let e = Env::new();
    let a = e.repo("a", "alpha");
    let b = e.repo("b", "beta");
    let plain = e.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    e.ok_at(&a, &["add", "1"]);
    e.ok_at(&b, &["add", "2"]);
    e.ok_at(&plain, &["add", "3"]);

    assert_eq!(e.ids_at(&a, &[]), ["ALP-1"]);
    assert_eq!(e.ids_at(&plain, &[]), ["LOCAL-1"]);
    assert_eq!(
        e.ids_at(&a, &["--all-projects"]),
        ["ALP-1", "BET-1", "LOCAL-1"]
    );
    assert_eq!(
        e.ids_at(&plain, &["--project-root", b.to_str().unwrap()]),
        ["BET-1"]
    );
    assert_eq!(
        e.code_at(&plain, &["list", "--project-root", "/no/where"]),
        3
    );

    e.exec_batch("UPDATE tickets SET archived_at = '2026-01-01T00:00:00Z' WHERE number = 1 AND project_id = (SELECT id FROM projects WHERE key='ALP')");
    assert!(e.ids_at(&a, &[]).is_empty());
    assert_eq!(e.ids_at(&a, &["--archived"]), ["ALP-1"]);

    for pair in [
        ["--all-worktrees", "--all-projects"],
        ["--all-projects", "--project-root"],
    ] {
        let mut args = vec!["list", pair[0], pair[1]];
        if pair[1] == "--project-root" {
            args.push("x");
        }
        assert_eq!(e.code_at(&a, &args), 2, "{pair:?}");
    }
}

#[test]
fn never_seen_repo_lists_nothing_and_creates_nothing() {
    let e = Env::new();
    let repo = e.repo("a", "fresh");
    assert!(e.ids_at(&repo, &[]).is_empty());
    let n: i64 = rusqlite::Connection::open(e.db())
        .unwrap()
        .query_row("SELECT count(*) FROM projects", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "only LOCAL");
}

#[test]
fn separate_git_dir_repo_and_its_worktree_are_scoped() {
    let e = Env::new();
    let work = e.path().join("a").join("sep-app");
    let gitdir = e.path().join("gitdirs").join("sep-app-store");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(gitdir.parent().unwrap()).unwrap();
    git(
        &work,
        &["init", "-q", "--separate-git-dir", gitdir.to_str().unwrap()],
    );
    git(&work, &["commit", "-q", "--allow-empty", "-m", "init"]);

    let t = e.ok_at(&work, &["add", "one"]);
    assert_eq!(t["id"], "SA-1");
    assert_eq!(t["worktree"], Value::Null);
    let p = e.ok_at(&work, &["list", "--all-worktrees"]);
    assert_eq!(p["tickets"].as_array().unwrap().len(), 1);

    // Git records the root work tree for linked worktrees only in
    // `core.worktree` (older git sets it for --separate-git-dir itself).
    git(
        &gitdir,
        &["config", "core.worktree", work.to_str().unwrap()],
    );
    let wt = e.worktree(&work, "feat");
    let t = e.ok_at(&wt, &["add", "two"]);
    assert_eq!(t["id"], "SA-2");
    assert_eq!(t["worktree"], wt.to_str().unwrap());
}

#[test]
fn bare_repo_is_unscoped() {
    let e = Env::new();
    let bare = e.path().join("bare.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare"]);
    // Unscoped with no --project is the LOCAL project.
    assert_eq!(e.ok_at(&bare, &["add", "x"])["id"], "LOCAL-1");
}

#[test]
fn submodule_is_unscoped() {
    let e = Env::new();
    let upstream = e.repo("u", "lib");
    let sup = e.repo("a", "super");
    git(
        &sup,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            upstream.to_str().unwrap(),
            "sub",
        ],
    );
    assert_eq!(e.ok_at(&sup.join("sub"), &["add", "x"])["id"], "LOCAL-1");
    assert_eq!(e.ok_at(&sup, &["add", "y"])["id"], "SUP-1");
}

#[test]
fn list_honors_global_project_outside_a_repo() {
    let e = Env::new();
    let repo = e.repo("a", "my-app");
    let plain = e.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    let r = repo.to_str().unwrap();
    e.ok_at(&plain, &["--project", r, "add", "remote"]);
    assert_eq!(e.ids_at(&plain, &["--project", r]), ["MA-1"]);
    // Without the flag the plain directory is still LOCAL.
    assert!(e.ids_at(&plain, &[]).is_empty());
}

#[test]
fn relink_and_project_root_normalize_paths_to_a_gone_root() {
    let e = Env::new();
    let repo = e.repo("a", "my-app");
    e.ok_at(&repo, &["add", "x"]);
    let moved = e.path().join("a").join("moved");
    std::fs::rename(&repo, &moved).unwrap();
    // --project-root with `..` through a directory that never existed.
    assert_eq!(
        e.ids_at(e.path(), &["--project-root", "a/ghost/../my-app"]),
        ["MA-1"]
    );
    let out = e.ok_at(
        e.path(),
        &[
            "project",
            "relink",
            "a/ghost/../my-app",
            moved.to_str().unwrap(),
        ],
    );
    assert_eq!(out["project"]["key"], "MA");
}

#[test]
fn add_trims_title_and_local_text_show_omits_worktree() {
    let e = Env::new();
    let plain = e.path().join("plain");
    std::fs::create_dir(&plain).unwrap();
    let t = e.ok_at(&plain, &["add", "  padded  "]);
    assert_eq!(t["title"], "padded");
    let text = String::from_utf8(e.run_at(&plain, &["--text", "show", "LOCAL-1"]).stdout).unwrap();
    assert!(!text.contains("worktree:"), "{text}");
    let repo = e.repo("a", "scree");
    e.ok_at(&repo, &["add", "x"]);
    let text = String::from_utf8(e.run_at(&repo, &["--text", "show", "SCR-1"]).stdout).unwrap();
    assert!(text.contains("worktree: (root checkout)"), "{text}");
}

#[test]
fn list_all_projects_orders_by_project_key_then_position() {
    let e = Env::new();
    let a = e.repo("a", "alpha");
    let z = e.repo("a", "zeta");
    e.ok_at(&a, &["add", "a1"]);
    e.ok_at(&a, &["add", "a2"]);
    e.ok_at(&z, &["add", "z1"]);
    e.ok_at(&z, &["add", "z2"]);
    let ids = e.ids_at(&a, &["--all-projects"]);
    assert_eq!(ids, ["ALP-1", "ALP-2", "ZET-1", "ZET-2"]);
    assert_eq!(
        e.ids_at(&a, &["--all-projects", "--limit", "3"]),
        ["ALP-1", "ALP-2", "ZET-1"]
    );
}

#[test]
fn list_global_project_overrides_a_different_repo_cwd() {
    let e = Env::new();
    let a = e.repo("a", "alpha");
    let b = e.repo("b", "beta");
    e.ok_at(&a, &["add", "in a"]);
    e.ok_at(&b, &["add", "in b"]);
    assert_eq!(e.ids_at(&b, &["--project", a.to_str().unwrap()]), ["ALP-1"]);
    // Naming the project cwd is already in changes nothing.
    assert_eq!(e.ids_at(&b, &["--project", b.to_str().unwrap()]), ["BET-1"]);
}

#[test]
fn symlinked_path_with_dotdot_resolves_through_the_link() {
    let e = Env::new();
    let repo = e.repo("a", "my-app");
    e.ok_at(&repo, &["add", "x"]);
    // link -> a/my-app; "link/../my-app" is a/my-app/../my-app via the
    // filesystem, but lexically it would be "my-app" beside the link.
    let link = e.path().join("link");
    std::os::unix::fs::symlink(&repo, &link).unwrap();
    assert_eq!(
        e.ids_at(e.path(), &["--project-root", "link/../my-app"]),
        ["MA-1"]
    );
    let moved = e.path().join("a").join("moved");
    std::fs::rename(&repo, &moved).unwrap();
    // The root is gone and the link dangles, so the text lookup is used.
    let other = e.repo("c", "elsewhere");
    let out = e.ok_at(
        &other,
        &[
            "project",
            "relink",
            repo.to_str().unwrap(),
            moved.to_str().unwrap(),
        ],
    );
    assert_eq!(out["project"]["key"], "MA");
    // The stored root was updated: list from the new location.
    assert_eq!(e.ids_at(&moved, &[]), ["MA-1"]);
}

#[test]
fn positions_step_by_1000_per_worktree() {
    let e = Env::new();
    let repo = e.repo("a", "alpha");
    let wt = e.worktree(&repo, "feat");
    e.ok_at(&repo, &["add", "r1"]);
    e.ok_at(&repo, &["add", "r2"]);
    e.ok_at(&wt, &["add", "w1"]);
    let conn = rusqlite::Connection::open(e.db()).unwrap();
    let pos = |title: &str| -> i64 {
        conn.query_row(
            "SELECT position FROM tickets WHERE title = ?1",
            [title],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!((pos("r1"), pos("r2"), pos("w1")), (1000, 2000, 1000));
}

#[test]
fn tickets_carry_project_key_and_root() {
    let e = Env::new();
    let repo = e.repo("a", "alpha");
    let wt = e.worktree(&repo, "feat");
    e.ok_at(&repo, &["add", "root ticket"]);
    e.ok_at(&wt, &["add", "wt ticket"]);
    e.ok_at(e.path(), &["add", "local ticket"]);

    let list = e.ok_at(e.path(), &["list", "--all-projects"]);
    let by_title = |title: &str| -> Value {
        list["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["title"] == title)
            .unwrap()
            .clone()
    };
    let root = by_title("root ticket");
    let linked = by_title("wt ticket");
    let local = by_title("local ticket");
    let repo_root = repo.to_str().unwrap();
    assert_eq!(root["project"]["key"], "ALP");
    assert_eq!(root["project"]["root"], repo_root);
    assert_eq!(root["worktree"], Value::Null);
    assert_eq!(linked["project"]["key"], "ALP");
    assert_eq!(linked["project"]["root"], repo_root);
    assert_eq!(linked["worktree"], wt.to_str().unwrap());
    assert_eq!(local["project"]["key"], "LOCAL");
    assert_eq!(local["project"]["root"], Value::Null);

    let shown = e.ok_at(&wt, &["show", linked["id"].as_str().unwrap()]);
    assert_eq!(shown["project"]["key"], "ALP");
    assert_eq!(shown["project"]["root"], repo_root);
}

/// `project::list_with_counts` over three Projects, several Worktrees, an
/// unscoped ticket, a done ticket and a swept Worktree, read from the test's
/// own cwd with no git call.
#[test]
fn list_with_counts_fixture() {
    let e = Env::new();
    let W3 {
        alpha,
        wt1,
        wt2,
        gone,
        wtb,
        ..
    } = w3_fixture(&e);

    // A removed Worktree is still listed until a sweep archives its tickets.
    git(
        &alpha,
        &["worktree", "remove", "--force", gone.to_str().unwrap()],
    );
    let conn = scree::db::open(&e.db()).unwrap();
    let before = scree::project::list_with_counts(&conn).unwrap();
    let alpha_before = before.projects.iter().find(|p| p.key == "ALP").unwrap();
    assert!(
        alpha_before
            .worktrees
            .iter()
            .any(|w| w.path == gone.to_str().unwrap())
    );

    // Any command touching the project sweeps the removed Worktree.
    e.ok_at(&alpha, &["list"]);
    let o = scree::project::list_with_counts(&conn).unwrap();

    let keys: Vec<_> = o.projects.iter().map(|p| p.key.as_str()).collect();
    assert_eq!(keys, ["ALP", "BET", "GAM"]);
    let wt = |path: &Path, open| scree::model::WorktreeCount {
        open,
        path: path.to_str().unwrap().to_string(),
    };
    let a = &o.projects[0];
    assert_eq!(a.root, alpha.to_str().unwrap());
    assert_eq!((a.open, a.root_open), (5, 2));
    let mut expect = vec![wt(&wt1, 2), wt(&wt2, 1)];
    expect.sort_by(|x, y| x.path.cmp(&y.path));
    assert_eq!(a.worktrees, expect);
    let b = &o.projects[1];
    assert_eq!((b.open, b.root_open), (2, 1));
    assert_eq!(b.worktrees, vec![wt(&wtb, 1)]);
    let g = &o.projects[2];
    assert_eq!((g.open, g.root_open, g.worktrees.len()), (1, 1, 0));
    assert_eq!(o.unscoped_open, 1);
}

/// The W3 fixture: three Projects, linked Worktrees, a done ticket, an
/// unscoped ticket and a Worktree (`gone`) the test may remove.
struct W3 {
    alpha: PathBuf,
    wt1: PathBuf,
    wt2: PathBuf,
    gone: PathBuf,
    beta: PathBuf,
    wtb: PathBuf,
    gamma: PathBuf,
}

fn w3_fixture(e: &Env) -> W3 {
    let alpha = e.repo("a", "alpha");
    let beta = e.repo("a", "beta");
    let gamma = e.repo("a", "gamma");
    let wt1 = e.worktree(&alpha, "one");
    let wt2 = e.worktree(&alpha, "two");
    let gone = e.worktree(&alpha, "gone");
    let wtb = e.worktree(&beta, "bone");
    let add = |cwd: &Path, title: &str| {
        e.ok_at(cwd, &["add", title, "--status", "ready"])["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    add(&alpha, "alpha root 1");
    add(&alpha, "alpha root 2");
    add(&wt1, "wt1 a");
    add(&wt1, "wt1 b");
    add(&wt2, "wt2 a");
    let done = add(&wt2, "wt2 done");
    e.ok_at(&wt2, &["start", &done]);
    e.ok_at(&wt2, &["done", &done]);
    add(&beta, "beta root");
    add(&wtb, "bone a");
    add(&gone, "gone a");
    e.ok(&["add", "local ticket", "--status", "ready"]);
    e.ok_at(&gamma, &["add", "gamma draft"]);
    W3 {
        alpha,
        wt1,
        wt2,
        gone,
        beta,
        wtb,
        gamma,
    }
}

/// A configured `git` run in `dir`, through the shared scrub.
fn git(dir: &Path, args: &[&str]) {
    let out = git_cmd(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// Each `_in` read equals the CLI run from inside that Worktree (or with the
/// widening flag), and an unknown Project or Worktree is NotFound.
#[test]
fn scope_filter_matches_cli_scope() {
    use scree::ticket::ScopeFilter as F;
    let e = Env::new();
    let W3 {
        alpha,
        wt1,
        beta,
        gamma,
        ..
    } = w3_fixture(&e);
    let a_root = e.ok_at(&alpha, &["list", "--limit", "50"]);
    let id = a_root["tickets"][0]["id"].as_str().unwrap().to_string();
    e.ok_at(
        &alpha,
        &[
            "decision", "add", &id, "Which?", "--option", "x", "--option", "y",
        ],
    );
    e.ok_at(
        &beta,
        &[
            "--role", "human", "decision", "add", "BET-1", "Q2?", "--option", "x",
        ],
    );
    e.ok_at(
        &beta,
        &[
            "--role",
            "human",
            "decision",
            "answer",
            "D-1",
            "x",
            "--rationale",
            "r",
        ],
    );

    let conn = scree::db::open(&e.db()).unwrap();
    let who = scree::identity::Identity {
        role: scree::identity::Role::Agent,
        author: "agent".into(),
        session: None,
    };
    let key = |k: &str| k.to_string();
    let wt = |k: &str, p: &Path| F::Worktree {
        key: key(k),
        path: Some(p.to_str().unwrap().to_string()),
    };
    let la = scree::cli::ListArgs::default();
    let na = scree::cli::NextArgs {
        for_who: None,
        limit: Some(50),
        scope: Default::default(),
    };

    // (filter, cwd for the CLI, extra CLI flags)
    let cases: Vec<(F, &Path, &[&str])> = vec![
        (
            F::Worktree {
                key: key("ALP"),
                path: None,
            },
            &alpha,
            &[],
        ),
        (wt("ALP", &wt1), &wt1, &[]),
        (F::Project { key: key("ALP") }, &alpha, &["--all-worktrees"]),
        (
            F::Worktree {
                key: key("BET"),
                path: None,
            },
            &beta,
            &[],
        ),
        (F::Project { key: key("GAM") }, &gamma, &["--all-worktrees"]),
        (F::All, &alpha, &["--all-projects"]),
    ];
    for (filter, cwd, flags) in cases {
        let mut list = vec!["list"];
        list.extend(flags);
        let got = scree::ticket::list_in(&conn, &filter, &la, &who).unwrap();
        assert_eq!(
            serde_json::to_value(&got).unwrap(),
            e.ok_at(cwd, &list),
            "list {filter:?}"
        );
        let mut next = vec!["next", "--limit", "50"];
        next.extend(flags);
        let got = scree::ticket::next_in(&conn, &filter, &na, &who).unwrap();
        assert_eq!(
            serde_json::to_value(&got).unwrap(),
            e.ok_at(cwd, &next),
            "next {filter:?}"
        );
        let mut open = vec!["decisions", "--open"];
        open.extend(flags);
        let got = scree::decision::open_queue_in(&conn, &filter).unwrap();
        assert_eq!(
            serde_json::to_value(&got).unwrap(),
            e.ok_at(cwd, &open),
            "open {filter:?}"
        );
        let mut decided = vec!["decisions", "--decided"];
        decided.extend(flags);
        let got = scree::decision::decided_log_in(&conn, &filter, None, None, 50, false).unwrap();
        assert_eq!(
            serde_json::to_value(&got).unwrap(),
            e.ok_at(cwd, &decided),
            "decided {filter:?}"
        );
    }

    let nf = |f: F| {
        matches!(
            scree::ticket::list_in(&conn, &f, &la, &who),
            Err(scree::error::Error::NotFound(_))
        )
    };
    assert!(nf(F::Project { key: key("NOPE") }));
    assert!(nf(F::Worktree {
        key: key("NOPE"),
        path: None
    }));
    assert!(nf(F::Worktree {
        key: key("ALP"),
        path: Some("/no/such/worktree".into())
    }));
    assert!(matches!(
        scree::decision::open_queue_in(&conn, &F::Project { key: key("NOPE") }),
        Err(scree::error::Error::NotFound(_))
    ));
}

/// `ListArgs.search` matches id, title and description, ASCII
/// case-insensitively, with `%` literal, blank unfiltered, and composing with
/// status and scope.
#[test]
fn list_search_matches_id_title_description() {
    use scree::ticket::ScopeFilter as F;
    let e = Env::new();
    let alpha = e.repo("a", "alpha");
    let beta = e.repo("a", "beta");
    let add = |cwd: &Path, title: &str, desc: &str| {
        e.ok_at(
            cwd,
            &["add", title, "--description", desc, "--status", "ready"],
        )["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    add(&alpha, "Needle in title", "plain");
    add(&alpha, "plain two", "hides a NEEDLE here");
    add(&alpha, "ninety%", "no match");
    add(&alpha, "plain four", "plain");
    let done = add(&alpha, "plain five needle", "plain");
    e.ok_at(&alpha, &["start", &done]);
    e.ok_at(&alpha, &["done", &done]);
    add(&beta, "beta needle", "plain");

    let conn = scree::db::open(&e.db()).unwrap();
    let who = scree::identity::Identity {
        role: scree::identity::Role::Agent,
        author: "agent".into(),
        session: None,
    };
    let titles = |filter: &F, search: Option<&str>, status: Vec<scree::cli::Status>| {
        let a = scree::cli::ListArgs {
            search: search.map(String::from),
            status,
            ..Default::default()
        };
        let mut t: Vec<String> = scree::ticket::list_in(&conn, filter, &a, &who)
            .unwrap()
            .tickets
            .into_iter()
            .map(|t| t.title)
            .collect();
        t.sort();
        t
    };
    let ap = F::Project { key: "ALP".into() };

    assert_eq!(
        titles(&ap, Some("nEEdle"), vec![]),
        ["Needle in title", "plain two"]
    );
    assert_eq!(titles(&ap, Some("%"), vec![]), ["ninety%"]);
    assert_eq!(titles(&ap, Some("_"), vec![]), Vec::<String>::new());
    assert_eq!(titles(&ap, Some("alp-2"), vec![]), ["plain two"]);
    assert_eq!(titles(&ap, Some("  "), vec![]).len(), 4);
    assert_eq!(titles(&ap, None, vec![]).len(), 4);
    assert_eq!(
        titles(&ap, Some("needle"), vec![scree::cli::Status::Done]),
        ["plain five needle"]
    );
    assert_eq!(
        titles(&F::All, Some("needle"), vec![]),
        ["Needle in title", "beta needle", "plain two"]
    );
}

/// `count_in` equals the unlimited `list_in` for every scope case, with and
/// without a search, and ignores `limit`.
#[test]
fn count_in_equals_unlimited_list() {
    use scree::ticket::ScopeFilter as F;
    let e = Env::new();
    let W3 { wt1, .. } = w3_fixture(&e);
    let conn = scree::db::open(&e.db()).unwrap();
    let who = scree::identity::Identity {
        role: scree::identity::Role::Agent,
        author: "agent".into(),
        session: None,
    };
    let wt = |k: &str, p: Option<&Path>| F::Worktree {
        key: k.into(),
        path: p.map(|p| p.to_str().unwrap().to_string()),
    };
    let filters = vec![
        F::All,
        F::Project { key: "ALP".into() },
        wt("ALP", None),
        wt("ALP", Some(&wt1)),
        F::Project { key: "BET".into() },
        F::Project { key: "GAM".into() },
    ];
    for filter in &filters {
        for search in [None, Some("a"), Some("wt1"), Some("nomatch")] {
            for (limit, archived) in [(None, false), (Some(1), false), (Some(1), true)] {
                let args = |limit| scree::cli::ListArgs {
                    search: search.map(String::from),
                    limit,
                    archived,
                    ..Default::default()
                };
                let full = scree::ticket::list_in(&conn, filter, &args(Some(u32::MAX)), &who)
                    .unwrap()
                    .tickets
                    .len();
                let got = scree::ticket::count_in(&conn, filter, &args(limit), &who).unwrap();
                assert_eq!(got, full as u64, "{filter:?} {search:?} {limit:?}");
            }
        }
    }
    let all = scree::ticket::count_in(&conn, &F::All, &Default::default(), &who).unwrap();
    assert!(all > 1);
    assert!(matches!(
        scree::ticket::count_in(
            &conn,
            &F::Project { key: "NOPE".into() },
            &Default::default(),
            &who
        ),
        Err(scree::error::Error::NotFound(_))
    ));
}
