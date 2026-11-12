use std::process::{Command, Output};

fn scree(db: &std::path::Path) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_scree"));
    for (k, _) in std::env::vars() {
        if k.starts_with("SCREE_") || k.starts_with("GIT_") || k == "CLAUDECODE" || k == "AI_AGENT"
        {
            cmd.env_remove(k);
        }
    }
    cmd.arg("--db").arg(db).arg("list").output().unwrap()
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
