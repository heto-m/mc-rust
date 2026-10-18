//! Dynamic light sources a Java mod computed (LambDynamicLights), copied for mcr's terrain shader.

use std::sync::atomic::{AtomicU64, Ordering, fence};

use mcr_render::DynamicLight;
use mcr_render::glam::DVec3;

use crate::abi::{
    LIGHT_COUNT, LIGHT_END, LIGHT_ENTRIES, LIGHT_ENTRY_SIZE, LIGHT_FLAGS, LIGHT_LEVEL, LIGHT_MAX,
    LIGHT_SEQ, LIGHT_START,
};

const WORDS: usize = (LIGHT_ENTRIES + LIGHT_MAX * LIGHT_ENTRY_SIZE) / 8;
/// Blocks a segment may span per axis (a beacon beam is the world's height, at most 4064).
const MAX_SPAN: f32 = 4096.0;
/// Beyond the world border by far; anything further is garbage.
const MAX_COORD: f64 = 1.0e8;

const _: () = {
    assert!(LIGHT_SEQ == 0 && LIGHT_FLAGS == 8 && LIGHT_COUNT == 12);
    assert!(LIGHT_ENTRIES.is_multiple_of(8) && LIGHT_ENTRY_SIZE.is_multiple_of(8));
    assert!(LIGHT_START == 0 && LIGHT_END == 24 && LIGHT_LEVEL == 36);
};

/// Written by Java at the end of its frame, read by the render thread once that frame completed.
#[derive(Debug)]
pub(crate) struct LightShared {
    words: Box<[AtomicU64]>,
}

/// The block's flags and whether its sources were read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LightRead {
    pub flags: u32,
    /// `false` when Java was writing; the caller keeps its previous sources.
    pub complete: bool,
}

impl LightShared {
    pub fn new() -> Self {
        Self {
            words: (0..WORDS).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    pub fn addr(&self) -> i64 {
        self.words.as_ptr() as i64
    }

    fn word(&self, at: usize) -> u64 {
        self.words[at / 8].load(Ordering::Relaxed)
    }

    fn u32_at(&self, at: usize) -> u32 {
        let bytes = self.word(at).to_ne_bytes();
        let o = at % 8;
        u32::from_ne_bytes(bytes[o..o + 4].try_into().unwrap_or_default())
    }

    fn f32_at(&self, at: usize) -> f32 {
        f32::from_bits(self.u32_at(at))
    }

    fn f64_at(&self, at: usize) -> f64 {
        f64::from_bits(self.word(at))
    }

    /// Replaces `out` with the published sources, bounded and clamped; leaves it alone on a torn read.
    /// `scratch` holds the read until it is known to be whole.
    pub fn read(&self, out: &mut Vec<DynamicLight>, scratch: &mut Vec<DynamicLight>) -> LightRead {
        let seq = self.words[LIGHT_SEQ / 8].load(Ordering::Acquire) as u32;
        let flags = self.u32_at(LIGHT_FLAGS);
        if seq & 1 != 0 {
            return LightRead {
                flags,
                complete: false,
            };
        }
        let count = (self.u32_at(LIGHT_COUNT) as usize).min(LIGHT_MAX);
        scratch.clear();
        for i in 0..count {
            let at = LIGHT_ENTRIES + i * LIGHT_ENTRY_SIZE;
            let start = DVec3::new(
                self.f64_at(at + LIGHT_START),
                self.f64_at(at + LIGHT_START + 8),
                self.f64_at(at + LIGHT_START + 16),
            );
            let span = [0, 4, 8].map(|o| self.f32_at(at + LIGHT_END + o));
            let level = self.f32_at(at + LIGHT_LEVEL);
            if let Some(light) = sanitize(start, span, level) {
                scratch.push(light);
            }
        }
        fence(Ordering::Acquire);
        let again = self.words[LIGHT_SEQ / 8].load(Ordering::Relaxed) as u32;
        if again != seq {
            return LightRead {
                flags,
                complete: false,
            };
        }
        std::mem::swap(out, scratch);
        LightRead {
            flags,
            complete: true,
        }
    }

    #[cfg(test)]
    fn write(&self, seq: u32, flags: u32, count: u32, entries: &[([f64; 3], [f32; 3], f32)]) {
        let pair = |a: u32, b: u32| {
            let mut bytes = [0u8; 8];
            bytes[..4].copy_from_slice(&a.to_ne_bytes());
            bytes[4..].copy_from_slice(&b.to_ne_bytes());
            u64::from_ne_bytes(bytes)
        };
        self.words[0].store(u64::from(seq), Ordering::Relaxed);
        self.words[1].store(pair(flags, count), Ordering::Relaxed);
        for (i, (start, span, level)) in entries.iter().enumerate() {
            let w = (LIGHT_ENTRIES + i * LIGHT_ENTRY_SIZE) / 8;
            for (k, v) in start.iter().enumerate() {
                self.words[w + k].store(v.to_bits(), Ordering::Relaxed);
            }
            self.words[w + 3].store(
                pair(span[0].to_bits(), span[1].to_bits()),
                Ordering::Relaxed,
            );
            self.words[w + 4].store(pair(span[2].to_bits(), level.to_bits()), Ordering::Relaxed);
        }
    }
}

fn sanitize(start: DVec3, span: [f32; 3], level: f32) -> Option<DynamicLight> {
    if !start.is_finite() || start.abs().max_element() > MAX_COORD {
        return None;
    }
    if !span.iter().all(|v| v.is_finite()) || !level.is_finite() {
        return None;
    }
    let level = level.min(15.0);
    if level <= 0.0 {
        return None;
    }
    let span = span.map(|v| f64::from(v.clamp(-MAX_SPAN, MAX_SPAN)));
    Some(DynamicLight {
        start,
        end: start + DVec3::from_array(span),
        level,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_points_and_segments() {
        let block = LightShared::new();
        block.write(
            2,
            3,
            2,
            &[
                ([10.5, 64.62, -3.25], [0.0; 3], 14.0),
                ([0.5, -64.0, 0.5], [0.0, 384.0, 0.0], 15.0),
            ],
        );
        let mut out = Vec::new();
        let read = block.read(&mut out, &mut Vec::new());
        assert_eq!(
            read,
            LightRead {
                flags: 3,
                complete: true
            }
        );
        assert_eq!(
            out,
            vec![
                DynamicLight::point(DVec3::new(10.5, 64.62, -3.25), 14.0),
                DynamicLight {
                    start: DVec3::new(0.5, -64.0, 0.5),
                    end: DVec3::new(0.5, 320.0, 0.5),
                    level: 15.0,
                },
            ]
        );
    }

    #[test]
    fn hostile_block_is_bounded_and_clamped() {
        let block = LightShared::new();
        let mut entries = vec![([1.0, 2.0, 3.0], [0.0; 3], 7.0); LIGHT_MAX];
        entries[0] = ([f64::NAN, 0.0, 0.0], [0.0; 3], 15.0);
        entries[1] = ([0.0, 0.0, 0.0], [0.0; 3], 99.0);
        entries[2] = ([0.0, 0.0, 0.0], [0.0; 3], -1.0);
        entries[3] = ([0.0, 0.0, 0.0], [f32::INFINITY, 0.0, 0.0], 9.0);
        entries[4] = ([0.0, 0.0, 0.0], [1e9, -1e9, 0.0], 9.0);
        entries[5] = ([1e300, 0.0, 0.0], [0.0; 3], 9.0);
        entries[6] = ([0.0, 0.0, 0.0], [0.0; 3], f32::NAN);
        block.write(0, 1, 100_000, &entries);
        let mut out = Vec::new();
        assert!(block.read(&mut out, &mut Vec::new()).complete);
        assert_eq!(out.len(), LIGHT_MAX - 5);
        assert_eq!(out[0].level, 15.0);
        assert_eq!(
            out[1].end,
            DVec3::new(f64::from(MAX_SPAN), -f64::from(MAX_SPAN), 0.0)
        );
        assert!(out.iter().all(|l| l.level > 0.0 && l.level <= 15.0));
    }

    #[test]
    fn torn_read_keeps_the_previous_sources() {
        let block = LightShared::new();
        block.write(1, 3, 1, &[([1.0, 2.0, 3.0], [0.0; 3], 7.0)]);
        let mut out = vec![DynamicLight::point(DVec3::ZERO, 5.0)];
        let read = block.read(&mut out, &mut Vec::new());
        assert!(!read.complete);
        assert_eq!(read.flags, 3);
        assert_eq!(out, vec![DynamicLight::point(DVec3::ZERO, 5.0)]);
    }

    #[test]
    fn empty_block_reads_nothing() {
        let mut out = vec![DynamicLight::point(DVec3::ZERO, 5.0)];
        let read = LightShared::new().read(&mut out, &mut Vec::new());
        assert_eq!(
            read,
            LightRead {
                flags: 0,
                complete: true
            }
        );
        assert!(out.is_empty());
    }
}
