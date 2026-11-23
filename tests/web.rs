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
    let pages = ["/", detail.as_str(), "/decisions", "/decisions?d=LOCAL-D-1"];
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
    assert!(
        body.contains("<a class=\"nav-item\" href=\"/decisions\""),
        "{body}"
    );
    for path in ["/board", "/graph"] {
        assert!(
            !body.contains(&format!("<a class=\"nav-item\" href=\"{path}\"")),
            "{path}"
        );
    }
    assert_eq!(body.matches("aria-disabled=\"true\"").count(), 2, "{body}");
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

// ---- Ticket detail writes (POST /ticket/ID/...) ----

fn new_ticket(e: &Env, status: &str, criteria: &[&str]) -> String {
    let mut args = vec!["add", "Writes", "--status", status];
    for c in criteria {
        args.push("--criterion");
        args.push(c);
    }
    e.ok(&args)["id"].as_str().unwrap().to_string()
}

fn detail(e: &Env, id: &str) -> scree::model::TicketDetail {
    scree::ticket::show(&conn(e), id).unwrap()
}

fn db_status(e: &Env, id: &str) -> String {
    detail(e, id).ticket.status
}

fn archived(e: &Env, id: &str) -> bool {
    detail(e, id).ticket.archived_at.is_some()
}

/// (checked, total) criteria.
fn criteria_counts(e: &Env, id: &str) -> (usize, usize) {
    let d = detail(e, id);
    (
        d.criteria.iter().filter(|c| c.checked).count(),
        d.criteria.len(),
    )
}

/// A successful write answers 200 with the refreshed `ticket` region.
fn assert_region(r: &common::Reply, shows: &str) {
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body.starts_with("<div id=\"ticket\""), "{}", r.body);
    assert!(r.body.contains(shows), "{shows}: {}", r.body);
}

/// A refusal answers `status` with only the `err-CONTROL` fragment.
fn assert_err(r: &common::Reply, status: u16, control: &str) {
    assert_eq!(r.status, status, "{}", r.body);
    assert!(
        r.body.starts_with(&format!("<div id=\"err-{control}\"")),
        "{}",
        r.body
    );
    assert!(!r.body.contains("id=\"ticket\""), "{}", r.body);
}

#[test]
fn write_ready_and_draft() {
    let e = Env::new();
    let id = new_ticket(&e, "draft", &[]);
    let w = e.spawn_web(&[], &[]);
    // Refused (library Error::Refused): a draft cannot be drafted again.
    assert_err(&w.post(&format!("/ticket/{id}/draft")), 422, "draft");
    let r = w.post(&format!("/ticket/{id}/ready"));
    assert_region(&r, "status-ready");
    assert_eq!(db_status(&e, &id), "ready");
    // Refused: already ready.
    assert_err(&w.post(&format!("/ticket/{id}/ready")), 422, "ready");
    assert_region(&w.post(&format!("/ticket/{id}/draft")), "status-draft");
    assert_eq!(db_status(&e, &id), "draft");
}

#[test]
fn write_start_succeeds_and_a_lost_claim_race_is_409() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    let page = w.get(&format!("/ticket/{id}")).body;
    assert!(page.contains("/start"), "{page}");
    // Another caller claims between the page load and the POST: Conflict.
    e.ok(&["start", &id]);
    let r = w.post(&format!("/ticket/{id}/start"));
    assert_err(&r, 409, "start");
    assert_eq!(db_status(&e, &id), "active");
    assert_ne!(
        detail(&e, &id).ticket.claimed_by.unwrap().author,
        "web",
        "the other claim must survive"
    );
    let id2 = new_ticket(&e, "ready", &[]);
    let r = w.post(&format!("/ticket/{id2}/start"));
    assert_region(&r, "status-active");
    assert_eq!(db_status(&e, &id2), "active");
}

#[test]
fn write_start_of_a_missing_ticket_is_404() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    assert_err(&w.post("/ticket/LOCAL-99/start"), 404, "start");
}

#[test]
fn write_release_stores_the_handoff_and_refuses_a_ready_ticket() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    // Refused: only an active ticket is released.
    assert_err(
        &w.post_form(&format!("/ticket/{id}/release"), &[]),
        422,
        "release",
    );
    w.post(&format!("/ticket/{id}/start"));
    let r = w.post_form(
        &format!("/ticket/{id}/release"),
        &[("handoff", "left off at step 3")],
    );
    assert_region(&r, "status-ready");
    assert!(r.body.contains("left off at step 3"), "{}", r.body);
    assert_eq!(db_status(&e, &id), "ready");
    assert_eq!(detail(&e, &id).handoff, "left off at step 3");
}

#[test]
fn write_done_refuses_unchecked_criteria_and_force_succeeds() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &["one"]);
    let w = e.spawn_web(&[], &[]);
    // Refused: an unchecked criterion.
    let r = w.post_form(&format!("/ticket/{id}/done"), &[]);
    assert_err(&r, 422, "done");
    assert_eq!(db_status(&e, &id), "ready");
    let r = w.post_form(&format!("/ticket/{id}/done"), &[("force", "1")]);
    assert_region(&r, "status-done");
    assert_eq!(db_status(&e, &id), "done");
    // Refused: already closed.
    assert_err(
        &w.post_form(&format!("/ticket/{id}/done"), &[]),
        422,
        "done",
    );
}

#[test]
fn write_done_without_criteria_needs_no_force() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    assert_region(
        &w.post_form(&format!("/ticket/{id}/done"), &[]),
        "status-done",
    );
}

#[test]
fn write_abandon_needs_a_reason() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    // Usage: a blank reason reaches the library.
    for body in [&[][..], &[("reason", "   ")][..]] {
        let r = w.post_form(&format!("/ticket/{id}/abandon"), body);
        assert_err(&r, 400, "abandon");
        assert_eq!(db_status(&e, &id), "ready");
    }
    let r = w.post_form(
        &format!("/ticket/{id}/abandon"),
        &[("reason", "not needed")],
    );
    assert_region(&r, "status-abandoned");
    assert_eq!(db_status(&e, &id), "abandoned");
}

#[test]
fn write_reopen_and_its_refusal() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    // Refused: an open ticket cannot be reopened.
    assert_err(&w.post(&format!("/ticket/{id}/reopen")), 422, "reopen");
    w.post_form(&format!("/ticket/{id}/done"), &[]);
    assert_eq!(db_status(&e, &id), "done");
    let r = w.post(&format!("/ticket/{id}/reopen"));
    assert_region(&r, "status-ready");
    assert_eq!(db_status(&e, &id), "ready");
}

#[test]
fn write_archive_and_unarchive() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    w.post_form(&format!("/ticket/{id}/done"), &[]);
    assert_region(&w.post(&format!("/ticket/{id}/archive")), "archived");
    assert!(archived(&e, &id));
    assert_region(&w.post(&format!("/ticket/{id}/unarchive")), "status-done");
    assert!(!archived(&e, &id));
    assert_err(&w.post("/ticket/LOCAL-99/archive"), 404, "archive");
}

#[test]
fn write_link_commits_resolves_in_the_projects_checkout() {
    let e = Env::new();
    let r = repo(&e, "my-app");
    let id = e.ok_at(&r, &["add", "Linked", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sha = String::from_utf8(
        git_cmd(&r)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let w = e.spawn_web(&[], &[]);
    // Usage: not a commit.
    let bad = w.post_form(
        &format!("/ticket/{id}/commits"),
        &[("commits", "no-such-rev")],
    );
    assert_err(&bad, 400, "commits");
    // Usage: nothing given.
    assert_err(
        &w.post_form(&format!("/ticket/{id}/commits"), &[]),
        400,
        "commits",
    );
    let ok = w.post_form(&format!("/ticket/{id}/commits"), &[("commits", "HEAD\n\n")]);
    assert_region(&ok, &sha[..10]);
    let linked: String = conn(&e)
        .query_row("SELECT sha FROM ticket_commits LIMIT 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(linked, sha);
    // Done links its commits too.
    let id2 = e.ok_at(&r, &["add", "Done linked", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let done = w.post_form(&format!("/ticket/{id2}/done"), &[("commits", "HEAD")]);
    assert_region(&done, &sha[..10]);
}

#[test]
fn write_link_commits_refuses_an_unscoped_ticket() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    let r = w.post_form(&format!("/ticket/{id}/commits"), &[("commits", "HEAD")]);
    assert_err(&r, 422, "commits");
}

#[test]
fn write_criteria_check_uncheck_add_remove() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &["first"]);
    let w = e.spawn_web(&[], &[]);
    let r = w.post_form(
        &format!("/ticket/{id}/criteria/1/check"),
        &[("evidence", "ran the tests")],
    );
    assert_region(&r, "1 of 1 checked");
    assert!(r.body.contains("ran the tests"), "{}", r.body);
    assert_eq!(criteria_counts(&e, &id), (1, 1));
    let r = w.post(&format!("/ticket/{id}/criteria/1/uncheck"));
    assert_region(&r, "0 of 1 checked");
    assert_eq!(criteria_counts(&e, &id), (0, 1));
    // Usage: not a number; NotFound: no such criterion.
    assert_err(
        &w.post(&format!("/ticket/{id}/criteria/x/check")),
        400,
        "criterion-x",
    );
    assert_err(
        &w.post(&format!("/ticket/{id}/criteria/9/check")),
        404,
        "criterion-9",
    );
    // Usage: blank text.
    assert_err(
        &w.post_form(&format!("/ticket/{id}/criteria"), &[("text", " ")]),
        400,
        "criterion-add",
    );
    let r = w.post_form(&format!("/ticket/{id}/criteria"), &[("text", "second")]);
    assert_region(&r, "second");
    assert_eq!(criteria_counts(&e, &id), (0, 2));
    let r = w.post(&format!("/ticket/{id}/criteria/2/remove"));
    assert_region(&r, "0 of 1 checked");
    assert_eq!(criteria_counts(&e, &id), (0, 1));
    assert_err(
        &w.post(&format!("/ticket/{id}/criteria/2/remove")),
        404,
        "criterion-2",
    );
}

#[test]
fn write_comments_add_edit_delete() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    // Usage: blank body.
    assert_err(
        &w.post_form(&format!("/ticket/{id}/comments"), &[("body", "")]),
        400,
        "comment-add",
    );
    let r = w.post_form(
        &format!("/ticket/{id}/comments"),
        &[("body", "first words")],
    );
    assert_region(&r, "first words");
    assert_eq!(detail(&e, &id).comments.len(), 1);
    let cid: i64 = conn(&e)
        .query_row("SELECT id FROM comments", [], |r| r.get(0))
        .unwrap();
    let updated: String = conn(&e)
        .query_row("SELECT updated_at FROM tickets", [], |r| r.get(0))
        .unwrap();
    assert!(
        r.body.contains(&format!("value=\"{updated}\"")),
        "{}",
        r.body
    );
    let edit = format!("/ticket/{id}/comments/{cid}/edit");
    let r = w.post_form(
        &edit,
        &[("body", "second words"), ("expected_updated_at", &updated)],
    );
    assert_region(&r, "second words");
    // Conflict: the ticket changed since the page the edit came from.
    let stale = w.post_form(
        &edit,
        &[
            ("body", "late words"),
            ("expected_updated_at", "2000-01-01T00:00:00Z"),
        ],
    );
    assert_err(&stale, 409, &format!("comment-{cid}"));
    let body: String = conn(&e)
        .query_row("SELECT body FROM comments WHERE id = ?1", [cid], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(body, "second words");
    // NotFound: no such comment.
    assert_err(
        &w.post_form(&format!("/ticket/{id}/comments/999/edit"), &[("body", "x")]),
        404,
        "comment-999",
    );
    assert_region(
        &w.post(&format!("/ticket/{id}/comments/{cid}/delete")),
        "Comments (0)",
    );
    assert_eq!(detail(&e, &id).comments.len(), 0);
    assert_err(
        &w.post(&format!("/ticket/{id}/comments/{cid}/delete")),
        404,
        &format!("comment-{cid}"),
    );
}

#[test]
fn write_hostile_comment_renders_inert() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    let evil = "<script>alert(1)</script> <img src=x onerror=alert(1)> [x](javascript:alert(1))";
    let r = w.post_form(&format!("/ticket/{id}/comments"), &[("body", evil)]);
    assert_eq!(r.status, 200);
    assert!(!r.body.contains("<script"), "{}", r.body);
    assert!(!r.body.contains("<img"), "{}", r.body);
    assert!(!r.body.contains("href=\"javascript"), "{}", r.body);
    assert!(r.body.contains("&lt;script&gt;"), "{}", r.body);
}

#[test]
fn moves_that_are_not_offered_are_disabled_with_a_reason() {
    let e = Env::new();
    let draft = new_ticket(&e, "draft", &[]);
    let active = new_ticket(&e, "ready", &[]);
    e.ok(&["start", &active]);
    let w = e.spawn_web(&[], &[]);
    let page = w.get(&format!("/ticket/{draft}")).body;
    assert!(page.contains("a draft; make it ready first"), "{page}");
    assert!(
        page.contains("only an active ticket can be released"),
        "{page}"
    );
    assert!(page.contains("ticket is open"), "{page}");
    assert!(page.contains("disabled"), "{page}");
    assert!(!page.contains("/start'"), "{page}");
    let page = w.get(&format!("/ticket/{active}")).body;
    assert!(page.contains("claimed by"), "{page}");
    assert!(
        page.contains(&format!("/ticket/{active}/release")),
        "{page}"
    );
    for control in [
        "ready",
        "start",
        "release",
        "done",
        "abandon",
        "reopen",
        "archive",
        "commits",
        "criterion-add",
        "comment-add",
    ] {
        assert!(
            page.contains(&format!("id=\"err-{control}\""))
                || control == "ready"
                || control == "start"
                || control == "reopen",
            "{control}: {page}"
        );
    }
}

#[test]
fn writes_still_need_a_same_origin_post() {
    let e = Env::new();
    let id = new_ticket(&e, "ready", &[]);
    let w = e.spawn_web(&[], &[]);
    let r = w.request("POST", &format!("/ticket/{id}/start"), &[]);
    assert_eq!(r.status, 403, "{}", r.body);
    assert_eq!(db_status(&e, &id), "ready");
}

// ---- Ticket detail writes B: blockers, edits, previews, subtasks ----

fn add_titled(e: &Env, title: &str, status: &str) -> String {
    e.ok(&["add", title, "--status", status])["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn blocked_by(e: &Env, id: &str) -> Vec<String> {
    detail(e, id)
        .ticket
        .blocked_by
        .iter()
        .map(|b| b.id.clone())
        .collect()
}

#[test]
fn blockers_add_and_remove() {
    let e = Env::new();
    let a = add_titled(&e, "A", "ready");
    let b = add_titled(&e, "B", "ready");
    let w = e.spawn_web(&[], &[]);
    let r = w.post_form(&format!("/ticket/{a}/blockers"), &[("blocker", &b)]);
    assert_region(&r, "Blocked by");
    assert_eq!(blocked_by(&e, &a), vec![b.clone()]);
    assert!(r.body.contains(&format!("/ticket/{a}/blockers/{b}/remove")));
    // Adding the same edge again changes nothing.
    assert_region(
        &w.post_form(&format!("/ticket/{a}/blockers"), &[("blocker", &b)]),
        "Blocked by",
    );
    assert_eq!(blocked_by(&e, &a), vec![b.clone()]);
    // Usage: nothing picked.
    assert_err(
        &w.post_form(&format!("/ticket/{a}/blockers"), &[("blocker", "")]),
        400,
        "blocker-add",
    );
    let r = w.post(&format!("/ticket/{a}/blockers/{b}/remove"));
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(blocked_by(&e, &a).is_empty());
    // Removing an edge that is not there is a no-op.
    assert_eq!(
        w.post(&format!("/ticket/{a}/blockers/{b}/remove")).status,
        200
    );
}

#[test]
fn blockers_refuse_self_cycle_cross_project_and_unknown() {
    let e = Env::new();
    let a = add_titled(&e, "A", "ready");
    let b = add_titled(&e, "B", "ready");
    let other = repo(&e, "other");
    let foreign = e.ok_at(&other, &["add", "Foreign", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let w = e.spawn_web(&[], &[]);
    let add = |id: &str, blocker: &str| {
        w.post_form(&format!("/ticket/{id}/blockers"), &[("blocker", blocker)])
    };
    // Refused (library): a ticket cannot block itself.
    assert_err(&add(&a, &a), 422, "blocker-add");
    assert_region(&add(&a, &b), "Blocked by");
    // Refused (library): B blocked by A closes a cycle.
    let r = add(&b, &a);
    assert_err(&r, 422, "blocker-add");
    assert!(blocked_by(&e, &b).is_empty());
    // Refused (library): dependencies stay within one Project.
    assert_err(&add(&a, &foreign), 422, "blocker-add");
    assert_eq!(blocked_by(&e, &a), vec![b.clone()]);
    // NotFound (library): no such blocker.
    assert_err(&add(&a, "LOCAL-99"), 404, "blocker-add");
}

#[test]
fn blocker_search_uses_library_search_and_excludes_self_and_blockers() {
    let e = Env::new();
    let a = add_titled(&e, "Needle host", "ready");
    let hit = add_titled(&e, "Needle other", "ready");
    let _miss = add_titled(&e, "Haystack", "ready");
    let done = add_titled(&e, "Needle done", "ready");
    e.ok(&["start", &done]);
    e.ok(&["done", &done]);
    let w = e.spawn_web(&[], &[]);
    let r = w.get(&format!("/ticket/{a}/blocker-search?q=needle"));
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(
        r.body.starts_with("<div id=\"blocker-results\""),
        "{}",
        r.body
    );
    assert!(r.body.contains("Needle other"), "{}", r.body);
    assert!(!r.body.contains("Needle host"), "self: {}", r.body);
    assert!(!r.body.contains("Needle done"), "closed: {}", r.body);
    assert!(!r.body.contains("Haystack"), "{}", r.body);
    assert!(r.body.contains(&format!("value=\"{hit}\"")), "{}", r.body);
    // Empty state.
    let r = w.get(&format!("/ticket/{a}/blocker-search?q=zzzz"));
    assert!(r.body.contains("No matching tickets"), "{}", r.body);
    // Already a blocker: no longer offered.
    w.post_form(&format!("/ticket/{a}/blockers"), &[("blocker", &hit)]);
    let r = w.get(&format!("/ticket/{a}/blocker-search?q=needle"));
    assert!(r.body.contains("No matching tickets"), "{}", r.body);
    // Unknown ticket.
    assert_err(
        &w.get("/ticket/LOCAL-99/blocker-search"),
        404,
        "blocker-add",
    );
}

#[test]
fn blocker_search_caps_at_twenty_rows_and_says_so() {
    let e = Env::new();
    let a = add_titled(&e, "Host", "ready");
    {
        let mut c = conn(&e);
        for i in 0..30 {
            scree::ticket::add(
                &mut c,
                &scree::scope::Scope::Unscoped,
                &new(&format!("Candidate {i}")),
            )
            .unwrap();
        }
    }
    let w = e.spawn_web(&[], &[]);
    let r = w.get(&format!("/ticket/{a}/blocker-search"));
    assert_eq!(
        r.body.matches("class=\"related\"").count(),
        20,
        "{}",
        r.body
    );
    assert!(r.body.contains("showing 20 of 30 matches"), "{}", r.body);
}

#[test]
fn edit_each_field() {
    let e = Env::new();
    let a = add_titled(&e, "Old title", "ready");
    let parent = add_titled(&e, "Parent", "ready");
    let w = e.spawn_web(&[], &[]);
    let path = format!("/ticket/{a}/edit");
    let edit = |fields: &[(&str, &str)]| {
        let v = detail(&e, &a).ticket.updated_at;
        let mut all = vec![("expected_updated_at", v.as_str())];
        all.extend_from_slice(fields);
        w.post_form(&path, &all)
    };
    let r = edit(&[("title", "New title")]);
    assert_region(&r, "New title");
    // The answer carries the form re-based on the new version.
    let v = detail(&e, &a).ticket.updated_at;
    assert!(r.body.contains("id=\"ticket-edit\""), "{}", r.body);
    assert!(r.body.contains(&format!("value=\"{v}\"")), "{}", r.body);
    assert_eq!(detail(&e, &a).ticket.title, "New title");
    assert_region(
        &edit(&[("description", "line one\r\nline two")]),
        "line two",
    );
    assert_eq!(detail(&e, &a).description, "line one\nline two");
    assert_region(&edit(&[("plan", "the **plan**")]), "<strong>plan</strong>");
    assert_eq!(detail(&e, &a).plan, "the **plan**");
    assert_region(&edit(&[("for", "human")]), "for human");
    assert_eq!(detail(&e, &a).ticket.for_who, "human");
    assert_region(&edit(&[("parent", &parent)]), "Subtask of");
    assert_eq!(
        detail(&e, &a).ticket.parent.as_deref(),
        Some(parent.as_str())
    );
    // A subtask has its own display id under the parent; look it up.
    let a = detail(&e, &parent).ticket.subtasks[0].id.clone();
    let v = detail(&e, &a).ticket.updated_at;
    let r = w.post_form(
        &format!("/ticket/{a}/edit"),
        &[("expected_updated_at", &v), ("detach", "1")],
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(detail(&e, &parent).ticket.subtasks.is_empty());
}

#[test]
fn edit_with_a_stale_version_is_409_and_changes_nothing() {
    let e = Env::new();
    let a = add_titled(&e, "Original", "ready");
    let w = e.spawn_web(&[], &[]);
    // The page the form came from.
    let loaded = detail(&e, &a).ticket.updated_at;
    // Another process edits the ticket after the page loaded.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    e.ok(&["edit", &a, "--description", "someone else was here"]);
    let before = detail(&e, &a);
    let r = w.post_form(
        &format!("/ticket/{a}/edit"),
        &[
            ("expected_updated_at", &loaded),
            ("title", "My late title"),
            ("description", ""),
        ],
    );
    assert_err(&r, 409, "edit");
    let after = detail(&e, &a);
    assert_eq!(after.ticket.title, "Original");
    assert_eq!(after.description, "someone else was here");
    assert_eq!(after.ticket.updated_at, before.ticket.updated_at);
}

#[test]
fn edit_refuses_blank_title_missing_version_and_no_change() {
    let e = Env::new();
    let a = add_titled(&e, "Keep", "ready");
    let w = e.spawn_web(&[], &[]);
    let v = detail(&e, &a).ticket.updated_at;
    let path = format!("/ticket/{a}/edit");
    // Usage (library): a blank title.
    assert_err(
        &w.post_form(&path, &[("expected_updated_at", &v), ("title", "  ")]),
        400,
        "edit",
    );
    assert_err(&w.post_form(&path, &[("title", "X")]), 400, "edit");
    assert_err(
        &w.post_form(&path, &[("expected_updated_at", &v), ("title", "Keep")]),
        400,
        "edit",
    );
    assert_eq!(detail(&e, &a).ticket.title, "Keep");
    assert_err(
        &w.post_form(
            "/ticket/LOCAL-99/edit",
            &[("expected_updated_at", &v), ("title", "X")],
        ),
        404,
        "edit",
    );
}

#[test]
fn edit_reparent_refusal_names_the_library_rule() {
    let e = Env::new();
    let top = add_titled(&e, "Top", "ready");
    let sub = e.ok(&["add", "Sub", "--parent", &top, "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let loose = add_titled(&e, "Loose", "ready");
    let w = e.spawn_web(&[], &[]);
    let v = detail(&e, &loose).ticket.updated_at;
    // Refused (library): tickets nest two levels deep.
    let r = w.post_form(
        &format!("/ticket/{loose}/edit"),
        &[("expected_updated_at", &v), ("parent", &sub)],
    );
    assert_err(&r, 422, "edit");
    assert!(detail(&e, &loose).ticket.parent.is_none());
}

#[test]
fn preview_renders_inert_markdown_and_writes_nothing() {
    let e = Env::new();
    let a = add_titled(&e, "P", "ready");
    let w = e.spawn_web(&[], &[]);
    let before = detail(&e, &a).ticket.updated_at;
    let evil = "# Head\n<script>alert(1)</script> [x](javascript:alert(1))";
    let r = w.post_form(
        &format!("/ticket/{a}/preview?field=plan"),
        &[("plan", evil)],
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body.starts_with("<div id=\"preview-plan\""), "{}", r.body);
    assert!(r.body.contains("<h1>"), "{}", r.body);
    assert!(!r.body.contains("<script"), "{}", r.body);
    assert!(!r.body.contains("href=\"javascript"), "{}", r.body);
    let r = w.post_form(
        &format!("/ticket/{a}/preview"),
        &[("field", "description"), ("body", "plain words")],
    );
    assert!(r.body.contains("plain words"), "{}", r.body);
    assert_err(
        &w.post_form(&format!("/ticket/{a}/preview?field=title"), &[]),
        400,
        "preview",
    );
    assert_eq!(detail(&e, &a).ticket.updated_at, before);
    assert!(detail(&e, &a).plan.is_empty());
}

#[test]
fn subtasks_add_in_running_order_and_refuse_nesting() {
    let e = Env::new();
    let top = add_titled(&e, "Top", "ready");
    let w = e.spawn_web(&[], &[]);
    let path = format!("/ticket/{top}/subtasks");
    let r = w.post_form(&path, &[("title", "First sub"), ("description", "why")]);
    assert_region(&r, "First sub");
    assert!(r.body.contains("id=\"ticket-subtask\""), "{}", r.body);
    let r = w.post_form(
        &path,
        &[
            ("title", "Second sub"),
            ("status", "ready"),
            ("for", "human"),
        ],
    );
    assert_region(&r, "Second sub");
    assert!(r.body.find("First sub").unwrap() < r.body.find("Second sub").unwrap());
    let d = detail(&e, &top);
    assert_eq!(d.ticket.subtasks.len(), 2);
    let first = detail(&e, &d.ticket.subtasks[0].id);
    assert_eq!(first.ticket.parent.as_deref(), Some(top.as_str()));
    assert_eq!(first.ticket.project, d.ticket.project);
    assert_eq!(first.ticket.worktree, d.ticket.worktree);
    assert_eq!(first.ticket.status, "draft");
    assert_eq!(first.description, "why");
    let second = detail(&e, &d.ticket.subtasks[1].id);
    assert_eq!(
        (
            second.ticket.status.as_str(),
            second.ticket.for_who.as_str()
        ),
        ("ready", "human")
    );
    // Usage (library): blank title.
    assert_err(&w.post_form(&path, &[("title", " ")]), 400, "subtask-add");
    // Refused (library): a subtask cannot have subtasks.
    let sub = d.ticket.subtasks[0].id.clone();
    assert_err(
        &w.post_form(&format!("/ticket/{sub}/subtasks"), &[("title", "Deep")]),
        422,
        "subtask-add",
    );
    assert_err(
        &w.post_form("/ticket/LOCAL-99/subtasks", &[("title", "X")]),
        404,
        "subtask-add",
    );
}

#[test]
fn a_subtask_in_a_worktree_stays_in_it_without_reading_the_cwd() {
    let e = Env::new();
    let r = repo(&e, "proj");
    let wt = worktree(&e, &r, "feat");
    let top = e.ok_at(&wt, &["add", "Top", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    // The server runs outside any repo.
    let w = e.spawn_web(&[], &[]);
    let r = w.post_form(&format!("/ticket/{top}/subtasks"), &[("title", "Child")]);
    assert_region(&r, "Child");
    let d = detail(&e, &top);
    let child = detail(&e, &d.ticket.subtasks[0].id);
    assert_eq!(child.ticket.worktree, d.ticket.worktree);
    assert!(child.ticket.worktree.is_some());
    assert_eq!(child.ticket.project.key, d.ticket.project.key);
}

#[test]
fn the_page_keeps_write_forms_outside_the_live_region() {
    let e = Env::new();
    let a = add_titled(&e, "Page", "ready");
    let w = e.spawn_web(&[], &[]);
    let page = w.get(&format!("/ticket/{a}")).body;
    let region_end = page.find("id=\"ticket-tools\"").expect("tools");
    let region_start = page.find("id=\"ticket\"").unwrap();
    // The edit form, picker and subtask form come after the region ends.
    for want in [
        "id=\"ticket-edit\"",
        "id=\"ticket-blockers\"",
        "id=\"ticket-subtask\"",
    ] {
        assert!(page.find(want).unwrap() > region_end, "{want}");
    }
    let region = &page[region_start..region_end];
    assert!(!region.contains("name=\"expected_updated_at\"") || region.contains("comment-"));
    assert!(!region.contains("id=\"ticket-edit\""));
    for control in ["edit", "blocker-add", "subtask-add"] {
        assert!(page.contains(&format!("id=\"err-{control}\"")), "{control}");
    }
}

// ---- Create: quick add and full form (POST /new, GET /new) ----

fn created_id(r: &common::Reply) -> String {
    assert_eq!(r.status, 200, "{}", r.body);
    let start = r.body.find("href=\"/ticket/").expect("created link") + "href=\"/ticket/".len();
    let end = start + r.body[start..].find('"').unwrap();
    r.body[start..end].to_string()
}

#[test]
fn quick_add_lands_in_the_picked_worktree_and_root_with_defaults() {
    let e = Env::new();
    let (_, wt) = setup(&e);
    let w = e.spawn_web(&[], &[]);
    let wts = wt.to_str().unwrap();
    let r = w.post_form("/new", &[("p", "MA"), ("w", wts), ("title", "In worktree")]);
    let id = created_id(&r);
    assert!(r.body.contains("id=\"create-quick\""), "{}", r.body);
    let d = detail(&e, &id);
    assert_eq!(d.ticket.project.key, "MA");
    assert_eq!(d.ticket.worktree.as_deref(), Some(wts));
    assert_eq!(d.ticket.status, "draft");
    assert_eq!(d.ticket.for_who, "agent");
    // The root checkout, by @root and by an absent w.
    for fields in [
        vec![("p", "MA"), ("w", "@root"), ("title", "Root a")],
        vec![
            ("p", "MA"),
            ("title", "Root b"),
            ("status", "ready"),
            ("for", "human"),
        ],
    ] {
        let id = created_id(&w.post_form("/new", &fields));
        let d = detail(&e, &id);
        assert_eq!(d.ticket.worktree, None);
        assert_eq!(d.ticket.project.key, "MA");
    }
    assert_eq!(tickets(&e), 4);
}

#[test]
fn quick_add_honours_ready_and_human() {
    let e = Env::new();
    setup(&e);
    let w = e.spawn_web(&[], &[]);
    let id = created_id(&w.post_form(
        "/new",
        &[
            ("p", "MA"),
            ("title", "T"),
            ("status", "ready"),
            ("for", "human"),
        ],
    ));
    let d = detail(&e, &id);
    assert_eq!(
        (d.ticket.status.as_str(), d.ticket.for_who.as_str()),
        ("ready", "human")
    );
}

#[test]
fn full_form_stores_description_plan_criteria_and_blockers() {
    let e = Env::new();
    setup(&e);
    // MA-1 is the seeded ticket in the same Project.
    assert_eq!(e.ok(&["show", "MA-1"])["project"]["key"], "MA");
    let w = e.spawn_web(&[], &[]);
    let id = created_id(&w.post_form(
        "/new",
        &[
            ("mode", "full"),
            ("p", "MA"),
            ("title", "Full"),
            ("description", "line1\r\nline2"),
            ("plan", "the plan"),
            ("criteria", "first\r\n\r\n  second  \r\n"),
            ("blockers", " MA-1 "),
        ],
    ));
    let d = detail(&e, &id);
    assert_eq!(d.description, "line1\nline2");
    assert_eq!(d.plan, "the plan");
    let texts: Vec<_> = d.criteria.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["first", "second"]);
    assert_eq!(blocked_by(&e, &id), vec!["MA-1".to_string()]);
    assert!(
        created_id(&w.post_form("/new", &[("mode", "full"), ("p", "MA"), ("title", "x")]))
            .starts_with("MA-")
    );
}

#[test]
fn refusals_show_inline_and_create_nothing() {
    let e = Env::new();
    let (_, wt) = setup(&e);
    let other = add_titled(&e, "Local", "ready");
    let w = e.spawn_web(&[], &[]);
    let before = tickets(&e);
    // Empty title, bad status, bad for.
    assert_err(
        &w.post_form("/new", &[("p", "MA"), ("title", "  ")]),
        400,
        "create",
    );
    assert_err(
        &w.post_form("/new", &[("p", "MA"), ("title", "x"), ("status", "done")]),
        400,
        "create",
    );
    assert_err(
        &w.post_form("/new", &[("p", "MA"), ("title", "x"), ("for", "robot")]),
        400,
        "create",
    );
    // Bad and cross-Project blockers.
    assert_err(
        &w.post_form(
            "/new",
            &[("p", "MA"), ("title", "x"), ("blockers", "MA-999")],
        ),
        404,
        "create",
    );
    let r = w.post_form("/new", &[("p", "MA"), ("title", "x"), ("blockers", &other)]);
    assert!(
        r.status == 422 || r.status == 404,
        "{} {}",
        r.status,
        r.body
    );
    assert!(r.body.contains("id=\"err-create\""));
    // Unknown Project and Worktree.
    assert_err(
        &w.post_form("/new", &[("p", "NOPE"), ("title", "x")]),
        404,
        "create",
    );
    assert_err(
        &w.post_form("/new", &[("p", "MA"), ("w", "/no/such"), ("title", "x")]),
        404,
        "create",
    );
    // Missing p.
    assert_err(&w.post_form("/new", &[("title", "x")]), 400, "create");
    assert_err(
        &w.post_form("/new", &[("p", "@all"), ("title", "x")]),
        400,
        "create",
    );
    assert_eq!(tickets(&e), before);
    // A gone directory.
    std::fs::remove_dir_all(&wt).unwrap();
    let r = w.post_form(
        "/new",
        &[("p", "MA"), ("w", wt.to_str().unwrap()), ("title", "x")],
    );
    assert_err(&r, 422, "create");
    assert!(r.body.contains(wt.to_str().unwrap()), "{}", r.body);
    assert_eq!(tickets(&e), before);
}

#[test]
fn all_projects_asks_for_a_project_first() {
    let e = Env::new();
    setup(&e);
    let w = e.spawn_web(&[], &[]);
    for path in ["/", "/new"] {
        let b = w.get(path).body;
        assert!(b.contains("Pick a Project to create a ticket"), "{path}");
        assert!(!b.contains("name=\"title\""), "{path}");
    }
    let picked = w.get("/?p=MA").body;
    assert!(
        picked.contains("name=\"title\"") && picked.contains("action")
            || picked.contains("@post('/new'")
    );
    let full = w.get("/new?p=MA").body;
    assert!(full.contains("name=\"criteria\"") && full.contains("name=\"blockers\""));
    assert!(full.contains("value=\"MA\""));
}

#[test]
fn empty_p_creates_in_the_unscoped_project() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let id = created_id(&w.post_form("/new", &[("p", ""), ("title", "Loose")]));
    let d = detail(&e, &id);
    assert_eq!(d.ticket.project.key, "LOCAL");
    assert_eq!(d.ticket.worktree, None);
    let page = w.get("/?p=").body;
    assert!(page.contains("name=\"title\""));
}

#[test]
fn a_subtask_takes_its_parents_scope_not_the_pickers() {
    let e = Env::new();
    let (_, wt) = setup(&e);
    let parent = e.ok_at(&wt, &["add", "Parent", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let w = e.spawn_web(&[], &[]);
    // The picked scope is the unscoped Project; the parent wins.
    let id = created_id(&w.post_form(
        "/new",
        &[("p", ""), ("parent", &parent), ("title", "Child")],
    ));
    let d = detail(&e, &id);
    let p = detail(&e, &parent);
    assert_eq!(d.ticket.parent.as_deref(), Some(parent.as_str()));
    assert_eq!(d.ticket.project, p.ticket.project);
    assert_eq!(d.ticket.worktree, p.ticket.worktree);
    assert_err(
        &w.post_form("/new", &[("p", ""), ("parent", "MA-999"), ("title", "x")]),
        404,
        "create",
    );
}

#[test]
fn a_human_may_create_for_human_and_an_agent_may_not_start_it() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let id = created_id(&w.post_form(
        "/new",
        &[
            ("p", ""),
            ("title", "Human work"),
            ("status", "ready"),
            ("for", "human"),
        ],
    ));
    assert_eq!(detail(&e, &id).ticket.for_who, "human");
    let agent = e.run_env_at(e.path(), &[("SCREE_ROLE", "agent")], &["start", &id]);
    assert!(!agent.status.success(), "{agent:?}");
    assert_eq!(db_status(&e, &id), "ready");
    let human = e.run_env_at(e.path(), &[("SCREE_ROLE", "human")], &["start", &id]);
    assert!(human.status.success(), "{human:?}");
}

#[test]
fn hostile_create_text_is_stored_verbatim_and_rendered_inert() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let evil = "<script>alert(1)</script> data-on-click=\"x\"";
    let r = w.post_form("/new", &[("p", ""), ("title", evil)]);
    let id = created_id(&r);
    assert!(!r.body.contains("<script>"));
    assert_eq!(detail(&e, &id).ticket.title, evil);
    let list = w.get("/?p=").body;
    assert!(!list.contains("<script>alert"), "{list}");
    assert!(list.contains("&lt;script&gt;"));
}

#[test]
fn the_list_keeps_the_quick_form_outside_the_live_region() {
    let e = Env::new();
    setup(&e);
    let w = e.spawn_web(&[], &[]);
    let page = w.get("/?p=MA").body;
    let form = page.find("id=\"create-quick\"").expect("quick form");
    let list = page.find("id=\"list\"").unwrap();
    assert!(form < list);
    assert!(page.contains("/new?p=MA"));
    assert!(page.contains("id=\"err-create\""));
}

// ---- Decisions screen ----

fn text(r: &common::Reply) -> String {
    // The error fragment's message, for comparing with the library's text.
    r.body.clone()
}

/// A LOCAL ticket with one open Decision (LOCAL-D-1) with options and a
/// recommendation.
fn decision_fixture(e: &Env) -> String {
    let t = e.ok(&["add", "Needs a choice", "--status", "ready"]);
    let id = t["id"].as_str().unwrap().to_string();
    e.ok(&[
        "decision",
        "add",
        &id,
        "Which <b>store</b>?",
        "--option",
        "sqlite",
        "--option",
        "postgres",
        "--recommend",
        "1",
    ]);
    id
}

fn decision_status(e: &Env, id: &str) -> (String, Option<String>, Option<String>) {
    let c = conn(e);
    let d = scree::decision::get(
        &c,
        &scree::decision::Ctx {
            cwd: Path::new("/"),
            project: None,
        },
        id,
    )
    .unwrap();
    (d.status, d.answer, d.decided_by)
}

#[test]
fn decisions_queue_orders_by_unblocks_then_oldest_and_respects_scope() {
    let e = Env::new();
    let (r, wt) = setup(&e);
    let a = e.ok(&["add", "A", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let b = e.ok(&["add", "B", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    e.ok(&["decision", "add", &a, "First asked"]);
    e.ok(&["decision", "add", &b, "Second asked"]);
    e.ok(&["decision", "add", &a, "Third asked"]);
    e.ok(&["decision", "link", "LOCAL-D-3", &b]);
    let seed = e.ok_at(&wt, &["add", "In MA", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    e.ok_at(&r, &["decision", "add", &seed, "Other project?"]);
    let w = e.spawn_web(&[], &[]);
    let page = w.get("/decisions").body;
    let at = |s: &str| {
        page.find(s)
            .unwrap_or_else(|| panic!("{s} missing: {page}"))
    };
    // D-3 unblocks two tickets; D-1 and D-2 one each, oldest first.
    assert!(at("LOCAL-D-3") < at("LOCAL-D-1"), "{page}");
    assert!(at("LOCAL-D-1") < at("LOCAL-D-2"), "{page}");
    assert!(page.contains("unblocks 2"), "{page}");
    assert!(page.contains("MA-D-1"), "{page}");
    // Scope narrows the queue; a Worktree reads its whole Project.
    let ma = w.get("/decisions?p=MA").body;
    assert!(ma.contains("MA-D-1") && !ma.contains("LOCAL-D-1"), "{ma}");
    let wtq = w
        .get(&format!("/decisions?p=MA&w={}", enc(wt.to_str().unwrap())))
        .body;
    assert!(
        wtq.contains("MA-D-1") && !wtq.contains("LOCAL-D-1"),
        "{wtq}"
    );
    let local = w.get("/decisions?p=").body;
    assert!(
        local.contains("LOCAL-D-1") && !local.contains("MA-D-1"),
        "{local}"
    );
    // The page and the live stream agree.
    let frame = first_frame(&w, "/decisions?p=MA");
    assert!(
        frame.contains("MA-D-1") && !frame.contains("LOCAL-D-1"),
        "{frame}"
    );
}

#[test]
fn answer_with_text_decides_as_human_with_qualified_id_and_leaves_queue() {
    let e = Env::new();
    decision_fixture(&e);
    // The server's cwd is an unrelated temp directory, not a repo.
    let w = e.spawn_web(&[], &[]);
    let r = w.post_form(
        "/decisions/LOCAL-D-1/answer",
        &[
            ("answer", "  postgres, really "),
            ("rationale", "scale"),
            ("p", ""),
        ],
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body.contains("id=\"decision-actions\""), "{}", r.body);
    assert!(r.body.contains("Answered LOCAL-D-1."), "{}", r.body);
    let (status, answer, by) = decision_status(&e, "LOCAL-D-1");
    assert_eq!(status, "decided");
    assert_eq!(answer.as_deref(), Some("postgres, really"));
    assert_eq!(by.as_deref(), Some("human"));
    let q = e.ok(&["decisions"]);
    assert!(q["decisions"].as_array().unwrap().is_empty());
    let page = w.get("/decisions").body;
    assert!(page.contains("No open Decisions"), "{page}");
}

#[test]
fn empty_answer_is_400_with_library_message_and_no_change() {
    let e = Env::new();
    decision_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    for blank in ["", "   \n "] {
        let r = w.post_form("/decisions/LOCAL-D-1/answer", &[("answer", blank)]);
        assert_eq!(r.status, 400, "{}", r.body);
        assert!(r.body.contains("id=\"err-answer\""), "{}", text(&r));
        assert!(r.body.contains("answer must not be empty"), "{}", r.body);
    }
    assert_eq!(decision_status(&e, "LOCAL-D-1").0, "open");
}

#[test]
fn accept_without_recommendation_is_400_with_library_message() {
    let e = Env::new();
    let t = e.ok(&["add", "T", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    e.ok(&["decision", "add", &t, "No advice?"]);
    let w = e.spawn_web(&[], &[]);
    let r = w.post_form("/decisions/LOCAL-D-1/accept", &[]);
    assert_eq!(r.status, 400, "{}", r.body);
    assert!(r.body.contains("id=\"err-accept\""), "{}", r.body);
    assert!(r.body.contains("has no recommendation"), "{}", r.body);
    assert_eq!(decision_status(&e, "LOCAL-D-1").0, "open");
    // No recommendation, so the page offers no accept button.
    let page = w.get("/decisions?d=LOCAL-D-1").body;
    assert!(!page.contains("Accept recommendation"), "{page}");
}

#[test]
fn accept_recommendation_decides_with_the_recommendation_text() {
    let e = Env::new();
    decision_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    let page = w.get("/decisions?d=LOCAL-D-1").body;
    assert!(page.contains("Accept recommendation"), "{page}");
    assert!(page.contains("data-confirm"), "{page}");
    let r = w.post_form("/decisions/LOCAL-D-1/accept", &[]);
    assert_eq!(r.status, 200, "{}", r.body);
    let (status, answer, by) = decision_status(&e, "LOCAL-D-1");
    assert_eq!(
        (status.as_str(), answer.as_deref(), by.as_deref()),
        ("decided", Some("sqlite"), Some("human"))
    );
}

#[test]
fn answering_an_already_answered_decision_is_409_inline_and_the_queue_updates_live() {
    let e = Env::new();
    decision_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    let mut s = w.stream(&format!("/stream?view={}", enc("/decisions")));
    let first = s.next_event(Duration::from_secs(3)).expect("first frame");
    assert!(first.contains("LOCAL-D-1"), "{first}");
    // Answered elsewhere (the CLI) after the page was loaded.
    e.ok(&[
        "--role",
        "human",
        "decision",
        "answer",
        "LOCAL-D-1",
        "elsewhere",
    ]);
    let r = w.post_form("/decisions/LOCAL-D-1/answer", &[("answer", "late")]);
    assert_eq!(r.status, 409, "{}", r.body);
    assert!(r.body.contains("id=\"err-answer\""), "{}", r.body);
    assert!(r.body.contains("already decided"), "{}", r.body);
    assert_eq!(
        decision_status(&e, "LOCAL-D-1").1.as_deref(),
        Some("elsewhere")
    );
    let r = w.post_form("/decisions/LOCAL-D-1/accept", &[]);
    assert_eq!(r.status, 409, "{}", r.body);
    // The queue stream sent the empty state.
    let next = s
        .next_event(Duration::from_secs(3))
        .expect("patch after the answer");
    assert!(next.contains("No open Decisions"), "{next}");
}

#[test]
fn selected_decision_answered_elsewhere_shows_this_changed_not_blank() {
    let e = Env::new();
    decision_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    let open = w.get("/decisions?d=LOCAL-D-1").body;
    assert!(open.contains("Which &lt;b&gt;store&lt;/b&gt;?"), "{open}");
    assert!(open.contains("aria-current=\"true\""), "{open}");
    e.ok(&[
        "--role",
        "human",
        "decision",
        "answer",
        "LOCAL-D-1",
        "mongo",
    ]);
    let plain = w.get("/decisions?d=LOCAL-D-1").body;
    assert!(plain.contains("This Decision changed"), "{plain}");
    assert!(plain.contains("answered by human: mongo"), "{plain}");
    assert!(
        !plain.contains("Answer</button>"),
        "no forms for a closed Decision: {plain}"
    );
    let live = first_frame(&w, "/decisions?d=LOCAL-D-1");
    assert!(live.contains("This Decision changed"), "{live}");
    assert!(live.contains("mongo"), "{live}");
}

#[test]
fn empty_queue_and_unknown_selected_id_have_their_own_states() {
    let e = Env::new();
    let w = e.spawn_web(&[], &[]);
    let r = w.get("/decisions");
    assert_eq!(r.status, 200);
    assert!(r.body.contains("No open Decisions"), "{}", r.body);
    let r = w.get("/decisions?d=LOCAL-D-9");
    assert_eq!(r.status, 200);
    assert!(
        r.body.contains("This Decision changed or was removed"),
        "{}",
        r.body
    );
    // Bare and malformed ids are refused, not guessed.
    for bad in ["D-1", "nonsense", "LOCAL-D-x"] {
        assert_eq!(w.get(&format!("/decisions?d={bad}")).status, 400, "{bad}");
    }
    assert_eq!(w.get("/decisions?p=NOPE").status, 404);
    // Unknown Decision on a write is 404 inline.
    let r = w.post_form("/decisions/LOCAL-D-9/answer", &[("answer", "x")]);
    assert_eq!(r.status, 404, "{}", r.body);
}

#[test]
fn no_banner_on_decisions_screen_and_banner_elsewhere() {
    let e = Env::new();
    decision_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    assert!(!w.get("/decisions").body.contains("class=\"banner\""));
    assert!(w.get("/").body.contains("class=\"banner\""));
    assert!(
        w.get("/decisions")
            .body
            .contains("href=\"/decisions\" aria-current=\"page\"")
    );
}

#[test]
fn hostile_decision_text_is_escaped_and_long_content_renders() {
    let e = Env::new();
    let t = e.ok(&["add", "T", "--status", "ready"])["id"]
        .as_str()
        .unwrap()
        .to_string();
    let evil = "<script>alert(1)</script>";
    let long = "word ".repeat(2000);
    let mut args: Vec<String> = ["decision", "add", &t, evil].map(String::from).to_vec();
    args.push("--option".into());
    args.push(evil.into());
    for i in 0..29 {
        args.push("--option".into());
        args.push(format!("option {i}"));
    }
    args.push("--option".into());
    args.push(long.clone());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    e.ok(&refs);
    let w = e.spawn_web(&[], &[]);
    let page = w.get("/decisions?d=LOCAL-D-1");
    assert_eq!(page.status, 200);
    assert!(!page.body.contains("<script>alert"), "{}", page.body);
    assert!(page.body.contains("&lt;script&gt;"));
    assert_eq!(page.body.matches("<li>").count(), 31);
    assert!(page.body.contains(long.trim()));
    assert_no_inline("decisions", &page.body);
    // Hostile answer text is stored verbatim and rendered inert afterwards.
    let r = w.post_form("/decisions/LOCAL-D-1/answer", &[("answer", evil)]);
    assert_eq!(r.status, 200, "{}", r.body);
    assert_eq!(decision_status(&e, "LOCAL-D-1").1.as_deref(), Some(evil));
    let after = w.get("/decisions?d=LOCAL-D-1").body;
    assert!(!after.contains("<script>alert"), "{after}");
    assert!(
        after.contains("answered by human: &lt;script&gt;"),
        "{after}"
    );
}

#[test]
fn decisions_queue_is_capped_at_200_rows_with_show_more() {
    let e = Env::new();
    e.ok(&["add", "T", "--status", "ready"]);
    {
        // 300 Decisions through the library is much faster than 300 processes.
        let mut c = conn(&e);
        let tx = c.transaction().unwrap();
        let pid: i64 = tx
            .query_row("SELECT id FROM projects WHERE key = 'LOCAL'", [], |r| {
                r.get(0)
            })
            .unwrap();
        let tid: i64 = tx
            .query_row("SELECT id FROM tickets LIMIT 1", [], |r| r.get(0))
            .unwrap();
        for n in 1..=300 {
            tx.execute(
                "INSERT INTO decisions (project_id, number, question, options, status, created_at)
                 VALUES (?1, ?2, ?3, '[]', 'open', '2026-01-01T00:00:00Z')",
                rusqlite::params![pid, n + 10, format!("Q{n}")],
            )
            .unwrap();
            let did = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO decision_tickets (decision_id, ticket_id) VALUES (?1, ?2)",
                [did, tid],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let w = e.spawn_web(&[], &[]);
    let page = w.get("/decisions").body;
    assert_eq!(page.matches("class=\"row\"").count(), 200, "rows");
    assert!(
        page.contains("showing 200 of 300"),
        "{}",
        &page[page.len().saturating_sub(600)..]
    );
    let all = w.get("/decisions?limit=300").body;
    assert_eq!(all.matches("class=\"row\"").count(), 300);
    assert!(!all.contains("showing "));
}

// ---- Ledger (W23) ----

/// Two decided Decisions (D-1 older, D-2 newer) on one ticket, plus an open D-3.
fn ledger_fixture(e: &Env) -> String {
    let t = e.ok(&["add", "Ledger ticket", "--status", "ready"]);
    let id = t["id"].as_str().unwrap().to_string();
    for q in ["First <b>question</b>?", "Second question?", "Still open?"] {
        e.ok(&["decision", "add", &id, q]);
    }
    e.ok(&[
        "--role",
        "human",
        "decision",
        "answer",
        "LOCAL-D-1",
        "alpha",
        "--rationale",
        "because <i>x</i>",
    ]);
    std::thread::sleep(Duration::from_millis(1100));
    e.ok(&["--role", "human", "decision", "answer", "LOCAL-D-2", "beta"]);
    id
}

fn superseded_by(e: &Env, id: &str) -> Option<String> {
    let c = conn(e);
    scree::decision::get(
        &c,
        &scree::decision::Ctx {
            cwd: Path::new("/"),
            project: None,
        },
        id,
    )
    .unwrap()
    .superseded_by
}

#[test]
fn ledger_lists_decided_newest_first_and_nav_links_it() {
    let e = Env::new();
    let ticket = ledger_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    let r = w.get("/ledger");
    assert_eq!(r.status, 200);
    let page = r.body;
    assert!(
        page.contains("<a class=\"nav-item\" href=\"/ledger\" aria-current=\"page\""),
        "{page}"
    );
    assert!(
        page.find("LOCAL-D-2").unwrap() < page.find("LOCAL-D-1").unwrap(),
        "{page}"
    );
    assert!(
        page.contains("Answer: alpha") && page.contains("Answer: beta"),
        "{page}"
    );
    assert!(
        page.contains("Rationale: because &lt;i&gt;x&lt;/i&gt;"),
        "{page}"
    );
    assert!(page.contains("Decided by human on "), "{page}");
    assert!(
        page.contains(&format!("href=\"/ticket/{ticket}\"")),
        "{page}"
    );
    assert!(
        page.contains("First &lt;b&gt;question&lt;/b&gt;?"),
        "{page}"
    );
    assert!(
        !page.contains("Still open?") && !page.contains("LOCAL-D-3"),
        "{page}"
    );
    // The open Decision shows in the banner.
    assert!(page.contains("class=\"banner\""), "{page}");
    assert_no_inline("ledger", &page);
}

#[test]
fn ledger_filters_search_and_since_with_distinct_empty_states() {
    let e = Env::new();
    ledger_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    let hit = w.get("/ledger?q=alpha").body;
    assert!(
        hit.contains("LOCAL-D-1") && !hit.contains("LOCAL-D-2"),
        "{hit}"
    );
    let none = w.get("/ledger?q=zzzz").body;
    assert!(none.contains("No decided Decisions match"), "{none}");
    let since = w.get("/ledger?since=1d").body;
    assert!(
        since.contains("LOCAL-D-1") && since.contains("LOCAL-D-2"),
        "{since}"
    );
    assert_eq!(w.get("/ledger?since=garbage").status, 400);
    let empty = Env::new();
    let w2 = empty.spawn_web(&[], &[]);
    assert!(
        w2.get("/ledger").body.contains("No decided Decisions"),
        "empty"
    );
    assert!(
        !w2.get("/ledger").body.contains("match"),
        "empty vs no-match"
    );
}

#[test]
fn ledger_supersede_works_with_qualified_ids_and_hides_by_default() {
    let e = Env::new();
    ledger_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    let sel = w.get("/ledger?d=LOCAL-D-1").body;
    assert!(
        sel.contains("Supersede LOCAL-D-1") && sel.contains("id=\"err-supersede\""),
        "{sel}"
    );
    let r = w.post_form("/ledger/LOCAL-D-1/supersede", &[("by", "local-d-2")]);
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(
        r.body.contains("LOCAL-D-1 superseded by LOCAL-D-2."),
        "{}",
        r.body
    );
    assert_eq!(superseded_by(&e, "LOCAL-D-1").as_deref(), Some("D-2"));
    let hidden = w.get("/ledger").body;
    assert!(!hidden.contains("LOCAL-D-1"), "{hidden}");
    let shown = w.get("/ledger?superseded=1").body;
    assert!(shown.contains("superseded by LOCAL-D-2"), "{shown}");
}

#[test]
fn ledger_supersede_refusals_are_inline_and_change_nothing() {
    let e = Env::new();
    ledger_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    assert_eq!(
        w.post_form("/ledger/LOCAL-D-1/supersede", &[("by", "LOCAL-D-2")])
            .status,
        200
    );
    // Already superseded.
    let r = w.post_form("/ledger/LOCAL-D-1/supersede", &[("by", "LOCAL-D-2")]);
    assert_eq!(r.status, 422, "{}", r.body);
    assert!(r.body.contains("id=\"err-supersede\""), "{}", r.body);
    // Itself.
    let r = w.post_form("/ledger/LOCAL-D-2/supersede", &[("by", "LOCAL-D-2")]);
    assert_eq!(r.status, 422, "{}", r.body);
    assert_eq!(superseded_by(&e, "LOCAL-D-2"), None);
    // Blank and bare ids are Usage.
    for by in ["", "  ", "D-2"] {
        let r = w.post_form("/ledger/LOCAL-D-2/supersede", &[("by", by)]);
        assert_eq!(r.status, 400, "{by:?}: {}", r.body);
    }
    // Unknown Decision.
    let r = w.post_form("/ledger/LOCAL-D-9/supersede", &[("by", "LOCAL-D-2")]);
    assert_eq!(r.status, 404, "{}", r.body);
    assert_eq!(superseded_by(&e, "LOCAL-D-1").as_deref(), Some("D-2"));
}

#[test]
fn ledger_respects_scope_and_updates_live() {
    let e = Env::new();
    ledger_fixture(&e);
    let w = e.spawn_web(&[], &[]);
    let other = w.get("/ledger?p=NOPE");
    assert_eq!(other.status, 404);
    let local = w.get("/ledger?p=").body;
    assert!(local.contains("LOCAL-D-1"), "{local}");
    let mut s = w.stream(&format!("/stream?view={}", enc("/ledger")));
    let first = s.next_event(Duration::from_secs(3)).expect("first frame");
    assert!(first.contains("LOCAL-D-1"), "{first}");
    e.ok(&["decision", "supersede", "LOCAL-D-1", "--by", "LOCAL-D-2"]);
    let next = s.next_event(Duration::from_secs(3)).expect("patch");
    assert!(!next.contains("LOCAL-D-1"), "{next}");
}

#[test]
fn ledger_escapes_hostile_text_and_caps_rows() {
    let e = Env::new();
    e.ok(&["add", "T", "--status", "ready"]);
    {
        let mut c = conn(&e);
        let tx = c.transaction().unwrap();
        let pid: i64 = tx
            .query_row("SELECT id FROM projects WHERE key = 'LOCAL'", [], |r| {
                r.get(0)
            })
            .unwrap();
        for n in 1..=300 {
            tx.execute(
                "INSERT INTO decisions (project_id, number, question, options, status, created_at, answer, decided_by, decided_at)
                 VALUES (?1, ?2, ?3, '[]', 'decided', '2026-01-01T00:00:00Z', '<script>alert(1)</script>', 'human', '2026-01-02T00:00:00Z')",
                rusqlite::params![pid, n, format!("Q{n}")],
            )
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let w = e.spawn_web(&[], &[]);
    let page = w.get("/ledger").body;
    assert_eq!(page.matches("class=\"row\"").count(), 200);
    assert!(page.contains("showing 200 of 300"));
    assert!(!page.contains("<script>alert"), "escaped");
    let all = w.get("/ledger?limit=300").body;
    assert_eq!(all.matches("class=\"row\"").count(), 300);
    assert!(!all.contains("showing "));
}
