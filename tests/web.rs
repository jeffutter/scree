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

const ASSET_PATHS: [(&str, &str); 5] = [
    ("/assets/tokens.css", "text/css; charset=utf-8"),
    ("/assets/app.css", "text/css; charset=utf-8"),
    ("/assets/datastar.js", "text/javascript; charset=utf-8"),
    ("/assets/shell.js", "text/javascript; charset=utf-8"),
    ("/assets/errors.js", "text/javascript; charset=utf-8"),
];

#[test]
fn assets_serve_with_etag_and_304() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    for (path, content_type) in ASSET_PATHS {
        let r = w.get(path);
        assert_eq!(r.status, 200, "{path}");
        assert_eq!(r.header("content-type"), Some(content_type), "{path}");
        assert_eq!(r.header("cache-control"), Some("no-cache"));
        assert_eq!(r.header("x-content-type-options"), Some("nosniff"));
        assert!(!r.body.is_empty());
        let tag = r.header("etag").expect("etag").to_string();

        let same = w.request("GET", path, &[("If-None-Match", &tag)]);
        assert_eq!(same.status, 304, "{path}");
        assert_eq!(same.header("etag"), Some(tag.as_str()));
        assert!(same.body.is_empty());

        let weak = w.request("GET", path, &[("If-None-Match", &format!("W/{tag}"))]);
        assert_eq!(weak.status, 304, "{path} weak");
        let list = w.request("GET", path, &[("If-None-Match", &format!("\"x\", {tag}"))]);
        assert_eq!(list.status, 304, "{path} list");
        let star = w.request("GET", path, &[("If-None-Match", "*")]);
        assert_eq!(star.status, 304, "{path} star");

        let stale = w.request("GET", path, &[("If-None-Match", "\"stale\"")]);
        assert_eq!(stale.status, 200, "{path} stale");
        assert_eq!(stale.body, r.body);
    }
    assert_eq!(w.get("/assets/nope.css").status, 404);
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn datastar_bundle_matches_licence_hash() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/web/assets");
    let notice = std::fs::read_to_string(dir.join("DATASTAR-LICENSE.md")).unwrap();
    let expected = notice
        .lines()
        .find_map(|l| l.strip_prefix("sha256:"))
        .expect("a sha256: line in DATASTAR-LICENSE.md")
        .trim();
    let bundle = std::fs::read(dir.join("datastar.js")).unwrap();
    assert_eq!(sha256_hex(&bundle), expected);

    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    assert_eq!(w.get("/assets/datastar.js").body.as_bytes(), bundle);
}

/// Asserts a rendered page has no inline style or script (the CSP forbids both).
fn assert_no_inline(page: &str, body: &str) {
    assert!(!body.contains("<style"), "{page}: <style> element");
    assert!(!body.contains("style="), "{page}: style attribute");
    for (i, _) in body.match_indices("<script") {
        let tag = &body[i..i + body[i..].find('>').unwrap()];
        assert!(tag.contains("src="), "{page}: inline <script>");
    }
}

#[test]
fn no_inline_style_or_script_in_pages() {
    let e = Env::new();
    let ids = make_tickets(&e, 1);
    e.ok(&["decision", "add", &ids[0], "Populate the banner?"]);
    let w = e.spawn_web(&[], &[]);
    // Add each new rendered page here.
    let pages = ["/"];
    for page in pages {
        let r = w.get(page);
        assert_eq!(r.status, 200, "{page}");
        assert_no_inline(page, &r.body);
    }
    assert!(w.get("/").body.contains("/assets/app.css"));
    assert!(w.get("/").body.contains("class=\"banner\""));
    // The lint itself rejects what it should.
    for bad in [
        "<style>a{}</style>",
        "<p style=\"x\">",
        "<script>1</script>",
    ] {
        assert!(std::panic::catch_unwind(|| assert_no_inline("bad", bad)).is_err());
    }
}

/// Make `n` ready Tickets (unscoped, Project LOCAL) and return their ids.
fn make_tickets(e: &Env, n: usize) -> Vec<String> {
    (0..n)
        .map(|i| {
            let t = e.ok(&["add", &format!("T{i}"), "--status", "ready"]);
            t["id"].as_str().unwrap().to_string()
        })
        .collect()
}

/// The numbers the library reports for the banner: (open, distinct blocked).
fn library_counts(e: &Env) -> (usize, usize) {
    let c = conn(e);
    let q = scree::decision::open_queue_in(&c, &scree::ticket::ScopeFilter::All).unwrap();
    let mut all: Vec<&String> = q.decisions.iter().flat_map(|d| &d.tickets).collect();
    all.sort();
    all.dedup();
    (q.decisions.len(), all.len())
}

#[test]
fn banner_hidden_without_decisions() {
    let e = Env::new();
    make_tickets(&e, 1);
    let w = e.spawn_web(&[], &[]);
    assert!(!w.get("/").body.contains("class=\"banner\""));
}

#[test]
fn banner_counts_match_the_library() {
    let e = Env::new();
    let ids = make_tickets(&e, 3);
    // D-1 blocks T0 and T1, D-2 blocks T1 and T2: three distinct Tickets, not four.
    e.ok(&["decision", "add", &ids[0], "<script>alert(1)</script> one?"]);
    e.ok(&["decision", "link", "D-1", &ids[1]]);
    e.ok(&["decision", "add", &ids[1], "Second?"]);
    e.ok(&["decision", "link", "D-2", &ids[2]]);
    assert_eq!(library_counts(&e), (2, 3));

    let w = e.spawn_web(&[], &[]);
    let body = w.get("/").body;
    assert!(body.contains("href=\"/decisions\""), "{body}");
    assert!(
        body.contains("2 open Decisions</span> block 3 Tickets."),
        "{body}"
    );
    // Counts only: the question text never reaches the banner.
    assert!(!body.contains("alert(1)"), "{body}");

    // A finished Ticket stops counting, through the page as in the library.
    e.ok(&["abandon", &ids[2], "not needed"]);
    let (open, blocked) = library_counts(&e);
    assert_eq!((open, blocked), (2, 2));
    let body = w.get("/").body;
    assert!(
        body.contains("2 open Decisions</span> block 2 Tickets."),
        "{body}"
    );
}

#[test]
fn nav_disables_unbuilt_screens_and_marks_the_current_one() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let body = w.get("/").body;
    assert!(body.contains("href=\"/\" aria-current=\"page\""), "{body}");
    for path in ["/board", "/decisions", "/ledger", "/graph"] {
        assert!(
            !body.contains(&format!("<a class=\"nav-item\" href=\"{path}\"")),
            "{path}"
        );
    }
    assert_eq!(body.matches("aria-disabled=\"true\"").count(), 4, "{body}");
}

#[test]
fn help_overlay_lists_exactly_the_defined_keys() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let body = w.get("/").body;
    let overlay = &body[body.find("id=\"help\"").unwrap()..];
    let overlay = &overlay[..overlay.find("</table>").unwrap()];
    let keys: Vec<&str> = overlay
        .split("<kbd>")
        .skip(1)
        .map(|s| s.split("</kbd>").next().unwrap())
        .collect();
    assert_eq!(keys, ["?", "Esc"]);
    assert!(body.contains("/assets/shell.js"));
}

#[test]
fn database_error_renders_an_error_page_with_500() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    // Break the schema under the running server.
    e.exec_batch("DROP TABLE decision_tickets");
    let r = w.get("/");
    assert_eq!(r.status, 500);
    assert!(r.body.contains("role=\"alert\""), "{}", r.body);
    assert!(r.body.contains("Could not read the database"));
}
