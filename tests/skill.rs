use std::process::{Command, Output};

fn scree(db: &std::path::Path, args: &[&str]) -> Output {
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

#[test]
fn skill_prints_frontmatter_and_matching_hash_without_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("scree.db");
    let out = scree(&db, &["skill"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!db.exists(), "skill must not create the database");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("---\nname: scree\ndescription: "));
    let (front, body) = text
        .strip_prefix("---\n")
        .unwrap()
        .split_once("\n---\n")
        .unwrap();
    assert!(front.lines().any(|l| l.starts_with("scree_version: ")));
    assert!(
        front
            .lines()
            .any(|l| l.starts_with("hash: ") && l.len() == 6 + 64)
    );
    assert!(body.starts_with("# scree"));
}

#[test]
fn check_exits_0_when_identical_and_1_when_missing_or_edited() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("scree.db");
    let file = dir.path().join("SKILL.md");
    let f = file.to_str().unwrap();

    let out = scree(&db, &["skill", "--check", f]);
    assert_eq!(out.status.code(), Some(1), "missing file is stale");

    std::fs::write(&file, scree(&db, &["skill"]).stdout).unwrap();
    assert_eq!(scree(&db, &["skill", "--check", f]).status.code(), Some(0));

    let mut edited = std::fs::read(&file).unwrap();
    edited.extend_from_slice(b"x");
    std::fs::write(&file, &edited).unwrap();
    assert_eq!(scree(&db, &["skill", "--check", f]).status.code(), Some(1));

    // A hand edit that keeps the length is caught too.
    let mut same_len = scree(&db, &["skill"]).stdout;
    let i = same_len.len() - 3;
    same_len[i] ^= 1;
    std::fs::write(&file, same_len).unwrap();
    assert_eq!(scree(&db, &["skill", "--check", f]).status.code(), Some(1));
    assert!(!db.exists());
}
