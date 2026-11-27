//! Shared harness for the integration tests: one host-independent spawn path.
//!
//! AGENTS.md promises that every test clears `SCREE_*`, `CLAUDECODE`,
//! `AI_AGENT` and `GIT_*` and runs outside any repo unless it is testing scope.
//! That rule used to be copied into each file and drifted; it lives here so a
//! new file imports it. Compiled into every integration binary through
//! `mod common;`, never as a target of its own, so items only some binaries
//! use are allowed to look dead.
#![allow(dead_code)]

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

use serde_json::Value;

/// Names removed from every child regardless of what the parent exports. Each
/// one is read by production code, so a stray value on the host would change
/// what a test sees:
///
/// - `CLAUDECODE`, `AI_AGENT`, `CLAUDE_CODE_SESSION_ID`: role and session,
///   src/identity.rs.
/// - `NO_COLOR`: colour choice, src/output.rs.
/// - `XDG_DATA_HOME`: database fallback, src/db.rs. A test that wants it sets
///   it itself after the scrub, as tests/cli.rs does.
pub const SCRUB_NAMES: &[&str] = &[
    "CLAUDECODE",
    "AI_AGENT",
    "NO_COLOR",
    "CLAUDE_CODE_SESSION_ID",
    "XDG_DATA_HOME",
];

/// Prefixes swept over the parent's variables. src/cli.rs binds five options
/// straight out of `SCREE_*`, and src/scope.rs detects scope by shelling out to
/// git, so a `GIT_DIR` reaching the child is exactly what this rule exists to
/// stop. `HOME` is deliberately absent: src/db.rs falls back to it, and the
/// temp-dir machinery needs it.
pub const SCRUB_PREFIXES: &[&str] = &["SCREE_", "GIT_"];

/// Remove every name in [`SCRUB_NAMES`] and every parent variable whose key
/// starts with a prefix in [`SCRUB_PREFIXES`]. Reads the environment through
/// `vars_os` like src/main.rs does, so a non-UTF-8 key cannot panic here
/// (tests/cli.rs::non_utf8_env_does_not_panic guards that property).
pub fn scrub(cmd: &mut Command) {
    for name in SCRUB_NAMES {
        cmd.env_remove(name);
    }
    scrub_prefixes(cmd, std::env::vars_os().map(|(key, _)| key));
}

/// Drop every key in `keys` that starts with a prefix in [`SCRUB_PREFIXES`].
/// Split out so the regression test can hand it keys the host need not export.
fn scrub_prefixes(cmd: &mut Command, keys: impl Iterator<Item = OsString>) {
    for key in keys {
        if SCRUB_PREFIXES
            .iter()
            .any(|p| key.to_string_lossy().starts_with(p))
        {
            cmd.env_remove(key);
        }
    }
}

/// The scree binary, scrubbed, with no working directory and no arguments.
pub fn scree_command() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_scree"));
    scrub(&mut cmd);
    cmd
}

/// A scrubbed scree command on `db`, run in `cwd`. For spawns built away from
/// an [`Env`], such as inside a thread.
pub fn scree_cmd(db: &Path, cwd: &Path) -> Command {
    let mut cmd = scree_command();
    cmd.current_dir(cwd).arg("--db").arg(db);
    cmd
}

/// A scrubbed `git` configured for reproducible commits, run in `dir`, for the
/// repo and worktree setup the scope tests do by hand.
pub fn git_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    scrub(&mut cmd);
    cmd.current_dir(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ]);
    cmd
}

fn ok_value(out: Output, args: &[&str]) -> Value {
    assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}

fn ticket_ids(v: &Value) -> Vec<String> {
    v["tickets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().to_string())
        .collect()
}

/// A throwaway directory that is not a repository, holding the database. Every
/// command an `Env` launches runs in it, which is what keeps a result off the
/// host.
pub struct Env {
    dir: tempfile::TempDir,
    path: PathBuf,
    flags: &'static [&'static str],
}

impl Env {
    /// An empty environment with no global flags; the database sits at its root.
    pub fn new() -> Self {
        Self::with_flags(&[])
    }

    /// Like [`Env::new`], except that every command carries `flags` ahead of its
    /// own arguments. The sweep suite pins a session this way.
    pub fn with_flags(flags: &'static [&'static str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        // Canonicalised so git and scree agree on one spelling of anything under
        // this directory: macOS hands out /var, which links to /private/var.
        let path = std::fs::canonicalize(dir.path()).unwrap();
        Env { dir, path, flags }
    }

    /// The throwaway directory itself.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn db(&self) -> PathBuf {
        self.path.join("scree.db")
    }

    /// A scrubbed command on this environment's database, run in `cwd`, carrying
    /// the environment's global flags but no arguments of its own.
    fn base(&self, cwd: &Path) -> Command {
        let mut cmd = scree_cmd(&self.db(), cwd);
        cmd.args(self.flags);
        cmd
    }

    /// A scrubbed command on this environment's database, run in `cwd`.
    pub fn cmd_at(&self, cwd: &Path, args: &[&str]) -> Command {
        let mut cmd = self.base(cwd);
        cmd.args(args);
        cmd
    }

    /// A scrubbed command on this environment's database, run in the temp dir.
    pub fn cmd(&self, args: &[&str]) -> Command {
        let path = self.path.clone();
        self.cmd_at(&path, args)
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    pub fn run_at(&self, cwd: &Path, args: &[&str]) -> Output {
        self.cmd_at(cwd, args).output().unwrap()
    }

    /// Run in `cwd` with extra variables applied after the scrub, for tests that
    /// prove how scree reads one.
    pub fn run_env_at(&self, cwd: &Path, extra: &[(&str, &str)], args: &[&str]) -> Output {
        let mut cmd = self.cmd_at(cwd, args);
        for (k, v) in extra {
            cmd.env(k, v);
        }
        cmd.output().unwrap()
    }

    pub fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        ok_value(out, args)
    }

    pub fn ok_at(&self, cwd: &Path, args: &[&str]) -> Value {
        let out = self.run_at(cwd, args);
        ok_value(out, args)
    }

    pub fn code(&self, args: &[&str]) -> i32 {
        self.run(args).status.code().unwrap()
    }

    pub fn code_at(&self, cwd: &Path, args: &[&str]) -> i32 {
        self.run_at(cwd, args).status.code().unwrap()
    }

    /// Launch in `cwd` with all three streams piped and `input` written to
    /// stdin; the caller waits on the child.
    pub fn spawn_stdin(&self, cwd: &Path, args: &[&str], input: &str) -> Child {
        let mut cmd = self.cmd_at(cwd, args);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child
    }

    fn run_stdin(&self, cwd: &Path, args: &[&str], input: &str) -> Output {
        self.spawn_stdin(cwd, args, input)
            .wait_with_output()
            .unwrap()
    }

    /// Run with `input` on stdin, expecting success; the JSON on stdout.
    pub fn stdin_ok(&self, args: &[&str], input: &str) -> Value {
        let path = self.path.clone();
        let out = self.run_stdin(&path, args, input);
        ok_value(out, args)
    }

    /// Run in `cwd` with `input` on stdin, expecting success; the JSON on stdout.
    pub fn stdin_ok_at(&self, cwd: &Path, args: &[&str], input: &str) -> Value {
        let out = self.run_stdin(cwd, args, input);
        ok_value(out, args)
    }

    /// Run with `input` on stdin; the exit code.
    pub fn stdin_code(&self, args: &[&str], input: &str) -> i32 {
        self.stdin_result(args, input).0
    }

    /// Run with `input` on stdin; the exit code and stderr as text.
    pub fn stdin_result(&self, args: &[&str], input: &str) -> (i32, String) {
        let path = self.path.clone();
        let out = self.run_stdin(&path, args, input);
        (
            out.status.code().unwrap(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    }

    /// Run with `var` set to the empty string *after* the scrub: the way to
    /// show an empty value reads as unset. `SCREE_DB` is special, because with
    /// it empty and no `--db` scree falls back to `XDG_DATA_HOME` (src/db.rs),
    /// which this points at the environment's directory.
    pub fn run_empty(&self, var: &str, args: &[&str]) -> Output {
        let mut cmd = if var == "SCREE_DB" {
            let mut c = scree_command();
            c.current_dir(&self.path)
                .env("XDG_DATA_HOME", &self.path)
                .args(self.flags);
            c
        } else {
            self.base(&self.path)
        };
        cmd.env(var, "").args(args).output().unwrap()
    }

    /// Run expecting success; stdout as text, unparsed. A byte-exact assertion
    /// needs the original key order, which parsing into a Value would drop.
    pub fn stdout(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    pub fn stderr(out: &Output) -> String {
        String::from_utf8_lossy(&out.stderr).to_string()
    }

    /// Ids from a `list` run in the temp dir.
    pub fn ids(&self, args: &[&str]) -> Vec<String> {
        let list = list_args(args);
        ticket_ids(&self.ok(&list))
    }

    /// Ids from a `list` run in `cwd`.
    pub fn ids_at(&self, cwd: &Path, args: &[&str]) -> Vec<String> {
        let list = list_args(args);
        ticket_ids(&self.ok_at(cwd, &list))
    }

    /// One value from a query against this environment's database.
    pub fn sql<T>(&self, q: &str, f: impl FnOnce(&rusqlite::Row) -> rusqlite::Result<T>) -> T {
        rusqlite::Connection::open(self.db())
            .unwrap()
            .query_row(q, [], f)
            .unwrap()
    }

    /// Run statements against this environment's database.
    pub fn exec_batch(&self, q: &str) {
        rusqlite::Connection::open(self.db())
            .unwrap()
            .execute_batch(q)
            .unwrap();
    }
}

fn list_args<'a>(args: &[&'a str]) -> Vec<&'a str> {
    let mut list = vec!["list"];
    list.extend_from_slice(args);
    list
}

/// Prove that a command the harness builds carries the whole scrub: an explicit
/// removal for every name in [`SCRUB_NAMES`], a removal rather than a value for
/// every parent variable matching [`SCRUB_PREFIXES`], and the harness directory
/// as the working directory. Called by `spawn_is_scrubbed` in tests/spawn.rs.
pub fn assert_spawn_is_scrubbed() {
    // Spelled out again here on purpose: dropping an entry from a list below
    // fails this test instead of quietly making one file host-dependent again.
    const NAMES: &[&str] = &[
        "CLAUDECODE",
        "AI_AGENT",
        "NO_COLOR",
        "CLAUDE_CODE_SESSION_ID",
        "XDG_DATA_HOME",
    ];
    const PREFIXES: &[&str] = &["SCREE_", "GIT_"];
    assert_eq!(SCRUB_NAMES, NAMES, "the fixed scrub list changed");
    assert_eq!(SCRUB_PREFIXES, PREFIXES, "the scrub prefixes changed");

    let e = Env::new();
    let cmd = e.cmd(&["list"]);
    let mut removed: Vec<String> = Vec::new();
    let mut handed_back: Vec<String> = Vec::new();
    for (key, value) in cmd.get_envs() {
        match value {
            None => removed.push(key.to_string_lossy().into_owned()),
            Some(_) => handed_back.push(key.to_string_lossy().into_owned()),
        }
    }
    for name in NAMES {
        assert!(removed.contains(&name.to_string()), "{name} not removed");
    }
    for (key, _) in std::env::vars_os() {
        let key = key.to_string_lossy().into_owned();
        if PREFIXES.iter().any(|p| key.starts_with(p)) {
            assert!(removed.contains(&key), "{key} not swept by prefix");
            assert!(!handed_back.contains(&key), "{key} set for the child");
        }
    }
    // The same sweep over keys this host happens not to export.
    for prefix in PREFIXES {
        let sentinel = OsString::from(format!("{prefix}SENTINEL"));
        let mut probe = scree_command();
        scrub_prefixes(&mut probe, std::iter::once(sentinel.clone()));
        assert!(
            probe
                .get_envs()
                .any(|(k, v)| v.is_none() && k == sentinel.as_os_str()),
            "{prefix}* survives a scrub"
        );
    }
    assert_eq!(
        cmd.get_current_dir(),
        Some(e.path()),
        "a harness command must not run in the checkout hosting nextest"
    );
}

/// A running `scree web` child. Dropping it kills the process.
pub struct Web {
    child: Child,
    pub addr: String,
    log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

/// An open `text/event-stream` response, read event by event.
pub struct EventStream {
    sock: std::net::TcpStream,
    pub status: u16,
    buf: String,
}

impl EventStream {
    /// The next event (text up to a blank line) within `within`, or `None` on
    /// timeout or when the server closed the stream.
    pub fn next_event(&mut self, within: std::time::Duration) -> Option<String> {
        use std::io::Read;
        let deadline = std::time::Instant::now() + within;
        loop {
            if let Some(i) = self.buf.find("\n\n") {
                let ev = self.buf[..i].to_string();
                self.buf.drain(..i + 2);
                return Some(ev);
            }
            let left = deadline.checked_duration_since(std::time::Instant::now())?;
            self.sock
                .set_read_timeout(Some(left.max(std::time::Duration::from_millis(1))))
                .unwrap();
            let mut chunk = [0u8; 4096];
            match self.sock.read(&mut chunk) {
                Ok(0) | Err(_) => return None,
                Ok(n) => self.buf.push_str(&String::from_utf8_lossy(&chunk[..n])),
            }
        }
    }

    /// True when the server closed the stream within `within`.
    pub fn closed_within(&mut self, within: std::time::Duration) -> bool {
        use std::io::Read;
        let deadline = std::time::Instant::now() + within;
        loop {
            let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) else {
                return false;
            };
            self.sock
                .set_read_timeout(Some(left.max(std::time::Duration::from_millis(1))))
                .unwrap();
            let mut chunk = [0u8; 4096];
            match self.sock.read(&mut chunk) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return false;
                }
                Err(_) => return true,
            }
        }
    }
}

/// A parsed HTTP response.
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

impl Env {
    /// Start `scree web --port 0` on this environment's database and wait for
    /// its `listening on http://ADDR` line. `extra_env` is applied after the
    /// scrub. Panics when the line does not arrive in 10 seconds.
    pub fn spawn_web(&self, extra_env: &[(&str, &str)], extra_args: &[&str]) -> Web {
        self.spawn_web_at(self.path(), extra_env, extra_args)
    }

    /// Like [`Env::spawn_web`] with the server's cwd set to `cwd`.
    pub fn spawn_web_at(&self, cwd: &Path, extra_env: &[(&str, &str)], extra_args: &[&str]) -> Web {
        let mut cmd = self.cmd_at(cwd, &["web", "--port", "0"]);
        cmd.args(extra_args);
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = log.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            for line in std::io::BufReader::new(stderr)
                .lines()
                .map_while(|l| l.ok())
            {
                sink.lock().unwrap().push(line.clone());
                if let Some(i) = line.find("listening on http://") {
                    let _ = tx.send(line[i + "listening on http://".len()..].trim().to_string());
                    // Keep draining so the child never blocks on a full pipe.
                }
            }
        });
        let addr = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("no 'listening on' line from scree web");
        Web { child, addr, log }
    }
}

impl Web {
    pub fn port(&self) -> u16 {
        self.addr.rsplit(':').next().unwrap().parse().unwrap()
    }

    /// GET or POST `path` with the default headers (valid Host, `Connection:
    /// close`) plus `extra`. A header in `extra` named `Host` replaces the
    /// default one.
    pub fn request(&self, method: &str, path: &str, extra: &[(&str, &str)]) -> Reply {
        self.request_body(method, path, extra, "")
    }

    /// Like [`Web::request`] with a request body (`Content-Length` set).
    pub fn request_body(
        &self,
        method: &str,
        path: &str,
        extra: &[(&str, &str)],
        body: &str,
    ) -> Reply {
        use std::io::{Read, Write};
        let mut head = format!("{method} {path} HTTP/1.1\r\n");
        if !extra.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
            head.push_str(&format!("Host: {}\r\n", self.addr));
        }
        for (k, v) in extra {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        head.push_str(&format!(
            "Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        ));
        head.push_str(body);
        let mut s = std::net::TcpStream::connect(&self.addr).unwrap();
        let t = Some(std::time::Duration::from_secs(5));
        s.set_read_timeout(t).unwrap();
        s.set_write_timeout(t).unwrap();
        s.write_all(head.as_bytes()).unwrap();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).unwrap();
        let raw = String::from_utf8_lossy(&raw).into_owned();
        let (h, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
        let mut lines = h.lines();
        let status = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|c| c.parse().ok())
            .unwrap_or_else(|| panic!("bad response: {raw:?}"));
        let headers = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect();
        Reply {
            status,
            headers,
            body: body.to_string(),
        }
    }

    /// Every stderr line the server has printed so far.
    pub fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    /// The `Origin` header value a same-origin write needs.
    pub fn origin(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// POST with a valid `Origin`.
    pub fn post(&self, path: &str) -> Reply {
        self.request("POST", path, &[("Origin", &self.origin())])
    }

    /// POST an urlencoded form with a valid `Origin`.
    pub fn post_form(&self, path: &str, fields: &[(&str, &str)]) -> Reply {
        let body = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields)
            .finish();
        self.request_body(
            "POST",
            path,
            &[
                ("Origin", &self.origin()),
                ("Content-Type", "application/x-www-form-urlencoded"),
            ],
            &body,
        )
    }

    /// Open `path` as an event stream. HTTP/1.0 so the body is not chunked.
    pub fn stream(&self, path: &str) -> EventStream {
        use std::io::{Read, Write};
        let mut sock = std::net::TcpStream::connect(&self.addr).unwrap();
        write!(sock, "GET {path} HTTP/1.0\r\nHost: {}\r\n\r\n", self.addr).unwrap();
        sock.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            assert_eq!(sock.read(&mut byte).unwrap(), 1, "stream closed in headers");
            head.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&head).into_owned();
        let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        EventStream {
            sock,
            status,
            buf: String::new(),
        }
    }

    pub fn get(&self, path: &str) -> Reply {
        self.request("GET", path, &[])
    }

    /// Send SIGINT and wait up to `within` for the child to exit; true when it did.
    pub fn interrupt(&mut self, within: std::time::Duration) -> bool {
        self.signal("INT", within)
    }

    /// Send the signal named as `kill` spells it (`TERM`, `KILL`) and wait up
    /// to `within` for the child to exit; true when it did.
    pub fn signal(&mut self, name: &str, within: std::time::Duration) -> bool {
        Command::new("kill")
            .args([&format!("-{name}"), &self.child.id().to_string()])
            .status()
            .unwrap();
        let start = std::time::Instant::now();
        while start.elapsed() < within {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    }
}

impl Drop for Web {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
