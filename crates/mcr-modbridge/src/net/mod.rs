//! The packet bridge: Rust owns the socket; Java's `Connection` runs on a copy of the stream.

mod clientbound;
mod filter;
mod phase;
mod relay;
mod wire;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mcr_client::{ClientHooks, LocalAction};
use mcr_jvm::{Ring, RingError, RingReader};
use mcr_protocol::{Direction, PacketIds};

pub use clientbound::DESYNC_REASON;
pub use filter::{FORWARD, ForwardTable, Verdict};
pub use phase::{NetPhase, PhaseTracker, TransitionIds};
pub use relay::NetRelay;
pub use wire::{MAX_PROBE_POSITIONS, PlayerState, ProbeRequest, WorldProbe};

use crate::abi::{
    ACTION_SIZE, NET_ACTION, NET_HEADER, NET_OPEN, NET_PLAYER, NET_PROBE, OPEN_HOST, OPEN_PORT,
    PLAYER_SIZE,
};
use clientbound::{Clientbound, Placed};
use wire::put_header;

/// Bound on what queues behind a stalled Java before its side is disconnected.
pub const SPILL_LIMIT: usize = 256 << 20;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Debug, Default)]
struct Counters {
    frames_copied: AtomicU64,
    bytes_copied: AtomicU64,
    frames_spilled: AtomicU64,
    frames_hidden: AtomicU64,
    desyncs: AtomicU64,
    forwarded: AtomicU64,
    chats: AtomicU64,
    stale: AtomicU64,
    malformed: AtomicU64,
}

/// A snapshot of the bridge's traffic counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetStats {
    pub frames_copied: u64,
    pub bytes_copied: u64,
    pub frames_spilled: u64,
    /// Login encryption and compression packets Java never sees.
    pub frames_hidden: u64,
    pub spill_peak_bytes: u64,
    pub desyncs: u64,
    pub forwarded: u64,
    pub chats: u64,
    /// Java messages from a session that is no longer current.
    pub stale: u64,
    pub malformed: u64,
    /// Java's serverbound frames Rust dropped, by (phase tag, packet name).
    pub dropped: BTreeMap<(u8, String), u64>,
}

/// Process-wide because the JVM is: both rings live as long as the process.
#[derive(Debug)]
pub struct NetShared {
    inbound: Arc<Ring>,
    outbound: Arc<Ring>,
    tx: Mutex<Clientbound>,
    rx: Mutex<RingReader>,
    next_session: AtomicU32,
    next_probe: AtomicU32,
    detached: AtomicBool,
    counters: Counters,
    dropped: Mutex<BTreeMap<(u8, i32), u64>>,
    probe: Mutex<Option<WorldProbe>>,
}

impl NetShared {
    pub(crate) fn new(
        inbound_capacity: usize,
        outbound_capacity: usize,
        spill_limit: usize,
    ) -> Result<Self, RingError> {
        let inbound = Ring::new(inbound_capacity)?;
        let outbound = Ring::new(outbound_capacity)?;
        let writer = inbound.writer().expect("fresh ring has a writer");
        let reader = outbound.reader().expect("fresh ring has a reader");
        Ok(Self {
            inbound,
            outbound,
            tx: Mutex::new(Clientbound::new(writer, spill_limit)),
            rx: Mutex::new(reader),
            next_session: AtomicU32::new(1),
            next_probe: AtomicU32::new(1),
            detached: AtomicBool::new(false),
            counters: Counters::default(),
            dropped: Mutex::new(BTreeMap::new()),
            probe: Mutex::new(None),
        })
    }

    pub(crate) fn inbound_addr(&self) -> i64 {
        self.inbound.addr()
    }

    pub(crate) fn outbound_addr(&self) -> i64 {
        self.outbound.addr()
    }

    /// Java crashed or its VM ended: stop copying and forwarding for good.
    pub(crate) fn detach(&self) {
        if !self.detached.swap(true, Ordering::AcqRel) {
            tracing::warn!("java side detached; the net bridge stops forwarding");
            lock(&self.tx).clear();
        }
    }

    pub fn is_detached(&self) -> bool {
        self.detached.load(Ordering::Acquire)
    }

    /// Runs on the pacer thread every frame so a spill drains even without new packets.
    pub(crate) fn flush_spill(&self) {
        if !self.is_detached() {
            lock(&self.tx).flush();
        }
    }

    fn push_frame(&self, session: u32, phase: NetPhase, id: i32, payload: &[u8]) {
        let placed = lock(&self.tx).push_frame(session, phase, id, payload);
        let c = &self.counters;
        match placed {
            Placed::Ring | Placed::Spilled => {
                c.frames_copied.fetch_add(1, Ordering::Relaxed);
                c.bytes_copied
                    .fetch_add(payload.len() as u64, Ordering::Relaxed);
                if placed == Placed::Spilled {
                    c.frames_spilled.fetch_add(1, Ordering::Relaxed);
                }
            }
            Placed::Desynced => {
                c.desyncs.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    session,
                    "java fell behind the server; disconnecting its side"
                );
            }
            Placed::Dropped => {}
        }
    }

    fn push_message(
        &self,
        kind: u16,
        session: u32,
        len: usize,
        force: bool,
        fill: &dyn Fn(&mut [u8]),
    ) {
        if !self.is_detached() {
            lock(&self.tx).push(kind, session, len, force, fill);
        }
    }

    fn open(&self, host: &str, port: u16) -> u32 {
        let session = self.next_session.fetch_add(1, Ordering::Relaxed);
        let host = &host.as_bytes()[..host.len().min(255)];
        let fill = |buf: &mut [u8]| {
            put_header(buf, 0, session);
            buf[OPEN_PORT..OPEN_PORT + 4].copy_from_slice(&u32::from(port).to_ne_bytes());
            buf[OPEN_HOST..].copy_from_slice(host);
        };
        self.push_message(NET_OPEN, session, OPEN_HOST + host.len(), true, &fill);
        session
    }

    fn close(&self, session: u32, reason: &str) {
        if !self.is_detached() {
            lock(&self.tx).push_close(session, reason);
        }
    }

    fn post_player(&self, session: u32, state: &PlayerState) {
        let fill = |buf: &mut [u8]| {
            put_header(buf, 0, session);
            state.encode(buf);
        };
        self.push_message(NET_PLAYER, session, PLAYER_SIZE, false, &fill);
    }

    fn post_action(&self, session: u32, action: LocalAction) {
        let fill = |buf: &mut [u8]| {
            put_header(buf, 0, session);
            wire::encode_action(action, buf);
        };
        self.push_message(NET_ACTION, session, ACTION_SIZE, false, &fill);
    }

    fn count_drop(&self, tag: u8, id: i32) {
        *lock(&self.dropped).entry((tag, id)).or_default() += 1;
    }

    fn stats(&self) -> NetStats {
        let c = &self.counters;
        let get = |a: &AtomicU64| a.load(Ordering::Relaxed);
        let ids = PacketIds::v26_3();
        let dropped = lock(&self.dropped)
            .iter()
            .map(|(&(tag, id), &n)| {
                let name = NetPhase::from_tag(tag)
                    .and_then(|p| ids.name_by_id(p.state(), Direction::Serverbound, id))
                    .map_or_else(|| format!("#{id}"), str::to_owned);
                ((tag, name), n)
            })
            .collect();
        NetStats {
            frames_copied: get(&c.frames_copied),
            bytes_copied: get(&c.bytes_copied),
            frames_spilled: get(&c.frames_spilled),
            frames_hidden: get(&c.frames_hidden),
            spill_peak_bytes: lock(&self.tx).spill_peak as u64,
            desyncs: get(&c.desyncs),
            forwarded: get(&c.forwarded),
            chats: get(&c.chats),
            stale: get(&c.stale),
            malformed: get(&c.malformed),
            dropped,
        }
    }
}

/// The game-side handle to the packet bridge.
#[derive(Debug, Clone, Copy)]
pub struct NetLink {
    shared: &'static NetShared,
}

impl NetLink {
    pub(crate) fn new(shared: &'static NetShared) -> Self {
        Self { shared }
    }

    /// A link to no JVM, with Java's end of the clientbound ring handed back; for benches and tests.
    pub fn standalone(inbound_capacity: usize) -> Result<(Self, RingReader), RingError> {
        let shared: &'static NetShared = Box::leak(Box::new(NetShared::new(
            inbound_capacity,
            1 << 16,
            SPILL_LIMIT,
        )?));
        let java = shared.inbound.reader().expect("fresh ring has a reader");
        Ok((Self::new(shared), java))
    }

    /// Hooks for one connection: Java opens a matching `ConnectScreen` (or binds the one it has)
    /// and follows the stream from the first frame.
    pub fn hooks(&self, host: &str, port: u16) -> ClientHooks {
        let shared = self.shared;
        let session = shared.open(host, port);
        let mut tracker = PhaseTracker::new(*TransitionIds::v26_3());
        let tap = move |id: i32, payload: &[u8]| {
            let phase = tracker.observe(id);
            if shared.is_detached() {
                return;
            }
            if tracker.hidden_from_java(phase, id) {
                shared
                    .counters
                    .frames_hidden
                    .fetch_add(1, Ordering::Relaxed);
                return;
            }
            shared.push_frame(session, phase, id, payload);
        };
        ClientHooks {
            frame_tap: Some(Box::new(tap)),
            relay: Some(Box::new(NetRelay::new(shared, session))),
        }
    }

    /// Returns the request id; the answer arrives through [`take_probe`](Self::take_probe) once a
    /// relay has polled it.
    pub fn request_probe(&self, request: &ProbeRequest) -> u32 {
        let id = self.shared.next_probe.fetch_add(1, Ordering::Relaxed);
        let fill = |buf: &mut [u8]| {
            put_header(buf, 0, 0);
            request.encode(id, buf);
        };
        self.shared
            .push_message(NET_PROBE, 0, request.len(), false, &fill);
        id
    }

    pub fn take_probe(&self) -> Option<WorldProbe> {
        lock(&self.shared.probe).take()
    }

    pub fn stats(&self) -> NetStats {
        self.shared.stats()
    }

    pub fn is_detached(&self) -> bool {
        self.shared.is_detached()
    }

    pub fn spilled_bytes(&self) -> usize {
        lock(&self.shared.tx).spilled_bytes()
    }
}

const _: () = assert!(NET_HEADER == 8);

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::abi::{NET_CLOSE, NET_FRAME, NET_SESSION, NET_TAG};
    use mcr_jvm::RingReader;

    pub fn link(spill_limit: usize) -> (NetLink, RingReader, mcr_jvm::RingWriter) {
        let shared: &'static NetShared = Box::leak(Box::new(
            NetShared::new(1 << 16, 1 << 16, spill_limit).unwrap(),
        ));
        let java_in = shared.inbound.reader().unwrap();
        let java_out = shared.outbound.writer().unwrap();
        (NetLink::new(shared), java_in, java_out)
    }

    pub fn session_of(p: &[u8]) -> u32 {
        u32::from_ne_bytes(p[NET_SESSION..NET_SESSION + 4].try_into().unwrap())
    }

    fn cb_id(state: mcr_protocol::ProtocolState, name: &str) -> i32 {
        PacketIds::v26_3()
            .id_by_name(state, Direction::Clientbound, name)
            .unwrap()
    }

    #[test]
    fn tap_tags_phases_hides_login_crypto_and_opens_first() {
        use mcr_protocol::ProtocolState::{Configuration, Login, Play};
        let (link, mut java, _) = link(1 << 20);
        let mut hooks = link.hooks("example.org", 25566);
        let tap = hooks.frame_tap.as_mut().unwrap();
        for (state, name) in [
            (Login, "hello"),
            (Login, "login_compression"),
            (Login, "login_finished"),
            (Configuration, "finish_configuration"),
            (Play, "keep_alive"),
        ] {
            tap(cb_id(state, name), b"x");
        }
        let mut seen = Vec::new();
        while let Some(m) = java.try_pop().unwrap() {
            seen.push((m.kind(), m.payload().to_vec()));
        }
        assert_eq!(seen[0].0, NET_OPEN);
        let open = &seen[0].1;
        assert_eq!(
            u32::from_ne_bytes(open[OPEN_PORT..OPEN_PORT + 4].try_into().unwrap()),
            25566
        );
        assert_eq!(&open[OPEN_HOST..], b"example.org");
        let session = session_of(open);
        let tags: Vec<u8> = seen[1..]
            .iter()
            .map(|(k, p)| {
                assert_eq!(*k, NET_FRAME);
                assert_eq!(session_of(p), session);
                p[NET_TAG]
            })
            .collect();
        assert_eq!(
            tags,
            [
                NetPhase::Login.tag(),
                NetPhase::Configuration.tag(),
                NetPhase::Play.tag()
            ]
        );
        let stats = link.stats();
        assert_eq!((stats.frames_copied, stats.frames_hidden), (3, 2));
        drop(hooks);
        let close = java.try_pop().unwrap().unwrap();
        assert_eq!(close.kind(), NET_CLOSE);
    }

    #[test]
    fn detached_bridge_copies_nothing() {
        let (link, mut java, _) = link(1 << 20);
        let mut hooks = link.hooks("localhost", 25565);
        while java.try_pop().unwrap().is_some() {}
        link.shared.detach();
        (hooks.frame_tap.as_mut().unwrap())(0, b"late");
        drop(hooks);
        assert!(java.try_pop().unwrap().is_none());
        assert!(link.is_detached());
    }

    #[test]
    fn stalled_java_desyncs_its_side_only() {
        let (link, mut java, _) = link(4096);
        let mut hooks = link.hooks("localhost", 25565);
        let tap = hooks.frame_tap.as_mut().unwrap();
        let login_finished = TransitionIds::v26_3().login_finished;
        tap(login_finished, &[]);
        for _ in 0..200 {
            tap(1, &[7; 512]);
        }
        assert_eq!(link.stats().desyncs, 1);
        let mut last = None;
        for _ in 0..100 {
            while let Some(m) = java.try_pop().unwrap() {
                last = Some((m.kind(), m.payload()[NET_HEADER..].to_vec()));
            }
            link.shared.flush_spill();
        }
        let (kind, reason) = last.unwrap();
        assert_eq!(kind, NET_CLOSE);
        assert_eq!(reason, DESYNC_REASON.as_bytes());
        assert!(link.spilled_bytes() < 4096);
    }
}
