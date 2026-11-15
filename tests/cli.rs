//! Output formats, exit codes and how scree reads its environment. Every
//! command runs in a throwaway directory through the shared harness.

mod common;

use std::path::PathBuf;

use common::{Env, scree_cmd};
use serde_json::Value;

fn assert_json_list(out: &std::process::Output) {
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v, serde_json::json!({"tickets": []}));
}

#[test]
fn list_on_fresh_db() {
    let e = Env::new();
    // A database one level down: scree must create the parent directory.
    let db: PathBuf = e.path().join("sub").join("scree.db");
    let out = scree_cmd(&db, e.path()).arg("list").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v, serde_json::json!({"tickets": []}));
    assert!(db.exists());
}

#[test]
fn newer_schema_exits_6() {
    let e = Env::new();
    let db = e.db();
    assert_json_list(&e.run(&["list"]));
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("PRAGMA user_version = 99")
        .unwrap();
    let out = e.run(&["list"]);
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    let v: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(v["error"]["code"], "database");
}

#[test]
fn json_and_text_conflict_exits_2() {
    let e = Env::new();
    let out = e.run(&["--json", "--text", "list"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

#[test]
fn bare_off_tty_prints_help_exit_2() {
    let e = Env::new();
    let out = e.run(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Usage"));
}

#[test]
fn text_flag_prints_plain_text() {
    let e = Env::new();
    let out = e.run(&["--text", "list"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&out.stdout).contains('\x1b'));
    assert!(serde_json::from_slice::<Value>(&out.stdout).is_err());
}

#[test]
fn bad_role_exits_2() {
    let e = Env::new();
    assert_eq!(e.code(&["--role", "robot", "list"]), 2);
}

#[test]
fn explicit_role_human_off_tty_stays_json() {
    let e = Env::new();
    let out = e.run(&["--role", "human", "list"]);
    assert_eq!(out.status.code(), Some(0));
    serde_json::from_slice::<Value>(&out.stdout).unwrap();
}

#[test]
fn non_utf8_env_does_not_panic() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let e = Env::new();
    let mut cmd = e.cmd(&["list"]);
    cmd.env("JUNK", OsStr::from_bytes(b"\xff\xfe"));
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn empty_scree_role_is_unset() {
    let e = Env::new();
    assert_json_list(&e.run_empty("SCREE_ROLE", &["list"]));
}

#[test]
fn bare_with_bad_format_prints_help() {
    let e = Env::new();
    let mut cmd = e.cmd(&[]);
    cmd.env("SCREE_FORMAT", "yaml");
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Usage"));
}

#[test]
fn empty_scree_db_falls_back_to_xdg() {
    let e = Env::new();
    assert_json_list(&e.run_empty("SCREE_DB", &["list"]));
    assert!(
        e.path().join("scree").join("scree.db").exists(),
        "the XDG fallback created no database"
    );
}

#[test]
fn empty_scree_role_resolves_as_unset() {
    // Unset role off a tty is the agent/json default.
    let e = Env::new();
    assert_json_list(&e.run_empty("SCREE_ROLE", &["list"]));
}

#[test]
fn empty_scree_format_resolves_as_unset() {
    let e = Env::new();
    assert_json_list(&e.run_empty("SCREE_FORMAT", &["list"]));
}

#[test]
fn empty_scree_author_resolves_as_unset() {
    // Unset author defaults to the role name; empty must not become "".
    let e = Env::new();
    assert_json_list(&e.run_empty("SCREE_AUTHOR", &["list"]));
}

#[test]
fn empty_scree_session_resolves_as_unset() {
    let e = Env::new();
    assert_json_list(&e.run_empty("SCREE_SESSION", &["list"]));
}

#[test]
fn tui_without_a_tty_exits_2_and_leaves_terminal_and_db_alone() {
    let e = Env::new();
    let db = e.db();
    let out = e.run(&["tui"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("scree tui needs a terminal"));
    assert!(out.stdout.is_empty(), "no escape sequences on stdout");
    assert!(!db.exists(), "must not open the database");
}

#[test]
fn done_since_applies_before_limit() {
    let e = Env::new();
    for t in ["one", "two", "three"] {
        assert_eq!(e.code(&["add", t]), 0);
    }
    e.exec_batch(
        "UPDATE tickets SET status = 'done', done_at = '2020-01-01T00:00:00Z' WHERE number IN (1, 2);
         UPDATE tickets SET status = 'done', done_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE number = 3;",
    );
    let ids = |extra: &[&str]| -> Vec<String> {
        let mut args = vec!["list", "--status", "done", "--limit", "2"];
        args.extend(extra);
        let out = e.run(&args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        v["tickets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap().to_string())
            .collect()
    };
    let plain = ids(&[]);
    assert_eq!(plain.len(), 2);
    let recent = ids(&["--done-since", "7d"]);
    assert_eq!(recent.len(), 1);
    assert!(
        !plain.contains(&recent[0]),
        "recent ticket sits past the limit by position"
    );
    assert_eq!(ids(&["--done-since", "2021-01-01T00:00:00Z"]), recent);
    let bad = e.run(&["list", "--done-since", "soon"]);
    assert_eq!(bad.status.code(), Some(2));
}
