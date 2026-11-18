//! Stale claims: the last-touched timestamp, `list --stale` and `start --reclaim`.

mod common;

use common::Env;

/// Push a ticket's claim `hours` into the past.
fn backdate(e: &Env, id: &str, hours: u32) {
    let conn = rusqlite::Connection::open(e.db()).unwrap();
    let n = conn
        .execute(
            "UPDATE tickets SET claim_touched_at =
                strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1)
             WHERE status = 'active' AND 'LOCAL-' || number = ?2",
            rusqlite::params![format!("-{hours} hours"), id],
        )
        .unwrap();
    assert_eq!(n, 1);
}

fn claimed(e: &Env) -> String {
    let id = e.ok(&["--role", "human", "add", "t", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let t = e.ok(&["start", &id, "--author", "old", "--session", "s1"]);
    assert!(t["claim_touched_at"].is_string());
    id
}

fn stale_ids(e: &Env, extra: &[&str]) -> Vec<String> {
    let mut a = vec!["list", "--stale"];
    a.extend_from_slice(extra);
    e.ok(&a)["tickets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn list_stale_uses_default_and_older_than() {
    let e = Env::new();
    let id = claimed(&e);
    assert!(stale_ids(&e, &[]).is_empty());
    backdate(&e, &id, 5);
    assert_eq!(stale_ids(&e, &[]), std::slice::from_ref(&id));
    assert!(stale_ids(&e, &["--older-than", "6h"]).is_empty());
    assert_eq!(
        stale_ids(&e, &["--older-than", "90m"]),
        std::slice::from_ref(&id)
    );
    assert_eq!(e.code(&["list", "--stale", "--older-than", "soon"]), 2);
    assert_eq!(e.code(&["list", "--older-than", "1h"]), 2);
}

#[test]
fn comment_check_and_edit_refresh_the_claim() {
    let e = Env::new();
    let id = e.ok(&[
        "--role",
        "human",
        "add",
        "t",
        "--status",
        "ready",
        "--criterion",
        "c",
    ])["id"]
        .as_str()
        .unwrap()
        .to_string();
    e.ok(&["start", &id, "--author", "old", "--session", "s1"]);
    let touched = || {
        e.ok(&["show", &id])["claim_touched_at"]
            .as_str()
            .unwrap()
            .to_string()
    };
    for step in 0..3 {
        backdate(&e, &id, 5);
        assert_eq!(stale_ids(&e, &[]).len(), 1);
        match step {
            0 => e.ok(&["comment", "add", &id, "still going"]),
            1 => e.ok(&["check", &format!("{id}#1"), "--evidence", "x"]),
            _ => e.ok(&["edit", &id, "--title", "renamed"]),
        };
        assert!(stale_ids(&e, &[]).is_empty(), "step {step}");
        assert!(!touched().is_empty());
    }
}

#[test]
fn reclaim_takes_a_stale_claim_and_names_the_holder() {
    let e = Env::new();
    let id = claimed(&e);
    backdate(&e, &id, 5);
    let t = e.ok(&[
        "start",
        &id,
        "--reclaim",
        "--author",
        "new",
        "--session",
        "s2",
    ]);
    assert_eq!(t["claimed_by"]["author"], "new");
    assert_eq!(t["claimed_by"]["session"], "s2");
    let last = t["comments"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["body"], "reclaimed stale claim from old/s1");
    // The claim is fresh again, so a second reclaim is refused.
    assert_eq!(
        e.code(&[
            "start",
            &id,
            "--reclaim",
            "--author",
            "third",
            "--session",
            "s3"
        ]),
        4
    );
}

#[test]
fn a_fresh_claim_is_never_reclaimable() {
    let e = Env::new();
    let id = claimed(&e);
    assert_eq!(
        e.code(&[
            "start",
            &id,
            "--reclaim",
            "--author",
            "new",
            "--session",
            "s2"
        ]),
        4
    );
    let t = e.ok(&["show", &id]);
    assert_eq!(t["claimed_by"]["author"], "old");
    // Plain start on an active ticket stays a conflict.
    assert_eq!(e.code(&["start", &id]), 5);
}

#[test]
fn the_previous_holder_loses_the_claim() {
    let e = Env::new();
    let id = claimed(&e);
    backdate(&e, &id, 5);
    e.ok(&[
        "start",
        &id,
        "--reclaim",
        "--author",
        "new",
        "--session",
        "s2",
    ]);
    assert_eq!(
        e.code(&["done", &id, "--force", "--author", "old", "--session", "s1"]),
        5
    );
    assert_eq!(
        e.code(&["release", &id, "--author", "old", "--session", "s1"]),
        5
    );
    // The new holder is unaffected.
    assert_eq!(
        e.ok(&["release", &id, "--author", "new", "--session", "s2"])["status"],
        "ready"
    );
}

#[test]
fn reclaim_flag_combinations_are_rejected() {
    let e = Env::new();
    assert_eq!(e.code(&["start", "--next", "--reclaim"]), 2);
    assert_eq!(e.code(&["start", "LOCAL-1", "--older-than", "1h"]), 2);
}

#[test]
fn reorder_does_not_refresh_the_claim() {
    let e = Env::new();
    let id = claimed(&e);
    let other = e.ok(&["--role", "human", "add", "o", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    backdate(&e, &id, 5);
    let before = e.ok(&["show", &id])["claim_touched_at"].clone();
    e.ok(&["--role", "human", "reorder", &id, "--after", &other]);
    assert_eq!(stale_ids(&e, &[]), std::slice::from_ref(&id));
    assert_eq!(e.ok(&["show", &id])["claim_touched_at"], before);
}

#[test]
fn sessionless_reclaim_still_locks_out_the_previous_holder() {
    let e = Env::new();
    let id = claimed(&e);
    backdate(&e, &id, 5);
    e.ok(&["--role", "human", "start", &id, "--reclaim"]);
    for cmd in ["done", "release"] {
        assert_eq!(
            e.code(&[cmd, &id, "--author", "old", "--session", "s1"]),
            5,
            "{cmd}"
        );
    }
}
