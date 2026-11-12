use std::process::{Command, Output};

fn scree_args(db: &std::path::Path, args: &[&str]) -> Output {
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
    cmd.arg("--db").arg(db).args(args).output().unwrap()
}

fn scree(db: &std::path::Path) -> Output {
    scree_args(db, &["list"])
}

#[test]
fn list_on_fresh_db() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("sub").join("scree.db");
    let out = scree(&db);
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v, serde_json::json!({"tickets": []}));
    assert!(db.exists());
}

#[test]
fn newer_schema_exits_6() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("scree.db");
    assert_eq!(scree(&db).status.code(), Some(0));
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute_batch("PRAGMA user_version = 99")
        .unwrap();
    let out = scree(&db);
    assert_eq!(out.status.code(), Some(6));
    assert!(out.stdout.is_empty());
    let v: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(v["error"]["code"], "database");
}

#[test]
fn json_and_text_conflict_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = scree_args(&dir.path().join("s.db"), &["--json", "--text", "list"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
}

#[test]
fn bare_off_tty_prints_help_exit_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = scree_args(&dir.path().join("s.db"), &[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Usage"));
}

#[test]
fn text_flag_prints_plain_text() {
    let dir = tempfile::tempdir().unwrap();
    let out = scree_args(&dir.path().join("s.db"), &["--text", "list"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!String::from_utf8_lossy(&out.stdout).contains('\x1b'));
    assert!(serde_json::from_slice::<serde_json::Value>(&out.stdout).is_err());
}

#[test]
fn bad_role_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let out = scree_args(&dir.path().join("s.db"), &["--role", "robot", "list"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn explicit_role_human_off_tty_stays_json() {
    let dir = tempfile::tempdir().unwrap();
    let out = scree_args(&dir.path().join("s.db"), &["--role", "human", "list"]);
    assert_eq!(out.status.code(), Some(0));
    serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap();
}

#[test]
fn non_utf8_env_does_not_panic() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_scree"));
    cmd.env("JUNK", std::ffi::OsStr::from_bytes(b"\xff\xfe"));
    let out = cmd
        .arg("--db")
        .arg(dir.path().join("s.db"))
        .arg("list")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn empty_scree_role_is_unset() {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_scree"));
    cmd.env("SCREE_ROLE", "");
    let out = cmd
        .arg("--db")
        .arg(dir.path().join("s.db"))
        .arg("list")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn bare_with_bad_format_prints_help() {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_scree"));
    cmd.env("SCREE_FORMAT", "yaml").env_remove("CLAUDECODE");
    let out = cmd
        .arg("--db")
        .arg(dir.path().join("s.db"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stdout).contains("Usage"));
}

fn with_empty(var: &str, extra: &[(&str, &str)], args: &[&str]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_scree"));
    for (k, _) in std::env::vars() {
        if k.starts_with("SCREE_") || k == "CLAUDECODE" || k == "AI_AGENT" || k == "NO_COLOR" {
            cmd.env_remove(k);
        }
    }
    cmd.env(var, "");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    if var != "SCREE_DB" {
        cmd.arg("--db").arg(dir.path().join("s.db"));
    } else {
        cmd.env("XDG_DATA_HOME", dir.path());
    }
    let out = cmd.args(args).output().unwrap();
    if var == "SCREE_DB" {
        assert!(dir.path().join("scree").join("scree.db").exists());
    }
    out
}

fn assert_json_list(out: &Output) {
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v, serde_json::json!({"tickets": []}));
}

#[test]
fn empty_scree_db_falls_back_to_xdg() {
    assert_json_list(&with_empty("SCREE_DB", &[], &["list"]));
}

#[test]
fn empty_scree_role_resolves_as_unset() {
    // Unset role off a tty is the agent/json default.
    assert_json_list(&with_empty("SCREE_ROLE", &[], &["list"]));
}

#[test]
fn empty_scree_format_resolves_as_unset() {
    assert_json_list(&with_empty("SCREE_FORMAT", &[], &["list"]));
}

#[test]
fn empty_scree_author_resolves_as_unset() {
    // Unset author defaults to the role name; empty must not become "".
    assert_json_list(&with_empty("SCREE_AUTHOR", &[], &["list"]));
}

#[test]
fn empty_scree_session_resolves_as_unset() {
    assert_json_list(&with_empty("SCREE_SESSION", &[], &["list"]));
}
