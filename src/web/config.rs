//! `scree web` listen configuration: loopback by default, a network address
//! only with `--i-know-this-is-open`.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub addr: SocketAddr,
    /// True for a non-loopback bind: the server warns and every page carries
    /// the open-to-the-network banner.
    pub open: bool,
}

impl Config {
    /// `localhost` means 127.0.0.1. An address that is not loopback is a usage
    /// error unless `open` (`--i-know-this-is-open`) is set: the server can
    /// modify the database and has no login. The flag is ignored for loopback.
    pub fn parse(bind: &str, port: u16, open: bool) -> Result<Self> {
        let ip: IpAddr = if bind == "localhost" {
            Ipv4Addr::LOCALHOST.into()
        } else {
            bind.parse()
                .map_err(|_| Error::Usage(format!("--bind: {bind:?} is not an IP address")))?
        };
        let network = !ip.is_loopback();
        if network && !open {
            return Err(Error::Usage(format!(
                "--bind {bind} is not loopback: anyone on the network could read and change \
                 tickets with no login; pass --i-know-this-is-open to allow it"
            )));
        }
        Ok(Config {
            addr: SocketAddr::new(ip, port),
            open: network,
        })
    }
}

/// Which `Host` header values the server answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostRule {
    /// Exactly these `host:port` spellings.
    Only(Vec<String>),
    /// Any `Host` (a wildcard bind has no single name to check). DNS rebinding
    /// protection does not apply; `Origin` must still match `Host`.
    Any,
}

/// The `Host` rule for a server bound to `ip`, with the real `port` (the one
/// after a port-0 bind). Loopback keeps its three spellings; a specific
/// network address accepts only itself; a wildcard address accepts any Host.
pub fn host_rule(ip: IpAddr, port: u16) -> HostRule {
    if ip.is_unspecified() {
        HostRule::Any
    } else if ip.is_loopback() {
        HostRule::Only(vec![
            format!("127.0.0.1:{port}"),
            format!("[::1]:{port}"),
            format!("localhost:{port}"),
        ])
    } else {
        HostRule::Only(vec![SocketAddr::new(ip, port).to_string()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_binds_parse() {
        for flag in [false, true] {
            for (b, want) in [
                ("127.0.0.1", "127.0.0.1:80"),
                ("localhost", "127.0.0.1:80"),
                ("::1", "[::1]:80"),
            ] {
                let c = Config::parse(b, 80, flag).unwrap();
                assert_eq!(c.addr.to_string(), want);
                assert!(!c.open, "{b}");
            }
        }
    }

    #[test]
    fn non_loopback_needs_the_flag_and_names_it() {
        for b in ["0.0.0.0", "192.0.2.1", "::"] {
            let e = Config::parse(b, 80, false).unwrap_err();
            assert_eq!(e.exit_code(), 2, "{b}");
            assert!(e.to_string().contains("--i-know-this-is-open"), "{e}");
            assert!(e.to_string().contains("loopback"), "{e}");
            assert!(Config::parse(b, 80, true).unwrap().open, "{b}");
        }
    }

    #[test]
    fn garbage_is_a_usage_error_even_with_the_flag() {
        for b in ["example.com", ""] {
            for flag in [false, true] {
                assert_eq!(
                    Config::parse(b, 80, flag).unwrap_err().exit_code(),
                    2,
                    "{b}"
                );
            }
        }
    }

    #[test]
    fn host_rules() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(
            host_rule(ip("127.0.0.1"), 9),
            HostRule::Only(vec![
                "127.0.0.1:9".into(),
                "[::1]:9".into(),
                "localhost:9".into()
            ])
        );
        assert_eq!(
            host_rule(ip("192.0.2.1"), 9),
            HostRule::Only(vec!["192.0.2.1:9".into()])
        );
        assert_eq!(
            host_rule(ip("2001:db8::1"), 9),
            HostRule::Only(vec!["[2001:db8::1]:9".into()])
        );
        assert_eq!(host_rule(ip("0.0.0.0"), 9), HostRule::Any);
        assert_eq!(host_rule(ip("::"), 9), HostRule::Any);
    }
}
