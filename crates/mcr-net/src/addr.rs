use crate::dns;
use std::net::IpAddr;

pub const DEFAULT_PORT: u16 = 25565;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerAddress {
    pub host: String,
    pub port: u16,
    pub explicit_port: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid server address {0:?}")]
pub struct AddressError(pub String);

impl ServerAddress {
    pub fn parse(input: &str) -> Result<Self, AddressError> {
        let bad = || AddressError(input.to_owned());
        let s = input.trim();
        let (host, port) = if let Some(rest) = s.strip_prefix('[') {
            let (host, tail) = rest.split_once(']').ok_or_else(bad)?;
            match tail {
                "" => (host, None),
                _ => (host, Some(tail.strip_prefix(':').ok_or_else(bad)?)),
            }
        } else if s.matches(':').count() > 1 {
            (s, None)
        } else {
            match s.split_once(':') {
                Some((h, p)) => (h, Some(p)),
                None => (s, None),
            }
        };
        if host.is_empty() {
            return Err(bad());
        }
        let port = port
            .map(|p| p.parse::<u16>().map_err(|_| bad()))
            .transpose()?;
        Ok(Self {
            host: host.to_owned(),
            port: port.unwrap_or(DEFAULT_PORT),
            explicit_port: port.is_some(),
        })
    }

    /// Follows `_minecraft._tcp` SRV when no port was given; any failure keeps the original.
    pub fn resolve(&self) -> (String, u16) {
        if !self.explicit_port && self.host.parse::<IpAddr>().is_err() {
            let name = format!("_minecraft._tcp.{}", self.host);
            if let Some(srv) = dns::lookup_srv(&name, &dns::nameservers()) {
                return (srv.target, srv.port);
            }
        }
        (self.host.clone(), self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> (String, u16, bool) {
        let a = ServerAddress::parse(s).unwrap();
        (a.host, a.port, a.explicit_port)
    }

    #[test]
    fn parses_forms() {
        assert_eq!(p("example.com"), ("example.com".into(), 25565, false));
        assert_eq!(p("example.com:1234"), ("example.com".into(), 1234, true));
        assert_eq!(p("[::1]"), ("::1".into(), 25565, false));
        assert_eq!(p("[::1]:99"), ("::1".into(), 99, true));
        assert_eq!(p("::1"), ("::1".into(), 25565, false));
        assert_eq!(p("127.0.0.1:25566"), ("127.0.0.1".into(), 25566, true));
    }

    #[test]
    fn rejects_bad() {
        for s in [
            "",
            ":25565",
            "host:",
            "host:70000",
            "host:abc",
            "[::1",
            "[::1]x",
        ] {
            assert!(ServerAddress::parse(s).is_err(), "{s}");
        }
    }

    #[test]
    fn explicit_port_and_ip_skip_srv() {
        let a = ServerAddress::parse("127.0.0.1").unwrap();
        assert_eq!(a.resolve(), ("127.0.0.1".into(), 25565));
        let a = ServerAddress::parse("example.invalid:1").unwrap();
        assert_eq!(a.resolve(), ("example.invalid".into(), 1));
    }
}
