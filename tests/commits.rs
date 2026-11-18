//! Commit links: `done` records the Worktree's HEAD, `--commit` and `link` add
//! more. Real throwaway repos, since the shas come from the git CLI.

mod common;

use std::path::{Path, PathBuf};

use common::{Env, git_cmd};
use serde_json::Value;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = git_cmd(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn repo(e: &Env) -> PathBuf {
    let dir = e.path().join("proj").join("my-app");
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    dir
}

/// A started ticket with no criteria, ready to finish.
fn started(e: &Env, cwd: &Path) -> String {
    let id = e.ok_at(cwd, &["add", "T"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    e.ok_at(cwd, &["ready", &id]);
    e.ok_at(cwd, &["start", &id]);
    id
}

#[test]
fn done_records_head_and_branch() {
    let e = Env::new();
    let dir = repo(&e);
    let id = started(&e, &dir);
    let head = git(&dir, &["rev-parse", "HEAD"]);
    let t = e.ok_at(&dir, &["done", &id]);
    assert_eq!(t["commits"].as_array().unwrap().len(), 1);
    assert_eq!(t["commits"][0]["sha"], head.as_str());
    assert_eq!(t["commits"][0]["branch"], "main");
    // Key order inside a commit is alphabetical.
    let raw = e.stdout(&["show", &id]);
    let (b, l, s) = (
        raw.find("\"branch\"").unwrap(),
        raw.find("\"linked_at\"").unwrap(),
        raw.find("\"sha\"").unwrap(),
    );
    assert!(b < l && l < s);
}

#[test]
fn done_outside_a_repo_records_nothing_and_succeeds() {
    let e = Env::new();
    let id = started(&e, e.path());
    let t = e.ok(&["done", &id]);
    assert_eq!(t["status"], "done");
    assert_eq!(t["commits"], Value::Array(vec![]));
}

#[test]
fn commit_flag_repeats_and_dedups() {
    let e = Env::new();
    let dir = repo(&e);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "two"]);
    let first = git(&dir, &["rev-parse", "HEAD~1"]);
    let head = git(&dir, &["rev-parse", "HEAD"]);
    let id = started(&e, &dir);
    let t = e.ok_at(
        &dir,
        &[
            "done",
            &id,
            "--commit",
            &first[..8],
            "--commit",
            &first,
            "--commit",
            "HEAD",
        ],
    );
    let shas: Vec<&str> = t["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["sha"].as_str().unwrap())
        .collect();
    assert_eq!(shas, vec![head.as_str(), first.as_str()]);
    assert_eq!(t["commits"][0]["branch"], "main");
    assert_eq!(t["commits"][1]["branch"], Value::Null);
}

#[test]
fn bad_sha_is_a_usage_error_and_changes_nothing() {
    let e = Env::new();
    let dir = repo(&e);
    let id = started(&e, &dir);
    assert_eq!(e.code_at(&dir, &["done", &id, "--commit", "deadbeef"]), 2);
    assert_eq!(e.ok_at(&dir, &["show", &id])["status"], "active");
    assert_eq!(e.code_at(&dir, &["link", &id, "--bogus-flag-as-rev"]), 2);
    assert_eq!(e.code_at(e.path(), &["link", &id, "HEAD"]), 2);
}

#[test]
fn link_after_the_fact_and_detached_head() {
    let e = Env::new();
    let dir = repo(&e);
    let id = started(&e, &dir);
    e.ok_at(&dir, &["done", &id]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "late"]);
    git(&dir, &["checkout", "-q", "--detach"]);
    let late = git(&dir, &["rev-parse", "HEAD"]);
    let t = e.ok_at(&dir, &["link", &id, "HEAD"]);
    assert_eq!(t["commits"].as_array().unwrap().len(), 2);
    assert_eq!(t["commits"][1]["sha"], late.as_str());
    assert_eq!(t["commits"][1]["branch"], Value::Null);
    // Linking again changes nothing.
    let t = e.ok_at(&dir, &["link", &id, &late]);
    assert_eq!(t["commits"].as_array().unwrap().len(), 2);
}

#[test]
fn show_text_lists_commits() {
    let e = Env::new();
    let dir = repo(&e);
    let id = started(&e, &dir);
    let head = git(&dir, &["rev-parse", "--short=7", "HEAD"]);
    e.ok_at(&dir, &["done", &id]);
    let out = e.run_env_at(&dir, &[("SCREE_FORMAT", "text")], &["show", &id]);
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(text.contains("\nCommits:\n"), "{text}");
    assert!(text.contains(&format!("{head}  main  ")), "{text}");
}
