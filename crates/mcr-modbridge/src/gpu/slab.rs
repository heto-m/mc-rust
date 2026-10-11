//! Generation-checked handle tables: Java holds `u64` handles, never pointers.
//!
//! A handle is `generation << 32 | kind << 28 | (index + 1)`, so stale handles and handles of the
//! wrong kind are both rejected. Generations use 31 bits, keeping every handle positive in Java.

/// `0` is never a valid handle, so Java can use it for "none".
pub type Handle = u64;

const INDEX_BITS: u32 = 28;
const INDEX_MASK: u64 = (1 << INDEX_BITS) - 1;
const MAX_GENERATION: u32 = (1 << 31) - 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Kind {
    Buffer = 1,
    Texture = 2,
    View = 3,
    Sampler = 4,
    Pipeline = 5,
    Encoder = 6,
    Surface = 7,
}

impl Kind {
    pub fn of(handle: Handle) -> Option<Kind> {
        Some(match (handle >> INDEX_BITS) & 0xF {
            1 => Kind::Buffer,
            2 => Kind::Texture,
            3 => Kind::View,
            4 => Kind::Sampler,
            5 => Kind::Pipeline,
            6 => Kind::Encoder,
            7 => Kind::Surface,
            _ => return None,
        })
    }
}

#[derive(Debug)]
struct Entry<T> {
    generation: u32,
    value: Option<T>,
}

#[derive(Debug)]
pub struct Slab<T> {
    kind: Kind,
    entries: Vec<Entry<T>>,
    free: Vec<u32>,
    live: usize,
}

impl<T> Slab<T> {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            entries: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    fn split(&self, handle: Handle) -> Option<(usize, u32)> {
        let index = handle & INDEX_MASK;
        let generation = (handle >> 32) as u32;
        let kind = (handle >> INDEX_BITS) & 0xF;
        (index != 0 && generation != 0 && kind == self.kind as u64)
            .then(|| (index as usize - 1, generation))
    }

    /// `None` once the index space is exhausted.
    pub fn insert(&mut self, value: T) -> Option<Handle> {
        let index = match self.free.pop() {
            Some(i) => {
                self.entries[i as usize].value = Some(value);
                i
            }
            None => {
                if self.entries.len() as u64 >= INDEX_MASK {
                    return None;
                }
                self.entries.push(Entry {
                    generation: 1,
                    value: Some(value),
                });
                (self.entries.len() - 1) as u32
            }
        };
        self.live += 1;
        let generation = self.entries[index as usize].generation;
        Some(
            (u64::from(generation) << 32)
                | ((self.kind as u64) << INDEX_BITS)
                | u64::from(index + 1),
        )
    }

    pub fn get(&self, handle: Handle) -> Option<&T> {
        let (index, generation) = self.split(handle)?;
        let e = self.entries.get(index)?;
        if e.generation != generation {
            return None;
        }
        e.value.as_ref()
    }

    pub fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
        let (index, generation) = self.split(handle)?;
        let e = self.entries.get_mut(index)?;
        if e.generation != generation {
            return None;
        }
        e.value.as_mut()
    }

    pub fn remove(&mut self, handle: Handle) -> Option<T> {
        let (index, generation) = self.split(handle)?;
        let e = self.entries.get_mut(index)?;
        if e.generation != generation {
            return None;
        }
        let value = e.value.take()?;
        e.generation = if e.generation >= MAX_GENERATION {
            1
        } else {
            e.generation + 1
        };
        self.free.push(index as u32);
        self.live -= 1;
        Some(value)
    }

    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.entries.iter_mut().filter_map(|e| e.value.as_mut())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_handles_are_rejected_after_reuse() {
        let mut slab = Slab::new(Kind::Buffer);
        let a = slab.insert("a").unwrap();
        assert_eq!(slab.get(a), Some(&"a"));
        assert_eq!(slab.remove(a), Some("a"));
        assert_eq!(slab.get(a), None);
        assert_eq!(slab.remove(a), None);
        let b = slab.insert("b").unwrap();
        assert_ne!(a, b, "the reused slot carries a new generation");
        assert_eq!(slab.get(a), None);
        assert_eq!(slab.get(b), Some(&"b"));
        assert_eq!(slab.len(), 1);
        assert_eq!(Kind::of(b), Some(Kind::Buffer));
    }

    #[test]
    fn garbage_and_foreign_handles_are_rejected() {
        let mut slab = Slab::new(Kind::View);
        let mut other = Slab::new(Kind::Texture);
        let a = slab.insert(1u8).unwrap();
        let t = other.insert(2u8).unwrap();
        for h in [0, a & 0xFFFF_FFFF, a + 1, u64::MAX, a ^ (1 << 40), t] {
            assert_eq!(slab.get(h), None, "{h:#x}");
        }
        assert!(slab.get_mut(a).is_some());
        assert!((a as i64) > 0);
    }
}
