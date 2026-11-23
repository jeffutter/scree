//! Library-level tests for the web UI's create-from-pick function.

mod common;

use std::path::{Path, PathBuf};

use common::{Env, git_cmd};
use scree::cli::{ForWho, NewStatus};
use scree::error::Error;
use scree::ticket::NewTicket;
use scree::web::create::create_in;

fn git(dir: &Path, args: &[&str]) {
    let out = git_cmd(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn repo(e: &Env, name: &str) -> PathBuf {
    let dir = e.path().join("a").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "init"]);
    dir
}

fn worktree(e: &Env, repo: &Path, name: &str) -> PathBuf {
    let wt = e.path().join("wts").join(name);
    std::fs::create_dir_all(wt.parent().unwrap()).unwrap();
    git(
        repo,
        &["worktree", "add", "-q", "-b", name, wt.to_str().unwrap()],
    );
    std::fs::canonicalize(wt).unwrap()
}

fn new(title: &str) -> NewTicket<'_> {
    NewTicket {
        title,
        description: "",
        plan: "",
        criteria: &[],
        for_who: ForWho::Agent,
        status: NewStatus::Draft,
        key: None,
        parent: None,
        blocked_by: &[],
    }
}

fn conn(e: &Env) -> rusqlite::Connection {
    scree::db::open(&e.db()).unwrap()
}

fn tickets(e: &Env) -> i64 {
    conn(e)
        .query_row("SELECT count(*) FROM tickets", [], |r| r.get(0))
        .unwrap()
}

fn projects(e: &Env) -> i64 {
    conn(e)
        .query_row("SELECT count(*) FROM projects", [], |r| r.get(0))
        .unwrap()
}

/// A Project `my-app` (key MA) with a seeded ticket in one Worktree.
fn setup(e: &Env) -> (PathBuf, PathBuf) {
    let r = repo(e, "my-app");
    let wt = worktree(e, &r, "feat");
    e.ok_at(&wt, &["add", "Seed"]);
    (r, wt)
}

#[test]
fn create_in_picked_worktree() {
    let e = Env::new();
    let (_, wt) = setup(&e);
    let mut c = conn(&e);
    let d = create_in(&mut c, "MA", Some(wt.to_str().unwrap()), &new("From web")).unwrap();
    let id = d.ticket.id.clone();
    // Read back through the CLI from the unrelated temp dir.
    let shown = e.ok(&["show", &id]);
    assert_eq!(shown["project"]["key"], "MA");
    assert_eq!(shown["worktree"], wt.to_str().unwrap());
}

#[test]
fn create_at_checkout_has_no_worktree() {
    let e = Env::new();
    let (r, _) = setup(&e);
    let mut c = conn(&e);
    let d = create_in(&mut c, "MA", None, &new("Root")).unwrap();
    let shown = e.ok(&["show", &d.ticket.id]);
    assert_eq!(shown["project"]["key"], "MA");
    assert_eq!(shown["worktree"], serde_json::Value::Null);
    assert_eq!(shown["project"]["root"], r.to_str().unwrap());
}

#[test]
fn removed_worktree_is_refused_naming_the_path() {
    let e = Env::new();
    let (_, wt) = setup(&e);
    std::fs::remove_dir_all(&wt).unwrap();
    let before = tickets(&e);
    let mut c = conn(&e);
    let err = create_in(&mut c, "MA", Some(wt.to_str().unwrap()), &new("X")).unwrap_err();
    match err {
        Error::Refused(m) => assert!(m.contains(wt.to_str().unwrap()), "{m}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(tickets(&e), before);
}

#[test]
fn removed_root_is_refused_for_checkout_create() {
    let e = Env::new();
    let (r, wt) = setup(&e);
    std::fs::remove_dir_all(&wt).unwrap();
    std::fs::remove_dir_all(&r).unwrap();
    let mut c = conn(&e);
    let err = create_in(&mut c, "MA", None, &new("X")).unwrap_err();
    assert!(matches!(&err, Error::Refused(m) if m.contains(r.to_str().unwrap())));
}

#[test]
fn unknown_project_or_worktree_is_not_found_and_creates_no_project() {
    let e = Env::new();
    let (_, _wt) = setup(&e);
    let (p0, t0) = (projects(&e), tickets(&e));
    let mut c = conn(&e);
    let err = create_in(&mut c, "NOPE", None, &new("X")).unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
    let stray = e.path().join("stray");
    std::fs::create_dir_all(&stray).unwrap();
    let err = create_in(&mut c, "MA", Some(stray.to_str().unwrap()), &new("X")).unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "{err:?}");
    assert_eq!((projects(&e), tickets(&e)), (p0, t0));
}

#[test]
fn key_and_parent_are_rejected() {
    let e = Env::new();
    setup(&e);
    let mut c = conn(&e);
    let mut t = new("X");
    t.key = Some("ZZ");
    assert!(matches!(
        create_in(&mut c, "MA", None, &t),
        Err(Error::Usage(_))
    ));
    let mut t = new("X");
    t.parent = Some("MA-1");
    assert!(matches!(
        create_in(&mut c, "MA", None, &t),
        Err(Error::Usage(_))
    ));
}

// ---- scree web server skeleton (SCR-76) ----

use std::time::Duration;

const ADR_CSP: &str = "default-src 'none'; script-src 'self' 'unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

#[test]
fn serves_placeholder_with_csp() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let r = w.get("/");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("Scree"));
    assert_eq!(r.header("content-security-policy"), Some(ADR_CSP));
}

#[test]
fn guard_rejects_hostile_requests() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let origin = format!("http://{}", w.addr);
    let get = |h: &[(&str, &str)]| w.request("GET", "/", h).status;
    assert_eq!(get(&[("Host", "evil.test")]), 403, "foreign Host");
    assert_eq!(
        get(&[("Origin", "http://evil.test")]),
        403,
        "foreign Origin"
    );
    assert_eq!(get(&[("Origin", "null")]), 403, "Origin null");
    assert_eq!(get(&[("Sec-Fetch-Site", "same-site")]), 403);
    assert_eq!(get(&[("Sec-Fetch-Site", "cross-site")]), 403);
    assert_eq!(get(&[("Sec-Fetch-Site", "same-origin")]), 200);
    assert_eq!(get(&[("Origin", &origin)]), 200);
    // A refusal still carries the CSP.
    let r = w.request("GET", "/", &[("Host", "evil.test")]);
    assert_eq!(r.header("content-security-policy"), Some(ADR_CSP));

    let no_origin = w.request("POST", "/", &[]);
    assert_eq!(no_origin.status, 403);
    assert!(no_origin.body.contains("Origin"));
    // A valid POST passes the guard; there is no POST route, so the router answers.
    let ok = w.request("POST", "/", &[("Origin", &origin)]);
    assert_ne!(ok.status, 403);
    assert!(!ok.body.contains("forbidden"));
}

#[test]
fn non_loopback_bind_is_refused() {
    let e = Env::new();
    for bind in ["0.0.0.0", "192.0.2.1"] {
        let out = e.run(&["web", "--port", "0", "--bind", bind]);
        assert_eq!(out.status.code(), Some(2), "{bind}: {out:?}");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!err.contains("listening on"), "{err}");
        assert!(err.contains("loopback"), "{err}");
    }
}

#[test]
fn port_clash_names_the_port() {
    let e = Env::new();
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port().to_string();
    let out = e.run(&["web", "--port", &port]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains(&port));
}

#[test]
fn role_is_human_even_when_the_environment_says_agent() {
    let e = Env::new();
    // The placeholder page prints the Role the server resolved, so the body
    // is the observable. The environment variable and the flag both say agent.
    let w = e.spawn_web(&[("SCREE_ROLE", "agent"), ("CLAUDECODE", "1")], &[]);
    assert!(w.get("/").body.contains("role: human"));
    let w2 = e.spawn_web(&[], &["--role", "agent"]);
    assert!(w2.get("/").body.contains("role: human"));
}

#[test]
fn sigint_exits_promptly() {
    let e = Env::new();
    let mut w = e.spawn_web(&[], &[]);
    assert!(w.interrupt(Duration::from_secs(1)));
}
