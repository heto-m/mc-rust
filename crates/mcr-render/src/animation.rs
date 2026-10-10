//! Per-sprite animation state, evaluated per client tick into the GPU frame table.

/// One step of an animation: the layer offset from the sprite's first layer and its duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteFrame {
    pub layer_offset: u16,
    pub ticks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteAnimation {
    /// Packed `group << 14 | layer` of the sprite's first frame, as stored in quads.
    pub layer: u16,
    pub frames: Vec<SpriteFrame>,
    pub interpolate: bool,
}

/// Table entries: `[current layer, next layer | mix << 16]` with `mix` in 0..=65535.
pub type AnimEntry = [u32; 2];

pub const ANIM_TABLE_LEN: usize = 1 << 16;

fn identity(layer: u32) -> AnimEntry {
    [layer, 0]
}

impl SpriteAnimation {
    pub fn total_ticks(&self) -> u64 {
        self.frames.iter().map(|f| u64::from(f.ticks.max(1))).sum()
    }

    /// The entry for the sprite after `tick` client ticks, as vanilla's `AnimationState` advances.
    pub fn entry(&self, tick: u64) -> AnimEntry {
        let total = self.total_ticks();
        if self.frames.is_empty() || total == 0 {
            return identity(u32::from(self.layer));
        }
        let mut t = tick % total;
        let mut index = 0;
        for (i, f) in self.frames.iter().enumerate() {
            let len = u64::from(f.ticks.max(1));
            if t < len {
                index = i;
                break;
            }
            t -= len;
        }
        let frame = self.frames[index];
        let layer_of = |f: SpriteFrame| u32::from(self.layer) + u32::from(f.layer_offset);
        let current = layer_of(frame);
        if !self.interpolate {
            return identity(current);
        }
        let next = layer_of(self.frames[(index + 1) % self.frames.len()]);
        let mix = (t as f32 / frame.ticks.max(1) as f32 * 65535.0).round() as u32;
        [current, next | mix.min(65535) << 16]
    }
}

/// CPU mirror of the GPU table: only entries that changed since the last update are reported.
#[derive(Debug, Default)]
pub struct AnimationTable {
    anims: Vec<SpriteAnimation>,
    entries: Vec<AnimEntry>,
}

impl AnimationTable {
    pub fn identity_table() -> Vec<AnimEntry> {
        (0..ANIM_TABLE_LEN as u32).map(identity).collect()
    }

    pub fn new() -> Self {
        Self {
            anims: Vec::new(),
            entries: Self::identity_table(),
        }
    }

    pub fn animations(&self) -> &[SpriteAnimation] {
        &self.anims
    }

    /// Replaces the animated sprites and returns the full table to upload.
    pub fn set(&mut self, anims: Vec<SpriteAnimation>, tick: u64) -> &[AnimEntry] {
        self.anims = anims;
        self.entries = Self::identity_table();
        for a in &self.anims {
            self.entries[usize::from(a.layer)] = a.entry(tick);
        }
        &self.entries
    }

    /// Re-evaluates every animation at `tick`; returns `(table index, entry)` for the changed ones.
    pub fn update(&mut self, tick: u64) -> Vec<(usize, AnimEntry)> {
        let mut changed = Vec::new();
        for a in &self.anims {
            let i = usize::from(a.layer);
            let e = a.entry(tick);
            if self.entries[i] != e {
                self.entries[i] = e;
                changed.push((i, e));
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim(interpolate: bool) -> SpriteAnimation {
        SpriteAnimation {
            layer: 100,
            frames: vec![
                SpriteFrame {
                    layer_offset: 0,
                    ticks: 2,
                },
                SpriteFrame {
                    layer_offset: 2,
                    ticks: 4,
                },
                SpriteFrame {
                    layer_offset: 1,
                    ticks: 1,
                },
            ],
            interpolate,
        }
    }

    #[test]
    fn frames_advance_by_duration_and_wrap() {
        let a = anim(false);
        let layers: Vec<u32> = (0..9).map(|t| a.entry(t)[0]).collect();
        assert_eq!(layers, [100, 100, 102, 102, 102, 102, 101, 100, 100]);
    }

    #[test]
    fn interpolation_mixes_towards_the_next_frame() {
        let a = anim(true);
        assert_eq!(a.entry(0), [100, 102]);
        let mid = a.entry(1);
        assert_eq!(mid[0], 100);
        assert_eq!(mid[1] & 0xffff, 102);
        assert_eq!(mid[1] >> 16, 32768);
        let last = a.entry(6);
        assert_eq!(last[0], 101);
        assert_eq!(last[1] & 0xffff, 100);
    }

    #[test]
    fn update_reports_only_changes() {
        let mut t = AnimationTable::new();
        t.set(vec![anim(false)], 0);
        assert!(t.update(1).is_empty());
        let changed = t.update(2);
        assert_eq!(changed, vec![(100, [102, 0])]);
        assert!(t.update(3).is_empty());
    }
}
