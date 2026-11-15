mod common;

use common::Env;

#[test]
fn skill_prints_frontmatter_and_matching_hash_without_a_database() {
    let e = Env::new();
    let out = e.run(&["skill"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!e.db().exists(), "skill must not create the database");
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
    let e = Env::new();
    let file = e.path().join("SKILL.md");
    let f = file.to_str().unwrap();

    let out = e.run(&["skill", "--check", f]);
    assert_eq!(out.status.code(), Some(1), "missing file is stale");

    std::fs::write(&file, e.run(&["skill"]).stdout).unwrap();
    assert_eq!(e.run(&["skill", "--check", f]).status.code(), Some(0));

    let mut edited = std::fs::read(&file).unwrap();
    edited.extend_from_slice(b"x");
    std::fs::write(&file, &edited).unwrap();
    assert_eq!(e.run(&["skill", "--check", f]).status.code(), Some(1));

    // A hand edit that keeps the length is caught too.
    let mut same_len = e.run(&["skill"]).stdout;
    let i = same_len.len() - 3;
    same_len[i] ^= 1;
    std::fs::write(&file, same_len).unwrap();
    assert_eq!(e.run(&["skill", "--check", f]).status.code(), Some(1));
    assert!(!e.db().exists());
}
