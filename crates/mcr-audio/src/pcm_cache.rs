use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::decode::Pcm;

/// Playing voices hold their own `Arc`, so evicting never cuts a sound short.
pub const BUDGET_BYTES: usize = 64 << 20;
const MAX_ENTRY_BYTES: usize = BUDGET_BYTES / 4;

#[derive(Debug, Default)]
pub struct PcmCache {
    entries: FxHashMap<String, Entry>,
    bytes: usize,
    clock: u64,
}

#[derive(Debug)]
struct Entry {
    pcm: Arc<Pcm>,
    used: u64,
}

fn size(pcm: &Pcm) -> usize {
    std::mem::size_of_val(&*pcm.samples)
}

impl PcmCache {
    pub fn get(&mut self, name: &str) -> Option<Arc<Pcm>> {
        self.clock += 1;
        let entry = self.entries.get_mut(name)?;
        entry.used = self.clock;
        Some(entry.pcm.clone())
    }

    pub fn insert(&mut self, name: &str, pcm: Arc<Pcm>) {
        let bytes = size(&pcm);
        if bytes > MAX_ENTRY_BYTES {
            return;
        }
        while self.bytes + bytes > BUDGET_BYTES && self.evict_oldest() {}
        self.clock += 1;
        let entry = Entry {
            pcm,
            used: self.clock,
        };
        if let Some(old) = self.entries.insert(name.to_string(), entry) {
            self.bytes -= size(&old.pcm);
        }
        self.bytes += bytes;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    #[cfg(test)]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn evict_oldest(&mut self) -> bool {
        let Some(name) = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.used)
            .map(|(n, _)| n.clone())
        else {
            return false;
        };
        if let Some(old) = self.entries.remove(&name) {
            self.bytes -= size(&old.pcm);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm(bytes: usize) -> Arc<Pcm> {
        Arc::new(Pcm::mono(48_000, vec![0.0; bytes / 4]))
    }

    #[test]
    fn churn_stays_within_budget() {
        let mut cache = PcmCache::default();
        for i in 0..4096 {
            cache.insert(&format!("s{i}"), pcm(1 << 20));
            assert!(cache.bytes() <= BUDGET_BYTES);
        }
        assert_eq!(cache.len(), BUDGET_BYTES >> 20);
    }

    #[test]
    fn evicts_least_recently_used() {
        let mut cache = PcmCache::default();
        let n = BUDGET_BYTES / (1 << 20);
        for i in 0..n {
            cache.insert(&format!("s{i}"), pcm(1 << 20));
        }
        assert!(cache.get("s0").is_some());
        cache.insert("new", pcm(1 << 20));
        assert!(cache.get("s0").is_some());
        assert!(cache.get("s1").is_none());
        assert!(cache.get("new").is_some());
    }

    #[test]
    fn oversized_sound_is_not_kept() {
        let mut cache = PcmCache::default();
        cache.insert("huge", pcm(MAX_ENTRY_BYTES + 4));
        assert!(cache.get("huge").is_none());
        assert_eq!(cache.bytes(), 0);
    }

    #[test]
    fn reinsert_does_not_double_count() {
        let mut cache = PcmCache::default();
        cache.insert("a", pcm(1024));
        cache.insert("a", pcm(1024));
        assert_eq!(cache.bytes(), 1024);
        cache.clear();
        assert_eq!(cache.bytes(), 0);
    }
}
