use std::sync::RwLock;

use mcr_data::Version;
use mcr_nbt::{Compound, Value};
use rustc_hash::FxHashMap;

struct Prop {
    name: &'static str,
    values: Vec<&'static str>,
    stride: u32,
}

struct BlockSpec {
    first: u32,
    default: u32,
    props: Vec<Prop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Exact(u32),
    BadProperty(u32),
    UnknownBlock,
}

/// Maps a palette entry (block name plus property strings) to a global block state id.
pub struct BlockResolver {
    blocks: FxHashMap<&'static str, BlockSpec>,
    state_count: u32,
}

impl std::fmt::Debug for BlockResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockResolver")
            .field("blocks", &self.blocks.len())
            .finish()
    }
}

impl BlockResolver {
    pub fn new(version: &'static Version) -> Self {
        let mut blocks = FxHashMap::default();
        for (id, name) in version.blocks().iter() {
            let Some(info) = version.block_info(id) else {
                continue;
            };
            let (first, end) = info.states;
            let mut props: Vec<Prop> = version
                .state_properties(first)
                .map(|(name, _)| Prop {
                    name,
                    values: Vec::new(),
                    stride: 1,
                })
                .collect();
            for state in first..end {
                for (prop, (_, value)) in props.iter_mut().zip(version.state_properties(state)) {
                    if !prop.values.contains(&value) {
                        prop.values.push(value);
                    }
                }
            }
            let mut stride = 1;
            for prop in props.iter_mut().rev() {
                prop.stride = stride;
                stride *= prop.values.len() as u32;
            }
            blocks.insert(
                name,
                BlockSpec {
                    first,
                    default: info.default_state,
                    props,
                },
            );
        }
        Self {
            blocks,
            state_count: version.state_count() as u32,
        }
    }

    pub fn state_count(&self) -> u32 {
        self.state_count
    }

    pub fn default_state(&self, name: &str) -> Option<u32> {
        self.blocks.get(name).map(|b| b.default)
    }

    pub fn resolve(&self, name: &str, properties: Option<&Compound>) -> Resolved {
        let Some(spec) = self.blocks.get(name) else {
            return Resolved::UnknownBlock;
        };
        let mut ordinal = i64::from(spec.default - spec.first);
        let mut exact = true;
        for (key, value) in properties.into_iter().flat_map(Compound::iter) {
            if key.is_empty() {
                continue;
            }
            let found = spec.props.iter().find(|p| p.name == key).and_then(|p| {
                let Value::String(v) = value else { return None };
                p.values.iter().position(|x| x == v).map(|i| (p, i as i64))
            });
            let Some((prop, new)) = found else {
                exact = false;
                continue;
            };
            let count = prop.values.len() as i64;
            let stride = i64::from(prop.stride);
            let old = ordinal / stride % count;
            ordinal += (new - old) * stride;
        }
        let state = (i64::from(spec.first) + ordinal) as u32;
        if exact {
            Resolved::Exact(state)
        } else {
            Resolved::BadProperty(state)
        }
    }
}

/// Interns biome names to dense ids; biomes live in datapacks so there is no static registry.
#[derive(Debug)]
pub struct BiomeTable {
    inner: RwLock<(FxHashMap<String, u16>, Vec<String>)>,
    limit: usize,
}

impl BiomeTable {
    pub fn new(limit: usize) -> Self {
        Self {
            inner: RwLock::default(),
            limit,
        }
    }

    pub fn limit(&self) -> usize {
        self.limit
    }

    /// `None` when the table is full.
    pub fn intern(&self, name: &str) -> Option<u16> {
        if let Some(&id) = self.inner.read().unwrap().0.get(name) {
            return Some(id);
        }
        let mut guard = self.inner.write().unwrap();
        if let Some(&id) = guard.0.get(name) {
            return Some(id);
        }
        if guard.1.len() >= self.limit {
            return None;
        }
        let id = guard.1.len() as u16;
        guard.1.push(name.to_owned());
        guard.0.insert(name.to_owned(), id);
        Some(id)
    }

    pub fn name(&self, id: u16) -> Option<String> {
        self.inner.read().unwrap().1.get(id as usize).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.inner.read().unwrap().1.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(pairs: &[(&str, &str)]) -> Compound {
        let mut c = Compound::new();
        for (k, v) in pairs {
            c.insert(*k, *v);
        }
        c
    }

    #[test]
    fn plain_and_default_states() {
        let r = BlockResolver::new(mcr_data::v26_3());
        assert_eq!(r.resolve("minecraft:air", None), Resolved::Exact(0));
        assert_eq!(r.resolve("minecraft:stone", None), Resolved::Exact(1));
        assert_eq!(r.resolve("minecraft:nope", None), Resolved::UnknownBlock);
    }

    #[test]
    fn properties_select_the_matching_state() {
        let v = mcr_data::v26_3();
        let r = BlockResolver::new(v);
        let want = [
            ("half", "top"),
            ("facing", "south"),
            ("shape", "inner_left"),
        ];
        let Resolved::Exact(s) = r.resolve("minecraft:oak_stairs", Some(&props(&want))) else {
            panic!("not exact");
        };
        assert_eq!(v.state_block_name(s), Some("minecraft:oak_stairs"));
        let got: Vec<_> = v.state_properties(s).collect();
        for (k, val) in want {
            assert!(got.contains(&(k, val)), "{k}={val} in {got:?}");
        }
    }

    #[test]
    fn every_state_round_trips_through_its_properties() {
        let v = mcr_data::v26_3();
        let r = BlockResolver::new(v);
        for state in 0..v.state_count() as u32 {
            let name = v.state_block_name(state).unwrap();
            let c = props(&v.state_properties(state).collect::<Vec<_>>());
            assert_eq!(r.resolve(name, Some(&c)), Resolved::Exact(state));
        }
    }

    #[test]
    fn bad_property_falls_back_but_is_flagged() {
        let r = BlockResolver::new(mcr_data::v26_3());
        let c = props(&[("facing", "sideways")]);
        assert!(matches!(
            r.resolve("minecraft:oak_stairs", Some(&c)),
            Resolved::BadProperty(_)
        ));
    }

    #[test]
    fn biome_ids_are_dense_and_capped() {
        let t = BiomeTable::new(2);
        assert_eq!(t.intern("a"), Some(0));
        assert_eq!(t.intern("b"), Some(1));
        assert_eq!(t.intern("a"), Some(0));
        assert_eq!(t.intern("c"), None);
        assert_eq!(t.name(1).as_deref(), Some("b"));
    }
}
