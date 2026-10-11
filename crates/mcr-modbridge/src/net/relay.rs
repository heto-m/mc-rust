use std::sync::atomic::Ordering;

use mcr_client::{LocalPlayer, Phase, Relay, Relayed};

use super::filter::{ForwardTable, Verdict};
use super::phase::NetPhase;
use super::wire::{PlayerState, WorldProbe, header};
use super::{NetShared, lock};
use crate::abi::{
    CHAT_COMMAND, CHAT_MESSAGE, NET_HEADER, OUT_CHAT, OUT_DISCONNECT, OUT_FRAME, OUT_PROBE,
};

/// Bounds one poll so a flood from Java cannot stall a game tick.
const MAX_MESSAGES_PER_POLL: usize = 4096;
const MAX_TEXT_BYTES: usize = 32 << 10;

/// Java's side of one connection, polled by `mcr-client` on the game thread.
#[derive(Debug)]
pub struct NetRelay {
    shared: &'static NetShared,
    session: u32,
    reason: String,
    last_pos: Option<[f64; 3]>,
}

impl NetRelay {
    pub(crate) fn new(shared: &'static NetShared, session: u32) -> Self {
        Self {
            shared,
            session,
            reason: "Disconnected".into(),
            last_pos: None,
        }
    }

    pub fn session(&self) -> u32 {
        self.session
    }
}

fn text(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_TEXT_BYTES)])
}

impl Relay for NetRelay {
    fn poll(&mut self, phase: Phase, sink: &mut dyn FnMut(Relayed<'_>)) {
        let shared = self.shared;
        if shared.is_detached() {
            return;
        }
        let rust_phase = NetPhase::of_client(phase);
        let table = ForwardTable::v26_3();
        let c = &shared.counters;
        let mut rx = lock(&shared.rx);
        let mut corrupt = None;
        for _ in 0..MAX_MESSAGES_PER_POLL {
            let msg = match rx.try_pop() {
                Ok(Some(msg)) => msg,
                Ok(None) => break,
                Err(e) => {
                    corrupt = Some(e);
                    break;
                }
            };
            let kind = msg.kind();
            let payload = msg.payload();
            let Some((tag, session)) = header(payload) else {
                c.malformed.fetch_add(1, Ordering::Relaxed);
                continue;
            };
            let body = &payload[NET_HEADER..];
            if kind == OUT_PROBE {
                match WorldProbe::decode(payload) {
                    Some(probe) => *lock(&shared.probe) = Some(probe),
                    None => {
                        c.malformed.fetch_add(1, Ordering::Relaxed);
                    }
                }
                continue;
            }
            if session != self.session {
                c.stale.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            match kind {
                OUT_FRAME => {
                    let mut rest = body;
                    let Ok(id) = mcr_buf::read_varint(&mut rest) else {
                        c.malformed.fetch_add(1, Ordering::Relaxed);
                        continue;
                    };
                    let verdict = match (rust_phase, NetPhase::from_tag(tag)) {
                        (Some(rust), Some(tagged)) => table.verdict(rust, tagged, id, rest),
                        _ => Verdict::WrongPhase,
                    };
                    if verdict == Verdict::Forward {
                        c.forwarded.fetch_add(1, Ordering::Relaxed);
                        sink(Relayed::Frame { id, payload: rest });
                    } else {
                        shared.count_drop(tag, id);
                    }
                }
                OUT_CHAT => {
                    c.chats.fetch_add(1, Ordering::Relaxed);
                    let line = text(body);
                    match tag {
                        CHAT_MESSAGE => sink(Relayed::Chat(&line)),
                        CHAT_COMMAND => sink(Relayed::Command(line.trim_start_matches('/'))),
                        _ => {
                            c.malformed.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                OUT_DISCONNECT => sink(Relayed::Disconnect(&text(body))),
                _ => {
                    c.malformed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
        drop(rx);
        if let Some(e) = corrupt {
            tracing::error!("corrupt serverbound bridge ring: {e}");
            shared.detach();
        }
    }

    fn answers_custom_queries(&self) -> bool {
        !self.shared.is_detached()
    }

    fn tick_end(&mut self, tick: u64, player: &LocalPlayer) {
        let pos = player.physics.pos;
        let old = self.last_pos.replace(pos).unwrap_or(pos);
        let state = PlayerState::from_player(tick, player, old);
        self.shared.post_player(self.session, &state);
    }

    fn closed(&mut self, reason: &str) {
        reason.clone_into(&mut self.reason);
    }
}

impl Drop for NetRelay {
    fn drop(&mut self) {
        self.shared.close(self.session, &self.reason);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{link, session_of};
    use super::*;
    use crate::abi::{NET_CLOSE, NET_PLAYER, PLAYER_POS, PLAYER_TICK};
    use crate::net::wire::put_header;
    use mcr_jvm::RingWriter;
    use mcr_protocol::{Direction, PacketIds};

    fn frame(out: &mut RingWriter, phase: NetPhase, session: u32, id: i32, body: &[u8]) {
        let mut buf = vec![0u8; NET_HEADER];
        put_header(&mut buf, phase.tag(), session);
        mcr_buf::write_varint(&mut buf, id);
        buf.extend_from_slice(body);
        out.try_push(OUT_FRAME, &buf).unwrap();
    }

    fn message(out: &mut RingWriter, kind: u16, tag: u8, session: u32, body: &[u8]) {
        let mut buf = vec![0u8; NET_HEADER];
        put_header(&mut buf, tag, session);
        buf.extend_from_slice(body);
        out.try_push(kind, &buf).unwrap();
    }

    fn sb(phase: NetPhase, name: &str) -> i32 {
        PacketIds::v26_3()
            .id_by_name(phase.state(), Direction::Serverbound, name)
            .unwrap()
    }

    #[derive(Debug, PartialEq)]
    enum Out {
        Frame(i32, Vec<u8>),
        Chat(String),
        Command(String),
        Disconnect(String),
    }

    fn poll(relay: &mut dyn Relay, phase: Phase) -> Vec<Out> {
        let mut out = Vec::new();
        relay.poll(phase, &mut |r| {
            out.push(match r {
                Relayed::Frame { id, payload } => Out::Frame(id, payload.to_vec()),
                Relayed::Chat(t) => Out::Chat(t.into()),
                Relayed::Command(t) => Out::Command(t.into()),
                Relayed::Disconnect(t) => Out::Disconnect(t.into()),
            });
        });
        out
    }

    #[test]
    fn forwards_only_listed_packets_of_the_current_phase_and_session() {
        let (link, mut java_in, mut java_out) = link(1 << 20);
        let mut hooks = link.hooks("localhost", 25565);
        let session = session_of(java_in.try_pop().unwrap().unwrap().payload());
        let mut relay = hooks.relay.take().unwrap();
        let mut custom = Vec::new();
        mcr_buf::write_varint(&mut custom, 17);
        custom.extend_from_slice(b"mcr_testmod:hello");
        custom.extend_from_slice(b"payload");
        let play = NetPhase::Play;
        frame(
            &mut java_out,
            play,
            session,
            sb(play, "custom_payload"),
            &custom,
        );
        frame(
            &mut java_out,
            play,
            session,
            sb(play, "keep_alive"),
            &[0; 8],
        );
        frame(
            &mut java_out,
            play,
            session,
            sb(play, "move_player_pos"),
            &[0; 25],
        );
        frame(
            &mut java_out,
            play,
            session + 99,
            sb(play, "custom_payload"),
            &custom,
        );
        let cfg = NetPhase::Configuration;
        frame(
            &mut java_out,
            cfg,
            session,
            sb(cfg, "custom_payload"),
            &custom,
        );
        message(
            &mut java_out,
            OUT_CHAT,
            CHAT_MESSAGE,
            session,
            b"hello there",
        );
        message(
            &mut java_out,
            OUT_CHAT,
            CHAT_COMMAND,
            session,
            b"/time query day",
        );
        message(&mut java_out, OUT_DISCONNECT, 0, session, b"Quit");
        let got = poll(relay.as_mut(), Phase::Play);
        assert_eq!(
            got,
            [
                Out::Frame(sb(play, "custom_payload"), custom.clone()),
                Out::Chat("hello there".into()),
                Out::Command("time query day".into()),
                Out::Disconnect("Quit".into()),
            ]
        );
        let stats = link.stats();
        assert_eq!((stats.forwarded, stats.stale, stats.chats), (1, 1, 2));
        let dropped: Vec<_> = stats
            .dropped
            .keys()
            .map(|(t, n)| (*t, n.as_str()))
            .collect();
        assert!(dropped.contains(&(play.tag(), "keep_alive")));
        assert!(dropped.contains(&(play.tag(), "move_player_pos")));
        assert!(dropped.contains(&(cfg.tag(), "custom_payload")));
        assert!(relay.answers_custom_queries());
    }

    #[test]
    fn tick_end_posts_player_state_and_drop_closes_the_session() {
        let (link, mut java_in, _) = link(1 << 20);
        let mut hooks = link.hooks("localhost", 25565);
        let session = session_of(java_in.try_pop().unwrap().unwrap().payload());
        let mut relay = hooks.relay.take().unwrap();
        let mut player = LocalPlayer::default();
        player.physics.pos = [1.0, 2.0, 3.0];
        relay.tick_end(41, &player);
        player.physics.pos = [1.5, 2.0, 3.0];
        relay.tick_end(42, &player);
        relay.closed("Connection reset");
        drop(relay);
        let mut seen = Vec::new();
        while let Some(m) = java_in.try_pop().unwrap() {
            seen.push((m.kind(), m.payload().to_vec()));
        }
        assert_eq!(seen.len(), 3);
        let (kind, p) = &seen[1];
        assert_eq!((*kind, session_of(p)), (NET_PLAYER, session));
        let f64_at = |at: usize| f64::from_ne_bytes(p[at..at + 8].try_into().unwrap());
        assert_eq!(
            u64::from_ne_bytes(p[PLAYER_TICK..PLAYER_TICK + 8].try_into().unwrap()),
            42
        );
        assert_eq!(f64_at(PLAYER_POS), 1.5);
        assert_eq!(f64_at(crate::abi::PLAYER_OLD_POS), 1.0);
        assert_eq!(seen[2].0, NET_CLOSE);
        assert_eq!(&seen[2].1[NET_HEADER..], b"Connection reset");
    }

    #[test]
    fn probe_answers_are_kept_for_the_caller() {
        let (link, _java_in, mut java_out) = link(1 << 20);
        let mut hooks = link.hooks("localhost", 25565);
        let mut relay = hooks.relay.take().unwrap();
        let probe = WorldProbe {
            request: 3,
            level: true,
            chunks: vec![(1, 2)],
            ..WorldProbe::default()
        };
        java_out
            .try_push(OUT_PROBE, &crate::net::wire::tests::answer(&probe))
            .unwrap();
        assert!(poll(relay.as_mut(), Phase::Play).is_empty());
        assert_eq!(link.take_probe(), Some(probe));
        assert_eq!(link.take_probe(), None);
    }
}
