use crate::{List, Value};

const INDEX_THRESHOLD: usize = 8;
const EMPTY: u32 = u32::MAX;

/// Insertion-ordered map; linear scan while small, open-addressing index once it grows.
#[derive(Debug, Clone, Default)]
pub struct Compound {
    entries: Vec<(String, Value)>,
    index: Option<Vec<u32>>,
}

fn hash(key: &str) -> u64 {
    let bytes = key.as_bytes();
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ bytes.len() as u64;
    for chunk in bytes.chunks(8) {
        let mut b = [0u8; 8];
        b[..chunk.len()].copy_from_slice(chunk);
        h = (h.rotate_left(5) ^ u64::from_le_bytes(b)).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    h ^ (h >> 32)
}

fn build_index(entries: &[(String, Value)]) -> Vec<u32> {
    let mut slots = vec![EMPTY; (entries.len() * 2).next_power_of_two().max(32)];
    for (i, (k, _)) in entries.iter().enumerate() {
        place(&mut slots, hash(k), i as u32);
    }
    slots
}

fn place(slots: &mut [u32], h: u64, i: u32) {
    let mask = slots.len() - 1;
    let mut s = h as usize & mask;
    while slots[s] != EMPTY {
        s = (s + 1) & mask;
    }
    slots[s] = i;
}

impl Compound {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            entries: Vec::with_capacity(n),
            index: None,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &Value)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn keys(&self) -> impl ExactSizeIterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &Value> {
        self.entries.iter().map(|(_, v)| v)
    }

    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Value> {
        self.entries.iter_mut().map(|(_, v)| v)
    }

    fn position(&self, key: &str) -> Option<usize> {
        let Some(slots) = &self.index else {
            return self.entries.iter().position(|(k, _)| k == key);
        };
        let mask = slots.len() - 1;
        let mut s = hash(key) as usize & mask;
        loop {
            match slots[s] {
                EMPTY => return None,
                i if self.entries[i as usize].0 == key => return Some(i as usize),
                _ => s = (s + 1) & mask,
            }
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.position(key).map(|i| &self.entries[i].1)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.position(key).map(|i| &mut self.entries[i].1)
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.position(key).is_some()
    }

    /// Replacing an existing key keeps its position.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        let key = key.into();
        let value = value.into();
        if let Some(i) = self.position(&key) {
            return Some(std::mem::replace(&mut self.entries[i].1, value));
        }
        let i = self.entries.len();
        match &mut self.index {
            Some(slots) if (i + 1) * 2 > slots.len() => {
                self.entries.push((key, value));
                self.index = Some(build_index(&self.entries));
            }
            Some(slots) => {
                place(slots, hash(&key), i as u32);
                self.entries.push((key, value));
            }
            None => {
                self.entries.push((key, value));
                if self.entries.len() > INDEX_THRESHOLD {
                    self.index = Some(build_index(&self.entries));
                }
            }
        }
        None
    }

    /// Reorders entries as vanilla's `HashMap`-backed compound iterates them, so encoded bytes
    /// match a vanilla peer. Ties keep insertion order.
    pub fn sort_vanilla_order(&mut self) {
        let mut capacity = 16usize;
        while self.entries.len() * 4 > capacity * 3 {
            capacity *= 2;
        }
        self.entries.sort_by_cached_key(|(k, _)| {
            let h = k
                .encode_utf16()
                .fold(0u32, |h, unit| h.wrapping_mul(31).wrapping_add(unit.into()));
            ((h ^ (h >> 16)) as usize) & (capacity - 1)
        });
        if self.index.is_some() {
            self.index = Some(build_index(&self.entries));
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let i = self.position(key)?;
        let (_, v) = self.entries.remove(i);
        if self.index.is_some() {
            self.index = (self.entries.len() > INDEX_THRESHOLD).then(|| build_index(&self.entries));
        }
        Some(v)
    }

    pub fn get_i8(&self, key: &str) -> Option<i8> {
        self.get(key)?.as_i8()
    }

    pub fn get_i16(&self, key: &str) -> Option<i16> {
        self.get(key)?.as_i16()
    }

    pub fn get_i32(&self, key: &str) -> Option<i32> {
        self.get(key)?.as_i32()
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key)?.as_i64()
    }

    pub fn get_f32(&self, key: &str) -> Option<f32> {
        self.get(key)?.as_f32()
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get(key)?.as_f64()
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get_i8(key).map(|b| b != 0)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    pub fn get_compound(&self, key: &str) -> Option<&Compound> {
        self.get(key)?.as_compound()
    }

    pub fn get_list(&self, key: &str) -> Option<&List> {
        self.get(key)?.as_list()
    }

    pub fn get_byte_array(&self, key: &str) -> Option<&[i8]> {
        self.get(key)?.as_byte_array()
    }

    pub fn get_int_array(&self, key: &str) -> Option<&[i32]> {
        self.get(key)?.as_int_array()
    }

    pub fn get_long_array(&self, key: &str) -> Option<&[i64]> {
        self.get(key)?.as_long_array()
    }
}

impl PartialEq for Compound {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

impl<K: Into<String>, V: Into<Value>> FromIterator<(K, V)> for Compound {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut c = Compound::new();
        for (k, v) in iter {
            c.insert(k, v);
        }
        c
    }
}

impl<'a> IntoIterator for &'a Compound {
    type Item = (&'a str, &'a Value);
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, (String, Value)>,
        fn(&'a (String, Value)) -> (&'a str, &'a Value),
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_and_replace() {
        let mut c = Compound::new();
        c.insert("b", 1i32);
        c.insert("a", 2i32);
        c.insert("b", 3i32);
        assert_eq!(c.keys().collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(c.get_i32("b"), Some(3));
    }

    #[test]
    fn large_uses_index() {
        let mut c = Compound::new();
        for i in 0..1000 {
            c.insert(format!("k{i}"), i);
        }
        assert!(c.index.is_some());
        for i in 0..1000 {
            assert_eq!(c.get_i32(&format!("k{i}")), Some(i));
        }
        assert_eq!(c.remove("k5").and_then(|v| v.as_i32()), Some(5));
        assert!(!c.contains_key("k5"));
        assert_eq!(c.get_i32("k999"), Some(999));
        assert_eq!(c.len(), 999);
    }

    #[test]
    fn equality_ignores_order() {
        let a: Compound = [("x", 1i32), ("y", 2)].into_iter().collect();
        let b: Compound = [("y", 2i32), ("x", 1)].into_iter().collect();
        assert_eq!(a, b);
    }
}
