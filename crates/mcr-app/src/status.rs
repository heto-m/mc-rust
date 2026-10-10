//! Server list ping over the status protocol. Everything in the response is hostile.

use std::time::{Duration, Instant};

use mcr_net::{ConnectOptions, Connection, Event, ServerAddress};
use mcr_protocol::PacketIds;
use mcr_protocol::dispatch::{StatusClientbound, StatusServerbound};
use mcr_protocol::handshake::{Intent, Intention};
use mcr_protocol::types::text::{Color, Component, Content, Style};
use mcr_ui::FAVICON_SIZE;
use serde_json::Value;

const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_DEPTH: usize = 8;
const MAX_NODES: usize = 256;
const MAX_TEXT: usize = 512;
const MAX_FAVICON_B64: usize = 24 * 1024;
const MAX_FAVICON_PX: usize = 64 * 64 * 4;

#[derive(Debug, Clone, PartialEq)]
pub struct PingResult {
    pub motd: Component,
    pub online: i64,
    pub max: i64,
    pub version: String,
    pub protocol: i64,
    pub latency_ms: u32,
    pub favicon: Option<Vec<u8>>,
}

#[derive(Debug, thiserror::Error)]
pub enum PingError {
    #[error("{0}")]
    Net(String),
    #[error("invalid status response")]
    Invalid,
    #[error("timed out")]
    Timeout,
}

fn net<E: std::fmt::Display>(e: E) -> PingError {
    PingError::Net(e.to_string().chars().take(MAX_TEXT).collect())
}

pub fn ping(address: &str) -> Result<PingResult, PingError> {
    let addr = ServerAddress::parse(address).map_err(net)?;
    let (host, port) = addr.resolve();
    let ids = PacketIds::v26_3();
    let conn = Connection::connect_with(
        (host.as_str(), port),
        ConnectOptions {
            inbound_hook: None,
            connect_timeout: Some(TIMEOUT),
        },
    )
    .map_err(net)?;
    let send = |packet: &dyn Fn(&mut Vec<u8>) -> i32| -> Result<(), PingError> {
        let mut payload = Vec::new();
        let id = packet(&mut payload);
        conn.send(id, &payload).map_err(net)
    };
    send(&|out| {
        mcr_protocol::dispatch::HandshakeServerbound::Intention(Intention {
            protocol_version: mcr_data::v26_3().protocol() as i32,
            host: addr.host.chars().take(255).collect(),
            port,
            intent: Intent::Status,
        })
        .encode_into(ids, out)
    })?;
    send(&|out| {
        StatusServerbound::StatusRequest(mcr_protocol::status::StatusRequest).encode_into(ids, out)
    })?;
    let deadline = Instant::now() + TIMEOUT;
    let mut status: Option<Value> = None;
    let mut sent_at = Instant::now();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(PingError::Timeout);
        }
        let frame = match conn.recv_timeout(left) {
            Some(Event::Frame(f)) => f,
            Some(Event::Closed(e)) => return Err(net(e)),
            None => return Err(PingError::Timeout),
        };
        match StatusClientbound::decode(frame.id, &frame.payload, ids).map_err(net)? {
            StatusClientbound::StatusResponse(r) => {
                status = Some(serde_json::from_str(&r.json).map_err(|_| PingError::Invalid)?);
                sent_at = Instant::now();
                send(&|out| {
                    StatusServerbound::PingRequest(mcr_protocol::status::PingRequest { time: 1 })
                        .encode_into(ids, out)
                })?;
            }
            StatusClientbound::PongResponse(_) => {
                let latency = u32::try_from(sent_at.elapsed().as_millis()).unwrap_or(u32::MAX);
                return parse_status(&status.ok_or(PingError::Invalid)?, latency);
            }
        }
    }
}

pub fn parse_status(json: &Value, latency_ms: u32) -> Result<PingResult, PingError> {
    let obj = json.as_object().ok_or(PingError::Invalid)?;
    let players = obj.get("players");
    let int = |v: Option<&Value>| v.and_then(Value::as_i64).unwrap_or(0).clamp(0, 1 << 31);
    let version = obj.get("version");
    let name = version
        .and_then(|v| v.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("?");
    Ok(PingResult {
        motd: obj
            .get("description")
            .map(component_from_json)
            .unwrap_or_else(|| Component::text("")),
        online: int(players.and_then(|p| p.get("online"))),
        max: int(players.and_then(|p| p.get("max"))),
        version: sanitize(name),
        protocol: version
            .and_then(|v| v.get("protocol"))
            .and_then(Value::as_i64)
            .unwrap_or(-1),
        latency_ms,
        favicon: obj
            .get("favicon")
            .and_then(Value::as_str)
            .and_then(decode_favicon),
    })
}

fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT)
        .collect()
}

/// Click and hover events are dropped; depth, node count and text length are capped.
pub fn component_from_json(value: &Value) -> Component {
    let mut budget = MAX_NODES;
    build(value, 0, &mut budget)
}

fn build(value: &Value, depth: usize, budget: &mut usize) -> Component {
    if depth > MAX_DEPTH || *budget == 0 {
        return Component::text("");
    }
    *budget -= 1;
    match value {
        Value::String(s) => Component::text(sanitize(s)),
        Value::Array(items) => {
            let mut iter = items.iter();
            let mut first = iter
                .next()
                .map_or_else(|| Component::text(""), |v| build(v, depth + 1, budget));
            for v in iter {
                first.extra.push(build(v, depth + 1, budget));
            }
            first
        }
        Value::Object(o) => {
            let text = o
                .get("text")
                .and_then(Value::as_str)
                .map(sanitize)
                .or_else(|| o.get("translate").and_then(Value::as_str).map(sanitize))
                .unwrap_or_default();
            let mut c = Component::from_content(Content::Text(text));
            let flag = |k: &str| o.get(k).and_then(Value::as_bool);
            c.style = Style {
                color: o
                    .get("color")
                    .and_then(Value::as_str)
                    .and_then(Color::parse),
                bold: flag("bold"),
                italic: flag("italic"),
                underlined: flag("underlined"),
                strikethrough: flag("strikethrough"),
                obfuscated: flag("obfuscated"),
                ..Style::default()
            };
            if let Some(Value::Array(extra)) = o.get("extra") {
                for v in extra.iter().take(MAX_NODES) {
                    c.extra.push(build(v, depth + 1, budget));
                }
            }
            c
        }
        _ => Component::text(""),
    }
}

fn base64_decode(input: &str, limit: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in input.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
            if out.len() > limit {
                return None;
            }
        }
    }
    Some(out)
}

pub fn decode_favicon(data_uri: &str) -> Option<Vec<u8>> {
    let b64 = data_uri.strip_prefix("data:image/png;base64,")?;
    if b64.len() > MAX_FAVICON_B64 {
        return None;
    }
    decode_png64(&base64_decode(b64, MAX_FAVICON_B64)?)
}

/// Decodes a PNG that must be exactly 64x64 into RGBA.
pub fn decode_png64(png_bytes: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    if info.width != FAVICON_SIZE || info.height != FAVICON_SIZE {
        return None;
    }
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..frame.buffer_size()];
    let rgba = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == MAX_FAVICON_PX).then_some(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_status_and_drops_events() {
        let json: Value = serde_json::from_str(
            r#"{"version":{"name":"26.3","protocol":777},"players":{"online":2,"max":20},
            "description":{"text":"Hi","color":"red","clickEvent":{"action":"open_url","value":"http://x"},"extra":["!"]}}"#,
        )
        .unwrap();
        let r = parse_status(&json, 12).unwrap();
        assert_eq!((r.online, r.max, r.protocol), (2, 20, 777));
        assert_eq!(r.motd.to_plain_string(), "Hi!");
        assert!(r.motd.style.click_event.is_none());
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let mut v = Value::String("x".into());
        for _ in 0..10_000 {
            v = Value::Array(vec![v]);
        }
        let c = component_from_json(&v);
        assert!(c.to_plain_string().len() < 1000);
    }

    #[test]
    fn bad_favicons_are_dropped() {
        assert!(decode_favicon("data:image/png;base64,!!!").is_none());
        assert!(
            decode_favicon(&format!("data:image/png;base64,{}", "A".repeat(100_000))).is_none()
        );
        assert!(decode_favicon("nope").is_none());
    }

    #[test]
    fn base64_round_trip() {
        assert_eq!(base64_decode("aGVsbG8=", 100).unwrap(), b"hello");
    }
}
