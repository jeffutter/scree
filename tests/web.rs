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

const ADR_CSP: &str = "default-src 'none'; script-src 'self' 'unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

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
    let ids2 = e.ok(&["add", "Detail page"]);
    let detail = format!("/ticket/{}", ids2["id"].as_str().unwrap());
    let pages = ["/", detail.as_str()];
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
    assert_eq!(keys, ["?", "Esc", "j", "k", "Enter", "/"]);
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

// ---- scope extractor and picker (SCR-80) ----

use scree::web::scope;

/// Percent-encode `s` for a query value.
fn enc(s: &str) -> String {
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// The `name` select of the page: (is disabled, [(value, selected)]).
fn select(body: &str, name: &str) -> (bool, Vec<(String, bool)>) {
    let start = body
        .find(&format!("<select name=\"{name}\""))
        .unwrap_or_else(|| panic!("no select {name}: {body}"));
    let rest = &body[start..];
    let head_end = rest.find('>').unwrap();
    let disabled = rest[..head_end].contains("disabled");
    let rest = &rest[..rest.find("</select>").unwrap()];
    let opts = rest
        .split("<option ")
        .skip(1)
        .map(|o| {
            let tag = &o[..o.find('>').unwrap()];
            let v = tag.strip_prefix("value=\"").unwrap();
            let v = &v[..v.find('"').unwrap()];
            (
                v.replace("&amp;", "&"),
                tag.split_whitespace().any(|t| t == "selected"),
            )
        })
        .collect();
    (disabled, opts)
}

fn selected(body: &str, name: &str) -> Option<String> {
    select(body, name)
        .1
        .into_iter()
        .find(|(_, s)| *s)
        .map(|(v, _)| v)
}

/// Two Projects (my-app with a Worktree, other) and an unscoped ticket.
fn scope_setup(e: &Env) -> (PathBuf, PathBuf) {
    let (r, wt) = setup(e);
    let other = repo(e, "other");
    e.ok_at(&other, &["add", "Other"]);
    e.ok(&["add", "Loose"]);
    (r, wt)
}

#[test]
fn scope_urls_select_the_matching_view() {
    let e = Env::new();
    let (r, wt) = scope_setup(&e);
    e.ok_at(&r, &["add", "Root one"]);
    let w = e.spawn_web(&[], &[]);
    let wt_path = wt.to_str().unwrap();

    for url in ["/", "/?p=@all", "/?p=%40all"] {
        let r = w.get(url);
        assert_eq!(r.status, 200, "{url}");
        assert_eq!(selected(&r.body, "p").as_deref(), Some("@all"), "{url}");
        assert!(select(&r.body, "w").0, "w disabled for all: {url}");
        assert!(r.body.contains("All Projects (4 open)"), "{}", r.body);
        assert!(r.body.contains("No Project (1 open)"));
        assert!(r.body.contains(">MA (2 open)") || r.body.contains(">MA"));
    }

    let r1 = w.get("/?p=MA");
    assert_eq!(r1.status, 200);
    assert_eq!(selected(&r1.body, "p").as_deref(), Some("MA"));
    assert_eq!(selected(&r1.body, "w").as_deref(), Some("@all"));
    assert!(!select(&r1.body, "w").0);
    assert!(r1.body.contains("Root checkout (1)"));

    let r2 = w.get(&format!("/?p=MA&w={}", enc(wt_path)));
    assert_eq!(r2.status, 200);
    assert_eq!(selected(&r2.body, "w").as_deref(), Some(wt_path));

    let r3 = w.get("/?p=MA&w=@root");
    assert_eq!(r3.status, 200);
    assert_eq!(selected(&r3.body, "w").as_deref(), Some("@root"));

    let r4 = w.get("/?p=");
    assert_eq!(r4.status, 200);
    assert_eq!(selected(&r4.body, "p").as_deref(), Some(""));
    assert!(select(&r4.body, "w").0);
}

#[test]
fn unknown_or_malformed_scope_is_404_with_an_inline_fragment() {
    let e = Env::new();
    let (_, wt) = scope_setup(&e);
    let w = e.spawn_web(&[], &[]);
    let missing = enc("/no/such/worktree");
    let urls = [
        "/?p=NOPE".to_string(),
        format!("/?p=MA&w={missing}"),
        "/?w=@root".to_string(),
        "/?p=&w=@root".to_string(),
        "/?p=LOCAL".to_string(),
        "/?p=MA&p=OTHER".to_string(),
        // A Worktree of another Project is not a Worktree of this one.
        format!("/?p=OT&w={}", enc(wt.to_str().unwrap())),
    ];
    for url in urls {
        let r = w.get(&url);
        assert_eq!(r.status, 404, "{url}: {}", r.body);
        assert!(r.body.contains("id=\"err-scope\""), "{url}: {}", r.body);
        assert!(r.body.contains("role=\"alert\""));
    }
}

#[test]
fn same_url_renders_the_same_selection_and_round_trips() {
    let e = Env::new();
    let (_, wt) = scope_setup(&e);
    let w = e.spawn_web(&[], &[]);
    let wt_path = wt.to_str().unwrap();
    let filters = [
        scree::ticket::ScopeFilter::All,
        scree::ticket::ScopeFilter::Project {
            key: "LOCAL".into(),
        },
        scree::ticket::ScopeFilter::Project { key: "MA".into() },
        scree::ticket::ScopeFilter::Worktree {
            key: "MA".into(),
            path: None,
        },
        scree::ticket::ScopeFilter::Worktree {
            key: "MA".into(),
            path: Some(wt_path.to_string()),
        },
    ];
    for f in filters {
        let q = scope::query(&f);
        let url = if q.is_empty() {
            "/".into()
        } else {
            format!("/?{q}")
        };
        let first = w.get(&url);
        assert_eq!(first.status, 200, "{url}");
        // Parse the selections back out of the page: they name the same scope.
        let p = selected(&first.body, "p");
        let wv = if select(&first.body, "w").0 {
            None
        } else {
            selected(&first.body, "w")
        };
        // The page always sends p and an enabled w; map the defaults back.
        let wv = wv.filter(|v| v != "@all");
        let p = p.filter(|v| v != "@all");
        assert_eq!(
            scope::parse(p.as_deref(), wv.as_deref()).unwrap(),
            f,
            "{url}"
        );
        // Reload: identical selection.
        let again = w.get(&url);
        assert_eq!(selected(&again.body, "p"), selected(&first.body, "p"));
        assert_eq!(selected(&again.body, "w"), selected(&first.body, "w"));
    }
}

#[test]
fn picker_never_reads_the_server_cwd() {
    let e = Env::new();
    scope_setup(&e);
    let elsewhere = repo(&e, "unrelated");
    e.ok_at(&elsewhere, &["add", "Elsewhere"]);
    let from_tmp = e.spawn_web(&[], &[]);
    let from_repo = e.spawn_web_at(&elsewhere, &[], &[]);
    for url in ["/", "/?p=MA", "/?p=MA&w=@root", "/?p="] {
        let a = from_tmp.get(url);
        let b = from_repo.get(url);
        assert_eq!(a.status, b.status, "{url}");
        assert_eq!(a.body, b.body, "{url}");
    }
}

// ---- List screen (SCR-81) ----

fn rows(body: &str) -> usize {
    body.matches("data-id=\"").count()
}

/// Add `n` draft tickets titled `Bulk i` to the unscoped Project, in one
/// process (a CLI call per ticket would take minutes).
fn bulk(e: &Env, n: usize) {
    let mut c = conn(e);
    for i in 0..n {
        let title = format!("Bulk {i}");
        scree::ticket::add(&mut c, &scree::scope::Scope::Unscoped, &new(&title)).unwrap();
    }
}

#[test]
fn list_filters_narrow_the_rows_and_bad_values_are_400() {
    let e = Env::new();
    e.ok(&["add", "Alpha", "--status", "ready"]);
    e.ok(&["add", "Beta", "--status", "ready", "--for", "human"]);
    let done = e.ok(&["add", "Gamma", "--status", "ready"]);
    e.ok(&["start", done["id"].as_str().unwrap()]);
    e.ok(&["done", done["id"].as_str().unwrap()]);
    let w = e.spawn_web(&[], &[]);
    let body = w.get("/").body;
    assert_eq!(rows(&body), 2, "closed tickets are hidden by default");
    assert_eq!(rows(&w.get("/?for=human").body), 1);
    assert_eq!(rows(&w.get("/?status=done").body), 1);
    assert_eq!(rows(&w.get("/?status=done&status=ready").body), 3);
    assert_eq!(rows(&w.get("/?q=alp").body), 1);
    assert_eq!(rows(&w.get("/?available=1&for=human").body), 1);
    for bad in [
        "/?status=nope",
        "/?for=robot",
        "/?limit=0",
        "/?blocked=1&available=1",
        "/?older-than=4h",
        "/?blocked=maybe",
    ] {
        let r = w.get(bad);
        assert_eq!(r.status, 400, "{bad}");
        assert!(r.body.contains("id=\"err-filter\""), "{bad}: {}", r.body);
    }
    assert_eq!(w.get("/?older-than=4h&stale=1").status, 200);
}

#[test]
fn first_render_is_capped_at_200_of_2000() {
    let e = Env::new();
    bulk(&e, 2000);
    let w = e.spawn_web(&[], &[]);
    let r = w.get("/");
    assert_eq!(r.status, 200);
    assert_eq!(rows(&r.body), 200);
    assert!(r.body.contains("showing 200 of 2000"), "capped footer");
    assert!(r.body.contains("href=\"/?limit=400\""));
    let more = w.get("/?limit=400");
    assert_eq!(rows(&more.body), 400);
    assert!(more.body.contains("showing 400 of 2000"));
    let all = w.get("/?limit=2000");
    assert_eq!(rows(&all.body), 2000);
    assert!(!all.body.contains("showing "));
}

/// The region of the live stream's first frame for `view`, as HTML.
fn first_frame(w: &common::Web, view: &str) -> String {
    let mut s = w.stream(&format!("/stream?view={}", enc(view)));
    assert_eq!(s.status, 200);
    let ev = s.next_event(Duration::from_secs(3)).expect("first frame");
    ev.lines()
        .filter_map(|l| l.strip_prefix("data: elements "))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn filters_and_search_survive_reload_and_match_the_stream() {
    let e = Env::new();
    e.ok(&["add", "Find me", "--status", "ready"]);
    e.ok(&["add", "Other", "--status", "ready"]);
    let w = e.spawn_web(&[], &[]);
    let url = "/?status=ready&for=agent&q=find+me";
    let a = w.get(url);
    let b = w.get(url);
    assert_eq!(a.body, b.body);
    assert_eq!(rows(&a.body), 1);
    // The filter bar shows what the URL says.
    assert!(a.body.contains("value=\"find me\""), "{}", a.body);
    assert!(a.body.contains("<option value=\"agent\" selected>"));
    // The page's live view is this URL, and the stream's first frame is the
    // fragment the page already contains.
    let frame = first_frame(&w, "/?status=ready&for=agent&q=find+me");
    assert!(frame.starts_with("<div id=\"list-region\">"), "{frame}");
    assert!(a.body.contains(&frame), "stream and page disagree");
    assert!(
        a.body.contains(&format!(
            "@get('/stream?view={}')",
            enc("/?status=ready&for=agent&q=find+me")
        )),
        "{}",
        a.body
    );
}

#[test]
fn the_stream_view_keeps_the_scope() {
    let e = Env::new();
    scope_setup(&e);
    let w = e.spawn_web(&[], &[]);
    let page = w.get("/?p=MA&q=seed").body;
    assert!(
        page.contains(&format!("@get('/stream?view={}')", enc("/?p=MA&q=seed"))),
        "{page}"
    );
    let frame = first_frame(&w, "/?p=MA&q=seed");
    assert!(page.contains(&frame));
    assert_eq!(rows(&frame), 1);
}

#[test]
fn rows_group_by_project_only_when_the_scope_spans_projects() {
    let e = Env::new();
    scope_setup(&e);
    let w = e.spawn_web(&[], &[]);
    let all = w.get("/").body;
    assert!(all.contains("class=\"group-title\">MA<"), "{all}");
    assert!(all.contains("class=\"group-title\">LOCAL<"), "{all}");
    assert_eq!(all.matches("class=\"group\"").count(), 3);
    let one = w.get("/?p=MA").body;
    assert!(!one.contains("group-title"), "{one}");
    assert_eq!(rows(&one), 1);
}

#[test]
fn empty_scope_and_no_match_are_different_states() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let empty = w.get("/").body;
    assert!(empty.contains("No tickets here"), "{empty}");
    assert!(!empty.contains("No tickets match"));
    e.ok(&["add", "Something"]);
    let none = w.get("/?q=zzz").body;
    assert!(none.contains("No tickets match"), "{none}");
    assert!(none.contains("Clear filters"));
    assert!(!none.contains("No tickets here"));
    // A scope with only closed tickets has tickets: it is a no-match, not empty.
    let closed = e.ok(&["add", "Old", "--status", "ready"]);
    e.ok(&["start", closed["id"].as_str().unwrap()]);
    e.ok(&["done", closed["id"].as_str().unwrap()]);
    assert!(w.get("/?q=Old").body.contains("No tickets match"));
}

#[test]
fn hostile_titles_are_escaped() {
    let e = Env::new();
    for t in [
        "<script>alert(1)</script>",
        "\"><img src=x onerror=alert(1)>",
    ] {
        e.ok(&["add", t]);
    }
    let w = e.spawn_web(&[], &[]);
    for url in ["/", "/?q=%3Cscript%3E"] {
        let body = w.get(url).body;
        assert!(!body.contains("<script>alert"), "{url}");
        assert!(!body.contains("<img src=x"), "{url}");
        assert!(
            body.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
            "{url}"
        );
        assert_no_inline(url, &body);
    }
    // The search text is echoed into an attribute: it must be escaped too.
    let body = w.get("/?q=%22%3E%3Cimg+src%3Dx%3E").body;
    assert!(!body.contains("<img src=x>"), "{body}");
    let frame = first_frame(&w, "/");
    assert!(!frame.contains("<script>alert"));
}

#[test]
fn saved_links_return_the_expected_rows() {
    let e = Env::new();
    let a = e.ok(&["add", "Agent ready", "--status", "ready"]);
    e.ok(&["add", "Human ready", "--status", "ready", "--for", "human"]);
    e.ok(&["add", "Just a draft"]);
    e.ok(&["start", a["id"].as_str().unwrap()]);
    let w = e.spawn_web(&[], &[]);
    let page = w.get("/").body;
    assert!(
        page.contains("href=\"/?available=1&amp;for=human\""),
        "{page}"
    );
    assert!(page.contains("href=\"/?stale=1\""));
    // Next for me: Available tickets for the web's role (human).
    let next = w.get("/?available=1&for=human").body;
    assert_eq!(rows(&next), 1);
    assert!(next.contains("Human ready"));
    // Fresh claims are not stale; a bound in the future makes the claim stale.
    assert_eq!(rows(&w.get("/?stale=1").body), 0);
    assert_eq!(
        rows(&w.get("/?stale=1&older-than=2099-01-01T00:00:00Z").body),
        1
    );
}

#[test]
fn a_removed_ticket_leaves_the_stream_and_the_page_ships_the_selection_hooks() {
    let e = Env::new();
    let keep = e.ok(&["add", "Keep"])["id"].as_str().unwrap().to_string();
    let gone = e.ok(&["add", "Gone"])["id"].as_str().unwrap().to_string();
    let w = e.spawn_web(&[], &[]);
    let mut s = w.stream(&format!("/stream?view={}", enc("/")));
    let first = s.next_event(Duration::from_secs(3)).unwrap();
    assert!(first.contains(&gone) && first.contains(&keep));
    e.ok(&["abandon", &gone, "no longer needed"]);
    let next = s
        .next_event(Duration::from_secs(3))
        .expect("patch after abandon");
    assert!(next.contains(&keep) && !next.contains(&gone), "{next}");
    // The selection state itself is client script (manual browser check in
    // docs/web-ui.md); the page must ship its element and the script hook.
    let page = w.get("/").body;
    assert!(page.contains("id=\"list-changed\" hidden"), "{page}");
    assert!(page.contains("This ticket changed or was removed"));
    let js = w.get("/assets/shell.js").body;
    assert!(js.contains("list-changed") && js.contains("MutationObserver"));
}

#[test]
fn bad_scope_or_filter_on_the_stream_is_an_error_fragment() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    assert_eq!(
        w.get(&format!("/stream?view={}", enc("/?p=NOPE"))).status,
        404
    );
    assert_eq!(
        w.get(&format!("/stream?view={}", enc("/?status=x"))).status,
        400
    );
}

mod markdown_hostile {
    use scree::web::view::markdown::render;

    fn out(src: &str) -> String {
        render(src).into_string()
    }

    #[test]
    fn markdown_script_and_img_are_inert() {
        let o = out(
            "<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\ntext <img src=x onerror=alert(1)> more",
        );
        assert!(!o.contains("<script"), "{o}");
        assert!(!o.contains("<img"), "{o}");
        assert!(!o.contains("onerror=alert(1)>"), "{o}");
        assert!(o.contains("&lt;script&gt;"), "{o}");
    }

    #[test]
    fn markdown_javascript_link_is_text() {
        let o = out("[x](javascript:alert(1)) and [y](JavaScript:alert(2))");
        assert!(!o.contains("href=\"javascript"), "{o}");
        assert!(!o.contains("<a"), "{o}");
        assert!(o.contains('x') && o.contains('y'));
    }

    #[test]
    fn markdown_data_on_click_is_text() {
        let o = out(
            "<button data-on-click=\"@post('/x')\">go</button>\n\ndata-on-click=\"@post('/x')\"",
        );
        assert!(!o.contains("<button"), "{o}");
        assert!(!o.contains("<button"), "{o}");
        assert!(o.contains("&lt;button data-on-click="), "{o}");
    }

    #[test]
    fn markdown_safe_links_keep_href() {
        let o = out("[a](http://x.test) [b](https://x.test) [c](/rel) [d](rel/path)");
        for h in ["http://x.test", "https://x.test", "/rel", "rel/path"] {
            assert!(o.contains(&format!("href=\"{h}\"")), "{h}: {o}");
        }
    }
}

// ---- Ticket detail (SCR-83) ----

/// Text of `<li ... id="comment-N">` entries in a page.
fn comment_count(body: &str) -> usize {
    body.matches("id=\"comment-").count()
}

#[test]
fn detail_page_matches_scree_show() {
    let e = Env::new();
    let blocker = e.ok(&["add", "Blocker one", "--status", "ready"]);
    let blocker = blocker["id"].as_str().unwrap().to_string();
    let t = e.ok(&[
        "add",
        "Rich ticket",
        "--status",
        "ready",
        "--description",
        "Describe **this**",
        "--plan",
        "Plan step one",
        "--criterion",
        "first criterion",
        "--criterion",
        "second criterion",
    ]);
    let id = t["id"].as_str().unwrap().to_string();
    e.ok(&["start", &id]);
    e.ok(&["check", &format!("{id}#1"), "--evidence", "ran the thing"]);
    let sub = e.ok(&["add", "A subtask", "--parent", &id, "--status", "ready"]);
    let sub = sub["id"].as_str().unwrap().to_string();
    e.ok(&["comment", "add", &id, "hello from a comment"]);
    e.ok(&["decision", "add", &id, "Which way should this go?"]);
    e.ok(&["dep", "add", &id, &blocker]);
    let show = e.ok(&["show", &id]);
    let w = e.spawn_web(&[], &[]);
    let r = w.get(&format!("/ticket/{id}"));
    assert_eq!(r.status, 200, "{}", r.body);
    assert_no_inline("ticket", &r.body);
    let b = r.body;
    assert!(b.contains("Rich ticket"));
    assert!(b.contains("Describe <strong>this</strong>"), "{b}");
    assert!(b.contains("Plan step one"));
    for c in show["criteria"].as_array().unwrap() {
        assert!(b.contains(c["text"].as_str().unwrap()));
    }
    assert!(b.contains("ran the thing"));
    assert!(b.contains("1 of 2 checked"));
    for c in show["comments"].as_array().unwrap() {
        assert!(b.contains(c["body"].as_str().unwrap()));
    }
    assert_eq!(
        comment_count(&b),
        show["comments"].as_array().unwrap().len()
    );
    assert!(b.contains("Blocked by") && b.contains("Blocker one"));
    assert!(b.contains("Subtasks") && b.contains("A subtask") && b.contains(&sub));
    assert!(b.contains("Which way should this go?"));
    assert!(b.contains("claimed by"));
    // The blocker page lists what it blocks.
    let r = w.get(&format!("/ticket/{blocker}"));
    assert!(r.body.contains("Blocks") && r.body.contains("Rich ticket"));
    // Rows on the List link to the detail.
    assert!(w.get("/").body.contains(&format!("href=\"/ticket/{id}\"")));
}

#[test]
fn unknown_ticket_is_404_with_the_gone_state() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    for id in ["LOCAL-999", "nonsense"] {
        let r = w.get(&format!("/ticket/{id}"));
        assert_eq!(r.status, 404, "{id}");
        assert!(r.body.contains("removed or never existed"), "{id}");
    }
}

#[test]
fn scale_ticket_renders_in_one_response() {
    let e = Env::new();
    let title = "very long title ".repeat(20) + &"x".repeat(300);
    let mut args = vec![
        "add".to_string(),
        title.clone(),
        "--status".into(),
        "ready".into(),
    ];
    for i in 0..30 {
        args.push("--criterion".into());
        args.push(format!("criterion number {i} {}", "w".repeat(80)));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let id = e.ok(&refs)["id"].as_str().unwrap().to_string();
    {
        let mut c = conn(&e);
        let tx = c.transaction().unwrap();
        for i in 0..120 {
            tx.execute(
                "INSERT INTO comments (ticket_id, author, body, created_at)
                 SELECT t.id, 'agent', ?1, '2026-01-01T00:00:00Z' FROM tickets t
                 JOIN projects p ON p.id = t.project_id WHERE p.key || '-' || t.label = ?2",
                rusqlite::params![format!("comment body {i}"), id],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let show = e.ok(&["show", &id]);
    let w = e.spawn_web(&[], &[]);
    let r = w.get(&format!("/ticket/{id}"));
    assert_eq!(r.status, 200);
    assert_no_inline("scale", &r.body);
    assert_eq!(comment_count(&r.body), 120);
    assert_eq!(show["comments"].as_array().unwrap().len(), 120);
    assert!(r.body.contains(&title));
    assert_eq!(r.body.matches("class=\"criterion\"").count(), 30);
    assert!(r.body.contains("0 of 30 checked"));
}

#[test]
fn hostile_ticket_text_is_inert_on_the_detail_page() {
    let e = Env::new();
    let evil = "<script>alert(1)</script> [x](javascript:alert(1)) <img src=x onerror=alert(1)>";
    let id = e.ok(&[
        "add",
        evil,
        "--description",
        evil,
        "--plan",
        evil,
        "--criterion",
        evil,
    ])["id"]
        .as_str()
        .unwrap()
        .to_string();
    e.ok(&["comment", "add", &id, evil]);
    let w = e.spawn_web(&[], &[]);
    let url = format!("/ticket/{id}");
    for body in [w.get(&url).body, first_frame(&w, &url)] {
        assert!(!body.contains("<script>alert"), "{body}");
        assert!(!body.contains("<img"), "{body}");
        assert!(!body.contains("href=\"javascript"), "{body}");
        assert!(body.contains("&lt;script&gt;"), "{body}");
    }
}

#[test]
fn a_ticket_removed_by_another_process_shows_the_gone_state_in_the_stream() {
    let e = Env::new();
    let id = e.ok(&["add", "Doomed"])["id"].as_str().unwrap().to_string();
    let w = e.spawn_web(&[], &[]);
    let mut s = w.stream(&format!("/stream?view={}", enc(&format!("/ticket/{id}"))));
    let first = s.next_event(Duration::from_secs(3)).unwrap();
    assert!(first.contains("Doomed"), "{first}");
    // The CLI never deletes tickets, so a direct SQL delete is the only way to
    // simulate another process removing one.
    e.exec_batch(
        "PRAGMA foreign_keys = ON;
         DELETE FROM tickets WHERE title = 'Doomed';",
    );
    let next = s.next_event(Duration::from_secs(3)).expect("gone frame");
    assert!(next.contains("removed or never existed"), "{next}");
}

#[test]
fn web_never_sweeps() {
    // No file under src/web may call into the sweep module.
    let needle = concat!("sweep", "::");
    let mut stack = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/web")];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                seen += 1;
                let text = std::fs::read_to_string(&p).unwrap();
                assert!(!text.contains(needle), "{} calls {needle}", p.display());
            }
        }
    }
    assert!(seen > 5, "walked {seen} files");
}

#[test]
fn web_pages_leave_a_removed_worktrees_tickets_for_the_cli_to_sweep() {
    let e = Env::new();
    let r = repo(&e, "my-app");
    let wt = worktree(&e, &r, "feat");
    e.ok_at(&r, &["add", "root ticket", "--status", "ready"]);
    let open = e.ok_at(&wt, &["add", "open one", "--status", "ready"]);
    let open = open["id"].as_str().unwrap().to_string();
    let closed = e.ok_at(&wt, &["add", "closed one", "--status", "ready"]);
    let closed = closed["id"].as_str().unwrap().to_string();
    e.ok_at(&wt, &["start", &closed]);
    e.ok_at(&wt, &["done", &closed]);
    git(&r, &["worktree", "remove", "--force", wt.to_str().unwrap()]);
    let state = |id: &str| -> (String, bool) {
        let (key, label) = id.split_once('-').unwrap();
        e.sql(
            &format!(
                "SELECT t.status, t.archived_at IS NOT NULL FROM tickets t
                 JOIN projects p ON p.id = t.project_id
                 WHERE p.key = '{key}' AND t.label = '{label}'"
            ),
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
    };
    let before = (state(&open), state(&closed));
    assert_eq!(before.0, ("ready".to_string(), false));
    assert_eq!(before.1, ("done".to_string(), false));

    let w = e.spawn_web(&[], &[]);
    for url in [
        "/".to_string(),
        "/?p=MA".to_string(),
        format!("/ticket/{open}"),
        format!("/ticket/{closed}"),
    ] {
        assert_eq!(w.get(&url).status, 200, "{url}");
    }
    assert_eq!((state(&open), state(&closed)), before, "web must not sweep");

    // A sweeping CLI command archives and abandons them per docs/data-model.md.
    e.ok_at(&r, &["list"]);
    assert_eq!(state(&closed), ("done".to_string(), true));
    assert_eq!(state(&open), ("abandoned".to_string(), true));
}
