//! The control socket: how the CLI finds the running `scree web` (ADR 0005).
//!
//! The server listens on a unix socket whose path is derived from the database
//! path, so the server and any CLI using the same database agree on it with no
//! configuration. The protocol is one request line and one response line per
//! connection. The only request is `status`; the response is a JSON
//! [`Status`], or `{"error": "..."}` for anything else.
//!
//! Ownership of the socket is an exclusive lock on a file beside it, held for
//! the server's lifetime, so one database has at most one server. The kernel
//! drops the lock when the process exits for any reason, which makes a socket
//! file left by a killed server safe to replace.

use std::ffi::OsString;
use std::fs::{File, TryLockError};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const STATUS_REQUEST: &str = "status";
/// Bounds on what either side will read, and how long it will wait.
const MAX_REQUEST_BYTES: u64 = 64;
const MAX_RESPONSE_BYTES: u64 = 4096;
const TIMEOUT: Duration = Duration::from_secs(2);

/// Where a running server can be reached. Fields are alphabetical: this is
/// also the JSON `scree web status` prints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub port: u16,
    /// Base URL of the web UI, as a browser on this machine can open it.
    pub url: String,
}

impl Status {
    /// The status of a server bound to `addr` (the real address, after a
    /// port-0 bind). A wildcard address cannot be browsed to, so it is
    /// reported as the loopback address of the same family.
    pub(crate) fn of(addr: SocketAddr) -> Status {
        let mut reach = addr;
        if addr.ip().is_unspecified() {
            reach.set_ip(match addr {
                SocketAddr::V4(_) => Ipv4Addr::LOCALHOST.into(),
                SocketAddr::V6(_) => Ipv6Addr::LOCALHOST.into(),
            });
        }
        Status {
            port: addr.port(),
            url: format!("http://{reach}"),
        }
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = OsString::from(path);
    s.push(suffix);
    PathBuf::from(s)
}

/// The socket of the server for the database at `db`: `<db>.sock`.
fn socket_path(db: &Path) -> PathBuf {
    with_suffix(db, ".sock")
}

fn is_socket(path: &Path) -> std::io::Result<bool> {
    Ok(std::fs::symlink_metadata(path)?.file_type().is_socket())
}

/// The exclusive right to serve one database. Dropping it removes the socket
/// file, then releases the lock.
pub(crate) struct Claim {
    socket: PathBuf,
    _lock: File,
}

impl Claim {
    /// Take the lock, or fail naming the server that already holds it.
    pub(crate) fn acquire(db: &Path) -> Result<Claim> {
        let socket = socket_path(db);
        let lock_path = with_suffix(&socket, ".lock");
        let fail =
            |e: std::io::Error| Error::Other(format!("cannot lock {}: {e}", lock_path.display()));
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(fail)?;
        match lock.try_lock() {
            Ok(()) => Ok(Claim {
                socket,
                _lock: lock,
            }),
            Err(TryLockError::WouldBlock) => {
                // The holder may not be listening yet, or any more.
                let at = match query(db) {
                    Ok(Some(s)) => format!(" at {}", s.url),
                    _ => String::new(),
                };
                Err(Error::Other(format!(
                    "a scree web server{at} is already serving {}",
                    db.display()
                )))
            }
            Err(TryLockError::Error(e)) => Err(fail(e)),
        }
    }

    /// Bind the socket, usable by this user only, and answer `status` on it
    /// from a thread. The thread is not joined: it only waits in `accept`, and
    /// the process ending stops it.
    pub(crate) fn serve(&self, status: Status) -> Result<()> {
        let path = &self.socket;
        let fail =
            |e: std::io::Error| Error::Other(format!("cannot listen on {}: {e}", path.display()));
        // Holding the lock means any socket here is a dead server's. Anything
        // else at this path is not ours to delete; bind then fails on it.
        match is_socket(path) {
            Ok(true) => std::fs::remove_file(path).map_err(fail)?,
            Ok(false) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(fail(e)),
        }
        let listener = UnixListener::bind(path).map_err(fail)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(fail)?;
        thread::Builder::new()
            .spawn(move || answer(&listener, &status))
            .map_err(fail)?;
        Ok(())
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if is_socket(&self.socket).unwrap_or(false) {
            let _ = std::fs::remove_file(&self.socket);
        }
    }
}

fn answer(listener: &UnixListener, status: &Status) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let status = status.clone();
                // A client that stalls or hangs up, or a thread that cannot
                // start, only loses that answer.
                let _ = thread::Builder::new().spawn(move || reply(stream, &status));
            }
            // Accept fails transiently (e.g. out of descriptors); do not spin.
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn reply(mut stream: UnixStream, status: &Status) -> std::io::Result<()> {
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    let mut request = String::new();
    BufReader::new((&stream).take(MAX_REQUEST_BYTES)).read_line(&mut request)?;
    let response = match request.trim_end() {
        STATUS_REQUEST => serde_json::to_string(status)?,
        _ => r#"{"error":"unknown request"}"#.to_string(),
    };
    writeln!(stream, "{response}")
}

/// Ask the server for the database at `db` where it is. `None` means no server
/// is running; a server that cannot be asked or answers badly is an error.
/// Touches neither the database nor the lock.
pub fn query(db: &Path) -> Result<Option<Status>> {
    let path = socket_path(db);
    let fail = |e: &dyn std::fmt::Display| {
        Error::Other(format!(
            "cannot ask the scree web server at {}: {e}",
            path.display()
        ))
    };
    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        // No socket, a dead server's socket, or a path too long to have ever
        // been bound.
        Err(e)
            if matches!(
                e.kind(),
                ErrorKind::NotFound
                    | ErrorKind::NotADirectory
                    | ErrorKind::ConnectionRefused
                    | ErrorKind::InvalidInput
            ) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(fail(&e)),
    };
    let mut response = String::new();
    stream
        .set_read_timeout(Some(TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(TIMEOUT)))
        .and_then(|()| writeln!(stream, "{STATUS_REQUEST}"))
        .and_then(|()| BufReader::new(stream.take(MAX_RESPONSE_BYTES)).read_line(&mut response))
        .map_err(|e| fail(&e))?;
    serde_json::from_str(&response)
        .map(Some)
        .map_err(|e| fail(&format!("bad response: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> Status {
        Status::of("127.0.0.1:1".parse().unwrap())
    }

    #[test]
    fn socket_sits_beside_the_database() {
        assert_eq!(
            socket_path(Path::new("/d/scree.db")),
            PathBuf::from("/d/scree.db.sock")
        );
    }

    #[test]
    fn status_names_an_address_a_local_browser_can_open() {
        for (bound, url) in [
            ("127.0.0.1:80", "http://127.0.0.1:80"),
            ("0.0.0.0:80", "http://127.0.0.1:80"),
            ("[::]:80", "http://[::1]:80"),
            ("[::1]:80", "http://[::1]:80"),
            ("192.0.2.1:80", "http://192.0.2.1:80"),
        ] {
            let s = Status::of(bound.parse().unwrap());
            assert_eq!((s.port, s.url.as_str()), (80, url), "{bound}");
        }
    }

    #[test]
    fn no_socket_and_a_stale_socket_mean_no_server() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("s.db");
        assert_eq!(query(&db).unwrap(), None);
        drop(UnixListener::bind(socket_path(&db)).unwrap());
        assert!(socket_path(&db).exists());
        assert_eq!(query(&db).unwrap(), None);
        // A new server replaces it, and removes its own on the way out.
        let claim = Claim::acquire(&db).unwrap();
        claim.serve(status()).unwrap();
        assert_eq!(query(&db).unwrap(), Some(status()));
        drop(claim);
        assert!(!socket_path(&db).exists());
    }

    #[test]
    fn a_second_claim_is_refused_until_the_first_drops() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("s.db");
        let first = Claim::acquire(&db).unwrap();
        first.serve(status()).unwrap();
        let Err(e) = Claim::acquire(&db) else {
            panic!("second claim succeeded");
        };
        assert_eq!(e.exit_code(), 1);
        assert!(
            e.to_string()
                .contains("at http://127.0.0.1:1 is already serving"),
            "{e}"
        );
        drop(first);
        Claim::acquire(&db).unwrap();
    }

    #[test]
    fn a_regular_file_at_the_socket_path_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("s.db");
        std::fs::write(socket_path(&db), "keep").unwrap();
        let claim = Claim::acquire(&db).unwrap();
        assert!(claim.serve(status()).is_err());
        drop(claim);
        assert_eq!(std::fs::read_to_string(socket_path(&db)).unwrap(), "keep");
    }

    #[test]
    fn an_unknown_request_is_answered_with_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("s.db");
        let claim = Claim::acquire(&db).unwrap();
        claim.serve(status()).unwrap();
        let mut s = UnixStream::connect(socket_path(&db)).unwrap();
        s.write_all(b"nonsense\n").unwrap();
        let mut r = String::new();
        s.read_to_string(&mut r).unwrap();
        assert_eq!(r, "{\"error\":\"unknown request\"}\n");
    }
}
