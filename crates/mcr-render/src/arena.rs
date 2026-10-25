use std::collections::{BTreeMap, BTreeSet};

/// Best-fit offset allocator over `[0, capacity)` with coalescing free ranges.
#[derive(Debug)]
pub struct Arena {
    capacity: u32,
    by_offset: BTreeMap<u32, u32>,
    by_size: BTreeSet<(u32, u32)>,
    used: u32,
}

impl Arena {
    pub fn new(capacity: u32) -> Self {
        let mut a = Self {
            capacity,
            by_offset: BTreeMap::new(),
            by_size: BTreeSet::new(),
            used: 0,
        };
        if capacity > 0 {
            a.insert_free(0, capacity);
        }
        a
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn used(&self) -> u32 {
        self.used
    }

    pub fn largest_free(&self) -> u32 {
        self.by_size.last().map_or(0, |&(s, _)| s)
    }

    fn insert_free(&mut self, offset: u32, size: u32) {
        self.by_offset.insert(offset, size);
        self.by_size.insert((size, offset));
    }

    fn remove_free(&mut self, offset: u32, size: u32) {
        self.by_offset.remove(&offset);
        self.by_size.remove(&(size, offset));
    }

    pub fn alloc(&mut self, size: u32) -> Option<u32> {
        if size == 0 {
            return None;
        }
        let &(free_size, offset) = self.by_size.range((size, 0)..).next()?;
        self.remove_free(offset, free_size);
        if free_size > size {
            self.insert_free(offset + size, free_size - size);
        }
        self.used += size;
        Some(offset)
    }

    /// Extends the range to `[0, capacity)`; live offsets stay valid.
    pub fn grow(&mut self, capacity: u32) {
        if capacity <= self.capacity {
            return;
        }
        let (mut start, mut len) = (self.capacity, capacity - self.capacity);
        if let Some((&prev, &prev_len)) = self.by_offset.range(..start).next_back()
            && prev + prev_len == start
        {
            self.remove_free(prev, prev_len);
            start = prev;
            len += prev_len;
        }
        self.capacity = capacity;
        self.insert_free(start, len);
    }

    pub fn free(&mut self, offset: u32, size: u32) {
        debug_assert!(offset + size <= self.capacity);
        self.used -= size;
        let (mut start, mut len) = (offset, size);
        if let Some((&prev, &prev_len)) = self.by_offset.range(..offset).next_back()
            && prev + prev_len == start
        {
            self.remove_free(prev, prev_len);
            start = prev;
            len += prev_len;
        }
        if let Some(&next_len) = self.by_offset.get(&(offset + size)) {
            self.remove_free(offset + size, next_len);
            len += next_len;
        }
        self.insert_free(start, len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) as u32
        }
    }

    #[test]
    fn fuzz_no_overlap_and_full_coalesce() {
        let cap = 100_000;
        let mut arena = Arena::new(cap);
        let mut rng = Lcg(7);
        let mut live: Vec<(u32, u32)> = Vec::new();
        for _ in 0..20_000 {
            if live.is_empty() || !rng.next().is_multiple_of(3) {
                let size = rng.next() % 500 + 1;
                if let Some(off) = arena.alloc(size) {
                    assert!(
                        live.iter().all(|&(o, s)| off + size <= o || o + s <= off),
                        "overlap"
                    );
                    assert!(off + size <= cap);
                    live.push((off, size));
                }
            } else {
                let i = rng.next() as usize % live.len();
                let (o, s) = live.swap_remove(i);
                arena.free(o, s);
            }
            assert_eq!(arena.used(), live.iter().map(|l| l.1).sum::<u32>());
        }
        for (o, s) in live {
            arena.free(o, s);
        }
        assert_eq!(arena.used(), 0);
        assert_eq!(arena.largest_free(), cap);
        assert_eq!(arena.alloc(cap), Some(0));
    }

    #[test]
    fn grow_extends_the_trailing_free_range() {
        let mut arena = Arena::new(10);
        assert_eq!(arena.alloc(4), Some(0));
        assert_eq!(arena.alloc(7), None);
        arena.grow(20);
        assert_eq!(arena.capacity(), 20);
        assert_eq!(arena.largest_free(), 16);
        assert_eq!(arena.alloc(16), Some(4));
        arena.grow(20);
        assert_eq!(arena.alloc(1), None);
        arena.grow(24);
        assert_eq!(arena.alloc(4), Some(20));
        arena.free(0, 4);
        arena.free(4, 16);
        arena.free(20, 4);
        assert_eq!(arena.largest_free(), 24);
    }

    #[test]
    fn exhaustion_returns_none() {
        let mut arena = Arena::new(10);
        assert_eq!(arena.alloc(10), Some(0));
        assert_eq!(arena.alloc(1), None);
        assert_eq!(arena.alloc(0), None);
    }
}
