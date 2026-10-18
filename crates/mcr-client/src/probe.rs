//! Learns a server's version from its status response before joining.

use std::time::{Duration, Instant};

use mcr_net::{ConnectOptions, Connection, Event};
use mcr_protocol::PacketIds;
use mcr_protocol::dispatch::{HandshakeServerbound, StatusClientbound, StatusServerbound};
use mcr_protocol::handshake::{Intent, Intention};
use mcr_protocol::status::StatusRequest;
use mcr_protocol::version::{self, Wire};

use crate::error::{ClientError, Result};

const MAX_NAME: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerVersion {
    pub name: String,
    pub protocol: i32,
}

impl ServerVersion {
    /// Reads `version.name` and `version.protocol` from a status response.
    pub fn from_status_json(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let version = value.get("version")?;
        let protocol = i32::try_from(version.get("protocol")?.as_i64()?).ok()?;
        let name = version
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("?")
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_NAME)
            .collect();
        Some(Self { name, protocol })
    }

    /// The supported wire for this version.
    pub fn wire(&self) -> Result<Wire> {
        Wire::for_protocol(self.protocol).ok_or_else(|| ClientError::UnsupportedServer {
            version: self.name.clone(),
            protocol: self.protocol,
        })
    }
}

/// Pings `host:port` with the status protocol and returns the version it reports.
pub fn probe(host: &str, port: u16, timeout: Duration) -> Result<ServerVersion> {
    let ids = PacketIds::v26_3();
    let conn = Connection::connect_with(
        (host, port),
        ConnectOptions {
            connect_timeout: Some(timeout),
            ..Default::default()
        },
    )?;
    let mut payload = Vec::new();
    let id = HandshakeServerbound::Intention(Intention {
        protocol_version: version::PROTOCOL,
        host: host.chars().take(255).collect(),
        port,
        intent: Intent::Status,
    })
    .encode_into(ids, &mut payload);
    conn.send(id, &payload)?;
    payload.clear();
    let id = StatusServerbound::StatusRequest(StatusRequest).encode_into(ids, &mut payload);
    conn.send(id, &payload)?;
    let deadline = Instant::now() + timeout;
    let result = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break Err(ClientError::Timeout("reading the server's status"));
        }
        match conn.recv_timeout(left) {
            Some(Event::Frame(f)) => {
                if let StatusClientbound::StatusResponse(r) =
                    StatusClientbound::decode(f.id, &f.payload, ids)?
                {
                    break ServerVersion::from_status_json(&r.json).ok_or_else(|| {
                        ClientError::Invalid("status response has no version".into())
                    });
                }
            }
            Some(Event::Closed(e)) => break Err(ClientError::Connection(e.to_string())),
            None => break Err(ClientError::Timeout("reading the server's status")),
        }
    };
    conn.close();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_from_status() {
        let v = ServerVersion::from_status_json(
            r#"{"version":{"name":"26.2\u0007","protocol":776},"players":{"max":1,"online":0}}"#,
        )
        .unwrap();
        assert_eq!(
            v,
            ServerVersion {
                name: "26.2".into(),
                protocol: 776
            }
        );
        assert!(ServerVersion::from_status_json(r#"{"version":{"name":"x"}}"#).is_none());
        assert!(
            ServerVersion::from_status_json(r#"{"version":{"protocol":99999999999}}"#).is_none()
        );
        assert!(ServerVersion::from_status_json("[").is_none());
        let unsupported = ServerVersion {
            name: "1.8".into(),
            protocol: 47,
        };
        assert!(matches!(
            unsupported.wire(),
            Err(ClientError::UnsupportedServer { protocol: 47, .. })
        ));
        let canonical = ServerVersion {
            name: "26.3".into(),
            protocol: 777,
        };
        assert!(canonical.wire().unwrap().is_canonical());
    }

    #[test]
    fn mod_mode_refuses_older_servers_before_connecting() {
        let mut config = crate::ClientConfig::default().with_addr("127.0.0.1:1");
        config.wire = crate::WireChoice::Protocol(776);
        let hooks = crate::ClientHooks {
            frame_tap: Some(Box::new(|_, _| {})),
            relay: None,
        };
        let err = crate::Client::connect_hooked(config, hooks).unwrap_err();
        if version::is_supported("26.2") {
            assert!(matches!(&err, ClientError::ModModeNeedsCanonical(v) if v == "26.2"));
            assert_eq!(
                err.to_string(),
                "Mod mode needs a 26.3 server (this one runs 26.2)"
            );
        } else {
            assert!(matches!(
                err,
                ClientError::UnsupportedServer { protocol: 776, .. }
            ));
        }
    }
}
