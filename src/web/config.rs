//! `scree web` listen configuration: loopback only.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub addr: SocketAddr,
}

impl Config {
    /// `localhost` means 127.0.0.1. Any address that is not loopback is refused
    /// as a usage error: the server can modify the database, and the guard
    /// assumes only this machine can reach it.
    pub fn parse(bind: &str, port: u16) -> Result<Self> {
        let ip: IpAddr = if bind == "localhost" {
            Ipv4Addr::LOCALHOST.into()
        } else {
            bind.parse()
                .map_err(|_| Error::Usage(format!("--bind: {bind:?} is not an IP address")))?
        };
        if !ip.is_loopback() {
            return Err(Error::Usage(format!(
                "--bind {bind}: scree web listens on loopback only (127.0.0.1 or ::1)"
            )));
        }
        Ok(Config {
            addr: SocketAddr::new(ip, port),
        })
    }
}

/// The only `Host` header values the server answers, for the bound `port`
/// (the real one after a port-0 bind).
pub fn allowed_hosts(port: u16) -> Vec<String> {
    vec![
        format!("127.0.0.1:{port}"),
        format!("[::1]:{port}"),
        format!("localhost:{port}"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_binds_parse() {
        assert_eq!(
            Config::parse("127.0.0.1", 80).unwrap().addr.to_string(),
            "127.0.0.1:80"
        );
        assert_eq!(
            Config::parse("localhost", 80).unwrap().addr.to_string(),
            "127.0.0.1:80"
        );
        assert_eq!(
            Config::parse("::1", 80).unwrap().addr.to_string(),
            "[::1]:80"
        );
    }

    #[test]
    fn non_loopback_and_garbage_are_usage_errors() {
        for b in ["0.0.0.0", "192.0.2.1", "::", "example.com", ""] {
            let e = Config::parse(b, 80).unwrap_err();
            assert_eq!(e.exit_code(), 2, "{b}");
        }
    }

    #[test]
    fn hosts_cover_all_three_spellings() {
        assert_eq!(allowed_hosts(9), ["127.0.0.1:9", "[::1]:9", "localhost:9"]);
    }
}
