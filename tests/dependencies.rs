//! Dependencies and the computed Blocked and Available state, through the real binary against
//! throwaway repos and worktrees.

mod common;

use std::path::{Path, PathBuf};

use common::{Env, git_cmd};

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

#[test]
fn dep_add_rm_and_refusals() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&repo, "b", &[]);
    let c = e.add(&repo, "c", &[]);

    let t = e.ok_at(&repo, &["dep", "add", &a, &b]);
    assert_eq!(t["blocked_by"][0]["id"], "SCR-2");
    // Idempotent.
    e.ok_at(&repo, &["dep", "add", &a, &b]);
    assert_eq!(
        e.ok_at(&repo, &["show", &a])["blocked_by"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Self edge and two-cycle.
    assert_eq!(e.code_at(&repo, &["dep", "add", &a, &a]), 4);
    assert_eq!(e.code_at(&repo, &["dep", "add", &b, &a]), 4);
    // Three-cycle: a <- b <- c, then c blocked by... a already waits on b.
    e.ok_at(&repo, &["dep", "add", &b, &c]);
    assert_eq!(e.code_at(&repo, &["dep", "add", &c, &a]), 4);
    // A refused edge leaves nothing behind.
    assert_eq!(
        e.ok_at(&repo, &["show", &c])["blocked_by"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // Unknown ticket.
    assert_eq!(e.code_at(&repo, &["dep", "add", &a, "SCR-99"]), 3);

    // Remove, and removing again is a no-op.
    e.ok_at(&repo, &["dep", "rm", &a, &b]);
    e.ok_at(&repo, &["dep", "rm", &a, &b]);
    assert!(
        e.ok_at(&repo, &["show", &a])["blocked_by"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // With the edge gone the former cycle is fine.
    e.ok_at(&repo, &["dep", "add", &b, &a]);
}

#[test]
fn cycles_count_parent_and_sibling_order() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let s1 = e.sub(&repo, &p, "s1");
    let s2 = e.sub(&repo, &p, "s2");
    // The parent waits on its subtasks, so a subtask cannot wait on it.
    assert_eq!(e.code_at(&repo, &["dep", "add", &s1, &p]), 4);
    // s2 waits on s1 by order, so s1 cannot wait on s2.
    assert_eq!(e.code_at(&repo, &["dep", "add", &s1, &s2]), 4);
    e.ok_at(&repo, &["dep", "add", &s2, &s1]);
}

#[test]
fn reorder_refuses_a_move_that_closes_a_cycle() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let s1 = e.sub(&repo, &p, "s1");
    let s2 = e.sub(&repo, &p, "s2");
    let s3 = e.sub(&repo, &p, "s3");
    e.ok_at(&repo, &["dep", "add", &s2, &s1]);
    let before = e.ok_at(&repo, &["show", &p]).to_string();
    // s1 after s2 would make s1 wait on s2, which waits on s1.
    assert_eq!(e.code_at(&repo, &["reorder", &s1, "--after", &s2]), 4);
    assert_eq!(e.code_at(&repo, &["reorder", &s2, "--before", &s1]), 4);
    assert_eq!(e.code_at(&repo, &["reorder", &s1, "--last"]), 4);
    assert_eq!(e.ok_at(&repo, &["show", &p]).to_string(), before);
    // A move that adds no cycle still works.
    e.ok_at(&repo, &["reorder", &s3, "--first"]);
}

#[test]
fn cross_project_refused_cross_worktree_allowed() {
    let e = Env::new();
    let repo = e.repo("scree");
    let wa = e.worktree(&repo, "feat-a");
    let wb = e.worktree(&repo, "feat-b");
    let a = e.add(&wa, "a", &[]);
    let b = e.add(&wb, "b", &[]);
    e.ok_at(&wa, &["dep", "add", &a, &b]);

    let other = e.repo("other");
    let o = e.add(&other, "o", &[]);
    assert_eq!(e.code_at(&wa, &["dep", "add", &a, &o]), 4);
    assert_eq!(e.code_at(&wa, &["dep", "add", &o, &a]), 4);
}

#[test]
fn blocked_until_done_and_abandoned_still_blocks() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&repo, "b", &[]);
    e.ok_at(&repo, &["dep", "add", &a, &b]);

    assert_eq!(e.ids_at(&repo, &["--blocked"]), vec![a.clone()]);
    assert_eq!(e.ids_at(&repo, &["--available"]), vec![b.clone()]);
    assert_eq!(e.next(&repo), vec![b.clone()]);
    assert_eq!(e.code_at(&repo, &["--role", "agent", "start", &a]), 4);

    e.ok_at(&repo, &["abandon", &b, "nope"]);
    let shown = e.ok_at(&repo, &["show", &a]);
    assert_eq!(shown["blocked_by"][0]["status"], "abandoned");
    assert_eq!(e.ids_at(&repo, &["--blocked"]), vec![a.clone()]);
    assert!(e.ids_at(&repo, &["--available"]).is_empty());

    e.ok_at(&repo, &["reopen", &b]);
    e.ok_at(&repo, &["done", &b, "--force"]);
    assert!(e.ids_at(&repo, &["--blocked"]).is_empty());
    assert_eq!(e.ids_at(&repo, &["--available"]), vec![a.clone()]);
    assert!(
        e.ok_at(&repo, &["show", &a])["blocked_by"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Conflicting flags are a usage error.
    assert_eq!(e.code_at(&repo, &["list", "--blocked", "--available"]), 2);
}

#[test]
fn blocked_by_shape_text_and_next_pending() {
    let e = Env::new();
    let repo = e.repo("scree");
    let wa = e.worktree(&repo, "feat-a");
    let wb = e.worktree(&repo, "feat-b");
    let a = e.add(&wa, "a", &[]);
    let b = e.add(&wb, "bee", &[]);
    e.ok_at(&wa, &["dep", "add", &a, &b]);

    let shown = e.ok_at(&wa, &["show", &a]);
    let bb = &shown["blocked_by"][0];
    assert_eq!(bb["id"], "SCR-2");
    assert_eq!(bb["title"], "bee");
    assert_eq!(bb["status"], "ready");
    assert_eq!(bb["worktree"], wb.to_str().unwrap());

    let out = e.run_at(&wa, &["--text", "list"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("[blocked by SCR-2 in feat-b]"), "{text}");

    let n = e.ok_at(&wa, &["--role", "agent", "next"]);
    assert!(n["tickets"].as_array().unwrap().is_empty());
    assert_eq!(n["blocked_pending"], 1);
}

#[test]
fn add_blocked_by_sets_edges_and_refuses_cross_project() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&repo, "b", &[]);
    let c = e.add(&repo, "c", &["--blocked-by", &a, "--blocked-by", &b]);
    let shown = e.ok_at(&repo, &["show", &c]);
    assert_eq!(shown["blocked_by"].as_array().unwrap().len(), 2);
    assert_eq!(e.ids_at(&repo, &["--blocked"]), [c]);

    let other = e.repo("other");
    let o = e.add(&other, "o", &[]);
    let before: i64 = e.sql("SELECT count(*) FROM tickets", |r| r.get(0));
    assert_eq!(e.code_at(&repo, &["add", "x", "--blocked-by", &o]), 4);
    assert_eq!(e.code_at(&repo, &["add", "x", "--blocked-by", "SCR-99"]), 3);
    let after: i64 = e.sql("SELECT count(*) FROM tickets", |r| r.get(0));
    assert_eq!(before, after, "a refused add creates no ticket");
}

#[test]
fn add_subtask_blocked_by_parent_chain_is_a_cycle() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let count = || e.sql("SELECT count(*) FROM tickets", |r| r.get::<_, i64>(0));
    let before = count();
    // The parent waits on every subtask, so a new subtask cannot wait on it.
    assert_eq!(
        e.code_at(&repo, &["add", "s", "--parent", &p, "--blocked-by", &p]),
        4
    );
    // Nor on a ticket that already waits on the parent.
    let x = e.add(&repo, "x", &["--blocked-by", &p]);
    let before = before + 1;
    assert_eq!(count(), before);
    assert_eq!(
        e.code_at(&repo, &["add", "s", "--parent", &p, "--blocked-by", &x]),
        4
    );
    assert_eq!(count(), before, "a refused add creates no ticket");
    // Top-level and unrelated blockers still work.
    let y = e.add(&repo, "y", &[]);
    e.add(&repo, "z", &["--blocked-by", &y]);
    e.add(&repo, "s", &["--parent", &p, "--blocked-by", &y]);
}

/// What `blocks` returns for `id`, as (id, holding) pairs, on the env's database.
fn blocks_of(e: &Env, id: &str) -> Vec<(String, bool)> {
    let conn = scree::db::open(&e.db()).unwrap();
    scree::ticket::blocks(&conn, id)
        .unwrap()
        .into_iter()
        .map(|b| (b.id, b.holding))
        .collect()
}

#[test]
fn blocks_chain_is_direct_only_and_empty_when_nothing_waits() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&repo, "b", &["--blocked-by", &a]);
    let c = e.add(&repo, "c", &["--blocked-by", &b]);
    assert_eq!(blocks_of(&e, &a), vec![(b.clone(), true)]);
    assert_eq!(blocks_of(&e, &b), vec![(c.clone(), true)]);
    assert!(blocks_of(&e, &c).is_empty());
}

#[test]
fn blocks_crosses_worktrees_and_reports_the_worktree() {
    let e = Env::new();
    let repo = e.repo("scree");
    let wa = e.worktree(&repo, "feat-a");
    let wb = e.worktree(&repo, "feat-b");
    let a = e.add(&wa, "a", &[]);
    let b = e.add(&wb, "b", &["--blocked-by", &a]);
    let conn = scree::db::open(&e.db()).unwrap();
    let got = scree::ticket::blocks(&conn, &a).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].id, b);
    assert!(got[0].holding);
    assert_eq!(
        got[0].worktree.as_deref(),
        Some(wb.to_str().unwrap()),
        "the dependent's Worktree, not the blocker's"
    );
}

#[test]
fn blocks_keeps_dependents_of_a_done_blocker_without_holding() {
    let e = Env::new();
    let repo = e.repo("scree");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&repo, "b", &["--blocked-by", &a]);
    e.ok_at(&repo, &["done", &a, "--force"]);
    assert_eq!(blocks_of(&e, &a), vec![(b.clone(), false)]);

    // An abandoned blocker still holds, as in `blocked_by`.
    let c = e.add(&repo, "c", &[]);
    let d = e.add(&repo, "d", &["--blocked-by", &c]);
    e.ok_at(&repo, &["abandon", &c, "nope"]);
    assert_eq!(blocks_of(&e, &c), vec![(d, true)]);
}

#[test]
fn blocks_of_a_subtask_includes_its_parent_while_open() {
    let e = Env::new();
    let repo = e.repo("scree");
    let p = e.add(&repo, "p", &[]);
    let s = e.sub(&repo, &p, "s");
    assert_eq!(blocks_of(&e, &s), vec![(p.clone(), true)]);
    e.ok_at(&repo, &["done", &s, "--force"]);
    assert_eq!(blocks_of(&e, &s), vec![(p, false)]);
}

#[test]
fn blocks_holding_edges_match_scree_graph() {
    let e = Env::new();
    let repo = e.repo("scree");
    let wa = e.worktree(&repo, "feat-a");
    let a = e.add(&repo, "a", &[]);
    let b = e.add(&wa, "b", &["--blocked-by", &a]);
    let c = e.add(&repo, "c", &["--blocked-by", &a, "--blocked-by", &b]);
    let p = e.add(&repo, "p", &[]);
    let s = e.sub(&repo, &p, "s");
    let x = e.add(&repo, "x", &[]);
    e.add(&repo, "y", &["--blocked-by", &x]);
    e.ok_at(&repo, &["done", &x, "--force"]);

    let g = e.ok_at(
        &repo,
        &[
            "graph",
            "--json",
            "--all-worktrees",
            "--status",
            "draft",
            "--status",
            "ready",
            "--status",
            "active",
            "--status",
            "done",
            "--status",
            "abandoned",
        ],
    );
    let mut graph: Vec<(String, String)> = g["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["from"].as_str().unwrap().to_string(),
                d["to"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    graph.sort();
    assert!(!graph.is_empty());

    let mut query = Vec::new();
    for id in [&a, &b, &c, &p, &s, &x] {
        for (to, holding) in blocks_of(&e, id) {
            if holding {
                query.push((id.clone(), to));
            }
        }
    }
    query.sort();
    assert_eq!(query, graph);
}

/// A configured `git` run in `dir`, through the shared scrub.
fn git(dir: &Path, args: &[&str]) {
    let out = git_cmd(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}
