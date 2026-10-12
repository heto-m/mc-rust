use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub const VOICE_INITIAL: u8 = 0;
pub const VOICE_PLAYING: u8 = 1;
pub const VOICE_PAUSED: u8 = 2;
pub const VOICE_STOPPED: u8 = 3;

pub const GENERATION_MASK: u32 = 0x00FF_FFFF;

/// `[generation:24][state:8]`, the layout another engine reads through the table's address.
pub fn state_word(generation: u32, state: u8) -> u32 {
    ((generation & GENERATION_MASK) << 8) | u32::from(state)
}

/// One state word per external voice slot, shared lock-free with whoever owns the slots.
#[derive(Debug)]
pub struct VoiceStates {
    words: Box<[AtomicU32]>,
}

impl VoiceStates {
    pub fn new(slots: usize) -> Arc<Self> {
        let words = (0..slots)
            .map(|_| AtomicU32::new(state_word(0, VOICE_STOPPED)))
            .collect();
        Arc::new(Self { words })
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Start of `len()` naturally aligned `u32` words, valid as long as the table lives.
    pub fn as_ptr(&self) -> *const u32 {
        self.words.as_ptr().cast()
    }

    /// `(generation, state)` of `slot`.
    pub fn load(&self, slot: usize) -> Option<(u32, u8)> {
        let w = self.words.get(slot)?.load(Ordering::Acquire);
        Some((w >> 8, w as u8))
    }

    pub fn store(&self, slot: usize, generation: u32, state: u8) {
        if let Some(w) = self.words.get(slot) {
            w.store(state_word(generation, state), Ordering::Release);
        }
    }

    /// Moves `slot` from `from` to `to` only while it still holds `generation`.
    pub fn transition(&self, slot: usize, generation: u32, from: u8, to: u8) -> bool {
        self.words.get(slot).is_some_and(|w| {
            w.compare_exchange(
                state_word(generation, from),
                state_word(generation, to),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        })
    }

    /// Every playing or paused slot becomes stopped, keeping its generation.
    pub fn stop_all(&self) {
        for w in self.words.iter() {
            let mut v = w.load(Ordering::Acquire);
            while matches!(v as u8, VOICE_PLAYING | VOICE_PAUSED) {
                let stopped = (v & !0xFF) | u32::from(VOICE_STOPPED);
                match w.compare_exchange_weak(v, stopped, Ordering::AcqRel, Ordering::Acquire) {
                    Ok(_) => break,
                    Err(now) => v = now,
                }
            }
        }
    }

    pub fn link(self: &Arc<Self>, slot: usize, generation: u32) -> Option<StateLink> {
        (slot < self.words.len()).then(|| StateLink {
            table: self.clone(),
            slot,
            generation: generation & GENERATION_MASK,
        })
    }
}

/// Ties a mixer voice to one generation of a slot; the slot reads stopped once the voice is gone.
#[derive(Debug)]
pub struct StateLink {
    table: Arc<VoiceStates>,
    slot: usize,
    generation: u32,
}

impl StateLink {
    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub(crate) fn transition(&self, from: u8, to: u8) -> bool {
        self.table.transition(self.slot, self.generation, from, to)
    }
}

impl Drop for StateLink {
    fn drop(&mut self) {
        if !self.transition(VOICE_PLAYING, VOICE_STOPPED) {
            self.transition(VOICE_PAUSED, VOICE_STOPPED);
        }
        self.transition(VOICE_INITIAL, VOICE_STOPPED);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_a_link_stops_only_its_own_generation() {
        let table = VoiceStates::new(4);
        table.store(1, 7, VOICE_PLAYING);
        let old = table.link(1, 6).unwrap();
        drop(old);
        assert_eq!(table.load(1), Some((7, VOICE_PLAYING)));
        let current = table.link(1, 7).unwrap();
        drop(current);
        assert_eq!(table.load(1), Some((7, VOICE_STOPPED)));
        assert!(table.link(4, 0).is_none());
        assert_eq!(table.load(4), None);
    }

    #[test]
    fn stop_all_keeps_generations_and_initial_slots() {
        let table = VoiceStates::new(3);
        table.store(0, 1, VOICE_PLAYING);
        table.store(1, 2, VOICE_PAUSED);
        table.store(2, 3, VOICE_INITIAL);
        table.stop_all();
        assert_eq!(table.load(0), Some((1, VOICE_STOPPED)));
        assert_eq!(table.load(1), Some((2, VOICE_STOPPED)));
        assert_eq!(table.load(2), Some((3, VOICE_INITIAL)));
    }

    #[test]
    fn generations_wrap_within_24_bits() {
        assert_eq!(state_word(GENERATION_MASK + 2, VOICE_PAUSED), (1 << 8) | 2);
    }
}
