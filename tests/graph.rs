//! `scree graph`: scope, status filtering, formats and the diamond, through the real binary.

mod common;

use common::Env;

const FLAGS: &[&str] = &["--role", "agent"];

fn node_ids(v: &serde_json::Value) -> Vec<String> {
    v["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn done_tickets_are_hidden_unless_asked_for() {
    let e = Env::with_flags(FLAGS);
    e.stdout(&["add", "One", "--status", "ready"]);
    e.stdout(&["add", "Two", "--status", "ready", "--blocked-by", "LOCAL-1"]);
    e.stdout(&["start", "LOCAL-1"]);
    e.stdout(&["done", "LOCAL-1", "--force"]);
    assert_eq!(node_ids(&e.ok(&["graph", "--json"])), ["LOCAL-2"]);
    let g = e.ok(&["graph", "--json", "--status", "done", "--status", "ready"]);
    assert_eq!(node_ids(&g), ["LOCAL-1", "LOCAL-2"]);
}

#[test]
fn a_blocker_outside_the_filter_stays_as_an_external_node() {
    let e = Env::with_flags(FLAGS);
    e.stdout(&["add", "One", "--status", "draft"]);
    e.stdout(&["add", "Two", "--status", "ready", "--blocked-by", "LOCAL-1"]);
    let g = e.ok(&["graph", "--json", "--status", "ready"]);
    assert_eq!(node_ids(&g), ["LOCAL-2", "LOCAL-1"]);
    assert_eq!(g["nodes"][1]["external"], true);
    assert_eq!(g["edges"][0]["from"], "LOCAL-1");
}

#[test]
fn mermaid_conflicts_with_json_and_ignores_agent_json_default() {
    let e = Env::with_flags(FLAGS);
    e.stdout(&["add", "One", "--status", "ready"]);
    assert_eq!(e.code(&["graph", "--format", "mermaid", "--json"]), 2);
    let out = e.stdout(&["graph", "--format", "mermaid"]);
    assert!(out.starts_with("flowchart TD"), "{out}");
}

#[test]
fn diamond_prints_the_shared_blocker_once_in_full() {
    let e = Env::with_flags(FLAGS);
    e.stdout(&["add", "A", "--status", "ready"]);
    e.stdout(&["add", "B", "--status", "ready", "--blocked-by", "LOCAL-1"]);
    e.stdout(&["add", "C", "--status", "ready", "--blocked-by", "LOCAL-1"]);
    e.stdout(&[
        "add",
        "D",
        "--status",
        "ready",
        "--blocked-by",
        "LOCAL-2",
        "--blocked-by",
        "LOCAL-3",
    ]);
    let out = e.stdout(&["graph", "--text", "--ascii"]);
    assert_eq!(out.matches("(see above)").count(), 1, "{out}");
    assert!(
        out.contains("critical path: LOCAL-1 -> LOCAL-2 -> LOCAL-4"),
        "{out}"
    );
    assert!(out.contains("top blockers: LOCAL-1 (blocks 3)"), "{out}");
}
