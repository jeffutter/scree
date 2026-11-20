//! Subtasks, hierarchy rules and `move`, through the real binary against
//! throwaway repos and worktrees.

mod common;

use std::path::{Path, PathBuf};

use common::{Env, git_cmd};
use serde_json::Value;

// Repo and worktree builders on top of the shared harness. The environment's
// directory is not a repository, so every command here names the cwd it runs in.
impl Env {
    fn repo(&self, name: &str) -> PathBuf {
        let dir = self.path().join("repos").join(name);
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

    /// Add as an agent-visible ticket in `ready` state; its id.
    fn add(&self, cwd: &Path, title: &str, extra: &[&str]) -> String {
        let mut a = vec!["--role", "human", "add", title, "--status", "ready"];
        a.extend_from_slice(extra);
        self.ok_at(cwd, &a)["id"].as_str().unwrap().to_string()
    }

    fn sub(&self, cwd: &Path, parent: &str, title: &str) -> String {
        self.add(cwd, title, &["--parent", parent])
    }

    fn next(&self, cwd: &Path) -> Vec<String> {
        self.ok_at(cwd, &["--role", "agent", "next", "--limit", "10"])["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    }
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

#[test]
fn subtask_ids_scope_and_counters() {
    let e = Env::new();
    let repo = e.repo("scree");
    let wt = e.worktree(&repo, "feat");
    let a = e.add(&wt, "a", &[]);
    let b = e.add(&wt, "b", &[]);
    assert_eq!((a.as_str(), b.as_str()), ("SCR-1", "SCR-2"));

    // The parent's Scope wins over where the caller stands.
    let c = e.ok_at(&repo, &["add", "c", "--parent", "SCR-2"]);
    assert_eq!(s(&c["id"]), "SCR-2.1");
    assert_eq!(s(&c["parent"]), "SCR-2");
    assert_eq!(s(&c["worktree"]), wt.to_str().unwrap());
    let d = e.ok_at(&wt, &["add", "d", "--parent", "scr-2"]);
    assert_eq!(s(&d["id"]), "SCR-2.2");
    let first = e.ok_at(&wt, &["add", "e", "--parent", "SCR-1"]);
    assert_eq!(s(&first["id"]), "SCR-1.1", "numbers are per parent");

    // Subtasks do not consume the Project counter.
    assert_eq!(e.add(&wt, "f", &[]), "SCR-3");

    // Show works on subtask ids and lists subtasks on the parent.
    let shown = e.ok_at(&wt, &["show", "SCR-2.2"]);
    assert_eq!(s(&shown["title"]), "d");
    let parent = e.ok_at(&wt, &["show", "SCR-2"]);
    assert_eq!(parent["subtasks"].as_array().unwrap().len(), 2);
    assert_eq!(e.code_at(&wt, &["show", "SCR-2.9"]), 3);
    assert_eq!(e.code_at(&wt, &["show", "SCR-9.1"]), 3);
}

#[test]
fn third_level_and_bad_parents_are_refused() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let sub = e.sub(&repo, &a, "sub");
    assert_eq!(e.code_at(&repo, &["add", "x", "--parent", &sub]), 4);
    assert_eq!(e.code_at(&repo, &["show", "SCR-1.1.1"]), 2);
    assert_eq!(e.code_at(&repo, &["add", "x", "--parent", "SCR-99"]), 3);
    assert_eq!(
        e.code_at(&repo, &["add", "x", "--parent", &a, "--key", "ZZ"]),
        2
    );

    // A settled parent takes no new subtasks.
    let b = e.add(&repo, "b", &[]);
    e.ok_at(&repo, &["done", &b, "--force"]);
    assert_eq!(e.code_at(&repo, &["add", "x", "--parent", &b]), 4);

    // A parent in another Project than the one the caller names is refused.
    let other = e.repo("other");
    e.add(&other, "o", &[]);
    assert_eq!(e.code_at(&other, &["add", "x", "--parent", &a]), 4);
}

#[test]
fn parent_is_blocked_until_subtasks_settle() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let c1 = e.sub(&repo, &p, "c1");
    let c2 = e.sub(&repo, &p, "c2");

    let shown = e.ok_at(&repo, &["show", &p]);
    assert_eq!(shown["blocked"], true);
    assert_eq!(shown["available"], false);
    let by: Vec<&str> = shown["blocked_by"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| s(&b["id"]))
        .collect();
    assert_eq!(by, [c1.as_str(), c2.as_str()]);
    assert_eq!(e.code_at(&repo, &["--role", "agent", "start", &p]), 4);

    // done is refused, even forced, while any subtask is open.
    assert_eq!(e.code_at(&repo, &["done", &p, "--force"]), 4);
    e.ok_at(&repo, &["done", &c1, "--force"]);
    assert_eq!(e.code_at(&repo, &["done", &p, "--force"]), 4);
    e.ok_at(&repo, &["abandon", &c2, "no longer needed"]);
    let shown = e.ok_at(&repo, &["show", &p]);
    assert_eq!(shown["blocked"], false);
    assert_eq!(shown["available"], true);
    assert_eq!(
        s(&e.ok_at(&repo, &["done", &p, "--force"])["status"]),
        "done"
    );
}

#[test]
fn abandoning_a_parent_abandons_open_subtasks_with_comments() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let done = e.sub(&repo, &p, "finished");
    let open = e.sub(&repo, &p, "open");
    let active = e.sub(&repo, &p, "active");
    e.ok_at(&repo, &["done", &done, "--force"]);
    e.ok_at(&repo, &["--role", "agent", "start", &open]);
    e.ok_at(&repo, &["release", &open]);
    e.ok_at(&repo, &["reorder", &active, "--first"]);
    e.ok_at(&repo, &["--role", "agent", "start", &active]);

    let t = e.ok_at(&repo, &["abandon", &p, "dropped"]);
    assert_eq!(s(&t["status"]), "abandoned");
    assert_eq!(s(&t["comments"][0]["body"]), "dropped");

    let o = e.ok_at(&repo, &["show", &open]);
    assert_eq!(s(&o["status"]), "abandoned");
    assert_eq!(s(&o["comments"][0]["body"]), "Parent SCR-1 abandoned");
    let a = e.ok_at(&repo, &["show", &active]);
    assert_eq!(s(&a["status"]), "abandoned");
    assert!(a["claimed_by"].is_null());
    assert_eq!(s(&e.ok_at(&repo, &["show", &done])["status"]), "done");

    // A subtask cannot be reopened under a settled parent.
    assert_eq!(e.code_at(&repo, &["reopen", &open]), 4);
    e.ok_at(&repo, &["reopen", &p]);
    assert_eq!(s(&e.ok_at(&repo, &["reopen", &open])["status"]), "ready");
}

#[test]
fn only_the_first_unsettled_sibling_is_available() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let c1 = e.sub(&repo, &p, "c1");
    let c2 = e.sub(&repo, &p, "c2");
    let c3 = e.sub(&repo, &p, "c3");

    assert_eq!(e.next(&repo), [c1.as_str()]);
    assert_eq!(e.ok_at(&repo, &["show", &c2])["available"], false);
    assert_eq!(e.ok_at(&repo, &["show", &c1])["available"], true);

    // Claimed, the first sibling still holds the line.
    e.ok_at(&repo, &["--role", "agent", "start", "--next"]);
    assert!(e.next(&repo).is_empty());

    e.ok_at(&repo, &["done", &c1, "--force"]);
    assert_eq!(e.next(&repo), [c2.as_str()]);
    e.ok_at(&repo, &["abandon", &c2, "skip"]);
    assert_eq!(e.next(&repo), [c3.as_str()]);

    // Reordering changes which sibling is first.
    let c4 = e.sub(&repo, &p, "c4");
    e.ok_at(&repo, &["reorder", &c4, "--first"]);
    assert_eq!(e.next(&repo), [c4.as_str()]);
    assert_eq!(
        e.ok_at(&repo, &["--role", "agent", "start", "--next"])["id"],
        c4.as_str()
    );

    // list --parent shows only that parent's subtasks.
    assert_eq!(
        e.ids_at(
            &repo,
            &["--parent", &p, "--status", "done", "--status", "abandoned"]
        ),
        [c1.as_str(), c2.as_str()]
    );
}

#[test]
fn list_parent_filter() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let q = e.add(&repo, "q", &[]);
    let c = e.sub(&repo, &p, "c");
    assert_eq!(e.ids_at(&repo, &["--parent", &p]), [c.as_str()]);
    assert!(e.ids_at(&repo, &["--parent", &q]).is_empty());
    // A parent held by its subtasks is blocked; the work is the subtask.
    assert_eq!(e.ok_at(&repo, &["show", &p])["blocked"], true);
    assert_eq!(e.next(&repo), [c.as_str(), q.as_str()]);
}

#[test]
fn move_takes_the_whole_tree_and_places_it_last() {
    let e = Env::new();
    let repo = e.repo("scree");
    let wt = e.worktree(&repo, "feat");
    let p = e.add(&repo, "p", &[]);
    let c = e.sub(&repo, &p, "c");
    let other = e.add(&wt, "other", &[]);

    let t = e.ok_at(&repo, &["move", &p, "--worktree", wt.to_str().unwrap()]);
    assert_eq!(s(&t["worktree"]), wt.to_str().unwrap());
    assert_eq!(
        s(&e.ok_at(&repo, &["show", &c])["worktree"]),
        wt.to_str().unwrap()
    );
    // The tree sits after `other` among top-level tickets.
    let mut moved = e.ids_at(&wt, &[]);
    moved.sort();
    let mut want = vec![p.clone(), c.clone(), other.clone()];
    want.sort();
    assert_eq!(moved, want);
    assert!(e.ids_at(&repo, &[]).is_empty());
    let pos: i64 = e.sql(
        "SELECT position FROM tickets WHERE number = 1 AND parent_id IS NULL",
        |r| r.get(0),
    );
    assert_eq!(pos, 2000);

    // Back to the root checkout, by path and with --root.
    let t = e.ok_at(&repo, &["move", &p, "--root"]);
    assert!(t["worktree"].is_null());
    assert!(e.ok_at(&repo, &["show", &c])["worktree"].is_null());
    e.ok_at(&repo, &["move", &p, "--worktree", wt.to_str().unwrap()]);
    let t = e.ok_at(&wt, &["move", &p, "--worktree", repo.to_str().unwrap()]);
    assert!(t["worktree"].is_null());

    // Done trees may move; the move rewrites a label only.
    e.ok_at(&repo, &["done", &c, "--force"]);
    e.ok_at(&repo, &["done", &p, "--force"]);
    e.ok_at(&repo, &["move", &p, "--worktree", wt.to_str().unwrap()]);
    assert_eq!(s(&e.ok_at(&wt, &["show", &p])["status"]), "done");
}

#[test]
fn move_refusals() {
    let e = Env::new();
    let repo = e.repo("scree");
    let wt = e.worktree(&repo, "feat");
    let other = e.repo("other");
    let other_wt = e.worktree(&other, "o");
    let p = e.add(&repo, "p", &[]);
    let c = e.sub(&repo, &p, "c");

    // A subtask moves with its parent only.
    assert_eq!(
        e.code_at(&repo, &["move", &c, "--worktree", wt.to_str().unwrap()]),
        4
    );
    // Another Project's checkout or worktree is refused, and nothing changes.
    e.add(&other, "o", &[]);
    for target in [&other, &other_wt] {
        assert_eq!(
            e.code_at(&repo, &["move", &p, "--worktree", target.to_str().unwrap()]),
            4
        );
    }
    assert!(e.ok_at(&repo, &["show", &p])["worktree"].is_null());
    // Not a repository.
    let plain = e.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    assert_eq!(
        e.code_at(&repo, &["move", &p, "--worktree", plain.to_str().unwrap()]),
        4
    );
    // Exactly one target.
    assert_eq!(e.code_at(&repo, &["move", &p]), 2);
    assert_eq!(
        e.code_at(
            &repo,
            &["move", &p, "--root", "--worktree", wt.to_str().unwrap()]
        ),
        2
    );
    // LOCAL tickets have no worktrees.
    let local = e.path().join("plain");
    let l = e.add(&local, "l", &[]);
    assert_eq!(e.code_at(&local, &["move", &l, "--root"]), 2);
    assert_eq!(
        e.code_at(&local, &["move", &l, "--worktree", wt.to_str().unwrap()]),
        4
    );
}

#[test]
fn start_refuses_a_later_sibling_until_earlier_ones_settle() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let c1 = e.sub(&repo, &p, "c1");
    let c2 = e.sub(&repo, &p, "c2");
    let c3 = e.sub(&repo, &p, "c3");

    let refused = e.code_at(&repo, &["--role", "agent", "start", &c2]);
    assert_ne!(refused, 0);
    assert_eq!(e.ok_at(&repo, &["show", &c2])["status"], "ready");

    e.ok_at(&repo, &["--role", "agent", "start", &c1]);
    e.ok_at(&repo, &["done", &c1, "--force"]);
    e.ok_at(&repo, &["--role", "agent", "start", &c2]);
    e.ok_at(&repo, &["abandon", &c2, "skip"]);
    e.ok_at(&repo, &["--role", "agent", "start", &c3]);
}

// Re-parenting with `edit --parent` / `--no-parent`.

fn comments(e: &Env, cwd: &Path, id: &str) -> Vec<String> {
    e.ok_at(cwd, &["show", id])["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| s(&c["body"]).to_string())
        .collect()
}

#[test]
fn no_parent_promotes_a_subtask_and_keeps_its_row() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let c = e.add(
        &repo,
        "c",
        &["--parent", &p, "--plan", "the plan", "--criterion", "crit"],
    );
    let other = e.add(&repo, "other", &[]);
    e.ok_at(&repo, &["comment", "add", &c, "a note"]);
    e.ok_at(&repo, &["dep", "add", &other, &c]);
    let row_before: i64 = e.sql("SELECT id FROM tickets WHERE title = 'c'", |r| r.get(0));

    let t = e.ok_at(&repo, &["edit", &c, "--no-parent"]);
    assert_eq!(t["id"], "SCR-1.1", "re-parenting keeps the id");
    assert!(t["parent"].is_null());
    assert_eq!(t["plan"], "the plan");
    assert_eq!(t["status"], "ready");
    assert_eq!(t["criteria"].as_array().unwrap().len(), 1);
    let row_after: i64 = e.sql("SELECT id FROM tickets WHERE title = 'c'", |r| r.get(0));
    assert_eq!(row_before, row_after);

    let notes = comments(&e, &repo, "SCR-1.1");
    assert!(notes.contains(&"a note".to_string()));
    assert!(notes.iter().any(|n| n.contains("top-level")));
    // The same id still resolves, and so does the blocker edge.
    assert_eq!(e.ok_at(&repo, &["show", &c])["id"], "SCR-1.1");
    assert_eq!(
        e.ok_at(&repo, &["show", &other])["blocked_by"][0]["id"],
        "SCR-1.1"
    );
    // A subtask of the promoted ticket cannot nest a third id level, so it
    // takes a fresh Project number.
    let g = e.add(&repo, "g", &["--parent", "SCR-1.1"]);
    assert_eq!(g, "SCR-4");
    assert_eq!(e.ok_at(&repo, &["show", &g])["parent"], "SCR-1.1");
    // The parent no longer lists it.
    assert!(e.ids_at(&repo, &["--parent", &p]).is_empty());
}

#[test]
fn edit_parent_moves_under_another_top_level_ticket_last() {
    let e = Env::new();
    let repo = e.repo("scree");
    let wt = e.worktree(&repo, "feat");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&wt, "b", &[]);
    let b1 = e.sub(&wt, &b, "b1");
    let loose = e.add(&repo, "loose", &[]);

    let t = e.ok_at(&repo, &["edit", &loose, "--parent", &b]);
    assert_eq!(t["id"], "SCR-3", "re-parenting keeps the id");
    assert_eq!(t["parent"], "SCR-2");
    // It takes the new parent's worktree.
    assert_eq!(
        e.sql(
            "SELECT worktree_path FROM tickets WHERE title = 'loose'",
            |r| r.get::<_, String>(0)
        ),
        wt.to_str().unwrap()
    );
    assert_eq!(
        e.ids_at(&wt, &["--parent", &b]),
        vec![b1.clone(), "SCR-3".to_string()]
    );
    // Last among siblings, so the first sibling is the only one available.
    assert_eq!(e.ids_at(&wt, &["--available", "--parent", &b]), vec![b1]);
    let _ = a;
    assert!(
        comments(&e, &repo, "SCR-3")
            .iter()
            .any(|n| n.contains("SCR-3") && n.contains("SCR-2"))
    );
}

#[test]
fn promoted_ticket_can_be_moved_to_a_worktree() {
    let e = Env::new();
    let repo = e.repo("scree");
    let wt = e.worktree(&repo, "feat");
    let p = e.add(&repo, "p", &[]);
    let c = e.sub(&repo, &p, "c");
    assert_eq!(
        e.code_at(&repo, &["move", &c, "--worktree", wt.to_str().unwrap()]),
        4
    );
    let t = e.ok_at(&repo, &["edit", &c, "--no-parent"]);
    let id = s(&t["id"]).to_string();
    e.ok_at(&repo, &["move", &id, "--worktree", wt.to_str().unwrap()]);
    assert_eq!(e.ids_at(&wt, &[]), vec![id]);
}

#[test]
fn reparent_refusals_exit_4() {
    let e = Env::new();
    let repo = e.repo("scree");
    let other_repo = e.repo("other");
    let p = e.add(&repo, "p", &[]);
    let c = e.sub(&repo, &p, "c");
    let q = e.add(&repo, "q", &[]);
    let top = e.add(&repo, "top", &[]);
    let foreign = e.add(&other_repo, "foreign", &[]);
    let human = e.add(&repo, "human", &["--for", "human"]);
    let done = e.add(&repo, "done", &[]);
    e.ok_at(&repo, &["done", &done, "--force"]);

    let refused = |args: &[&str]| {
        let mut a = vec!["--role", "human", "edit"];
        a.extend_from_slice(args);
        let out = e.run_at(&repo, &a);
        assert_eq!(out.status.code(), Some(4), "{args:?}: {out:?}");
    };
    refused(&[&top, "--parent", &c]); // target is a subtask
    refused(&[&p, "--parent", &q]); // has subtasks
    refused(&[&top, "--parent", &foreign]); // other Project
    refused(&[&top, "--parent", &top]); // itself
    refused(&[&top, "--no-parent"]); // already top-level
    refused(&[&c, "--parent", &p]); // already under it
    refused(&[&top, "--parent", &done]); // settled parent

    // Active and claimed.
    e.ok_at(&repo, &["--role", "agent", "start", &top]);
    refused(&[&top, "--parent", &q]);
    e.ok_at(&repo, &["--role", "agent", "release", &top]);

    // An agent may not re-parent a human ticket.
    let out = e.run_at(&repo, &["--role", "agent", "edit", &human, "--parent", &q]);
    assert_eq!(out.status.code(), Some(4));
    // Archived.
    e.ok_at(&repo, &["archive", &top]);
    refused(&[&top, "--parent", &q]);
    // The flags conflict (usage error).
    assert_eq!(
        e.code_at(&repo, &["edit", &c, "--parent", &q, "--no-parent"]),
        2
    );
}

#[test]
fn reparent_refuses_a_dependency_cycle() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let x = e.add(&repo, "x", &[]);
    // x waits on p, so x cannot become p's subtask (p waits on its subtasks).
    e.ok_at(&repo, &["dep", "add", &x, &p]);
    let out = e.run_at(&repo, &["--role", "human", "edit", &x, "--parent", &p]);
    assert_eq!(out.status.code(), Some(4), "{out:?}");
    assert!(Env::stderr(&out).contains("cycle"));
    // Nothing changed: x is still top-level.
    assert!(e.ok_at(&repo, &["show", &x])["parent"].is_null());
}

/// A configured `git` run in `dir`, through the shared scrub.
fn git(dir: &Path, args: &[&str]) {
    let out = git_cmd(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}
