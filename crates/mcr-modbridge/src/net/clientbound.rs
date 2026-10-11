use std::collections::VecDeque;

use mcr_jvm::{PushError, RingWriter};

use super::phase::NetPhase;
use super::wire::{put_header, put_varint};
use crate::abi::{NET_CLOSE, NET_FRAME, NET_HEADER};

pub const DESYNC_REASON: &str = "mcr: the Java client fell behind the server";

#[derive(Debug)]
struct Spilled {
    kind: u16,
    session: u32,
    bytes: Box<[u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placed {
    Ring,
    Spilled,
    Dropped,
    /// The spill limit was hit for this session's frames; Java gets a disconnect.
    Desynced,
}

/// Rust→Java net ring writer. Never blocks: a full ring spills into a bounded queue that drains
/// before anything newer, so Java sees one ordered stream.
#[derive(Debug)]
pub(crate) struct Clientbound {
    writer: RingWriter,
    spill: VecDeque<Spilled>,
    spill_bytes: usize,
    spill_limit: usize,
    pub spill_peak: usize,
    desynced: u32,
}

impl Clientbound {
    pub fn new(writer: RingWriter, spill_limit: usize) -> Self {
        Self {
            writer,
            spill: VecDeque::new(),
            spill_bytes: 0,
            spill_limit,
            spill_peak: 0,
            desynced: 0,
        }
    }

    /// Moves spilled messages into the ring; true once nothing is left behind.
    pub fn flush(&mut self) -> bool {
        while let Some(front) = self.spill.front() {
            let len = front.bytes.len();
            match self.writer.try_push(front.kind, &front.bytes) {
                Ok(()) | Err(PushError::TooLarge { .. } | PushError::ReservedKind) => {
                    self.spill_bytes -= len;
                    self.spill.pop_front();
                }
                Err(PushError::Full) => return false,
            }
        }
        true
    }

    pub fn clear(&mut self) {
        self.spill.clear();
        self.spill_bytes = 0;
    }

    pub fn spilled_bytes(&self) -> usize {
        self.spill_bytes
    }

    /// `force` ignores the spill limit (session open and close must never be lost).
    pub fn push(
        &mut self,
        kind: u16,
        session: u32,
        len: usize,
        force: bool,
        fill: &dyn Fn(&mut [u8]),
    ) -> Placed {
        if self.flush() {
            match self.writer.try_push_with(kind, len, fill) {
                Ok(()) => return Placed::Ring,
                Err(PushError::Full) => {}
                Err(PushError::TooLarge { .. } | PushError::ReservedKind) => {
                    return Placed::Dropped;
                }
            }
        }
        if !force && self.spill_bytes + len > self.spill_limit {
            return Placed::Dropped;
        }
        let mut bytes = vec![0u8; len].into_boxed_slice();
        fill(&mut bytes);
        self.spill_bytes += len;
        self.spill_peak = self.spill_peak.max(self.spill_bytes);
        self.spill.push_back(Spilled {
            kind,
            session,
            bytes,
        });
        Placed::Spilled
    }

    pub fn push_frame(&mut self, session: u32, phase: NetPhase, id: i32, payload: &[u8]) -> Placed {
        if self.desynced == session {
            return Placed::Dropped;
        }
        let mut id_bytes = [0u8; 5];
        let id_len = put_varint(&mut id_bytes, id);
        let len = NET_HEADER + id_len + payload.len();
        let fill = |buf: &mut [u8]| {
            put_header(buf, phase.tag(), session);
            buf[NET_HEADER..NET_HEADER + id_len].copy_from_slice(&id_bytes[..id_len]);
            buf[NET_HEADER + id_len..].copy_from_slice(payload);
        };
        match self.push(NET_FRAME, session, len, false, &fill) {
            Placed::Dropped => {
                self.desync(session);
                Placed::Desynced
            }
            placed => placed,
        }
    }

    fn desync(&mut self, session: u32) {
        self.desynced = session;
        self.spill
            .retain(|s| !(s.session == session && s.kind == NET_FRAME));
        self.spill_bytes = self.spill.iter().map(|s| s.bytes.len()).sum();
        self.push_close(session, DESYNC_REASON);
    }

    pub fn push_close(&mut self, session: u32, reason: &str) -> Placed {
        let reason = &reason.as_bytes()[..reason.len().min(1024)];
        let fill = |buf: &mut [u8]| {
            put_header(buf, 0, session);
            buf[NET_HEADER..].copy_from_slice(reason);
        };
        self.push(NET_CLOSE, session, NET_HEADER + reason.len(), true, &fill)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::{NET_SESSION, NET_TAG};
    use mcr_jvm::Ring;

    fn drain(reader: &mut mcr_jvm::RingReader) -> Vec<(u16, Vec<u8>)> {
        let mut out = Vec::new();
        while let Some(m) = reader.try_pop().unwrap() {
            out.push((m.kind(), m.payload().to_vec()));
        }
        out
    }

    #[test]
    fn frames_carry_phase_session_and_vanilla_bytes() {
        let ring = Ring::new(4096).unwrap();
        let mut reader = ring.reader().unwrap();
        let mut cb = Clientbound::new(ring.writer().unwrap(), 1 << 20);
        assert_eq!(cb.push_frame(9, NetPhase::Play, 300, b"abc"), Placed::Ring);
        let got = drain(&mut reader);
        assert_eq!(got.len(), 1);
        let (kind, p) = &got[0];
        assert_eq!(*kind, NET_FRAME);
        assert_eq!(p[NET_TAG], NetPhase::Play.tag());
        assert_eq!(
            u32::from_ne_bytes(p[NET_SESSION..NET_SESSION + 4].try_into().unwrap()),
            9
        );
        let mut body = &p[NET_HEADER..];
        assert_eq!(mcr_buf::read_varint(&mut body).unwrap(), 300);
        assert_eq!(body, b"abc");
    }

    #[test]
    fn full_ring_spills_in_order_and_drains_later() {
        let ring = Ring::new(1024).unwrap();
        let mut reader = ring.reader().unwrap();
        let mut cb = Clientbound::new(ring.writer().unwrap(), 1 << 20);
        let mut placed = Vec::new();
        for i in 0..40 {
            placed.push(cb.push_frame(1, NetPhase::Play, i, &[i as u8; 60]));
        }
        assert!(placed.contains(&Placed::Spilled));
        assert!(cb.spilled_bytes() > 0);
        let mut ids = Vec::new();
        loop {
            let batch = drain(&mut reader);
            if batch.is_empty() && cb.flush() {
                break;
            }
            for (_, p) in batch {
                let mut body = &p[NET_HEADER..];
                ids.push(mcr_buf::read_varint(&mut body).unwrap());
            }
            cb.flush();
        }
        assert_eq!(ids, (0..40).collect::<Vec<_>>());
        assert_eq!(cb.spilled_bytes(), 0);
    }

    #[test]
    fn spill_limit_desyncs_only_that_session_and_queues_a_close() {
        let ring = Ring::new(1024).unwrap();
        let mut reader = ring.reader().unwrap();
        let mut cb = Clientbound::new(ring.writer().unwrap(), 512);
        let mut outcome = Placed::Ring;
        for i in 0..100 {
            outcome = cb.push_frame(5, NetPhase::Play, i, &[0; 100]);
            if outcome == Placed::Desynced {
                break;
            }
        }
        assert_eq!(outcome, Placed::Desynced);
        assert_eq!(
            cb.push_frame(5, NetPhase::Play, 1, &[0; 10]),
            Placed::Dropped
        );
        let mut kinds = Vec::new();
        for _ in 0..10 {
            kinds.extend(drain(&mut reader));
            cb.flush();
        }
        let (last_kind, last) = kinds.last().unwrap();
        assert_eq!(*last_kind, NET_CLOSE);
        assert_eq!(&last[NET_HEADER..], DESYNC_REASON.as_bytes());
        assert_ne!(cb.push_frame(6, NetPhase::Login, 0, &[]), Placed::Dropped);
    }
}
