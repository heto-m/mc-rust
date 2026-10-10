use mcr_buf::Identifier;
use mcr_nbt::Compound;
use mcr_protocol::common::UpdateTags;
use mcr_protocol::configuration::RegistryData;
use mcr_protocol::play::clientbound::ui::UpdateRecipes;
use mcr_protocol::types::component::HolderSet;
use rustc_hash::{FxHashMap, FxHashSet};

const MAX_REGISTRIES: usize = 256;
const MAX_ENTRIES: usize = 1 << 16;
const MAX_TOTAL_ENTRIES: usize = 1 << 17;
const MAX_ENTRY_NBT_BYTES: usize = 1 << 16;
const MAX_TOTAL_NBT_BYTES: usize = 1 << 24;
const MAX_TOTAL_TAGS: usize = 1 << 16;
const MAX_TAG_REGISTRIES: usize = 256;
const MAX_TAGS_PER_REGISTRY: usize = 1 << 14;
const MAX_TAG_IDS: usize = 1 << 20;
const MAX_RECIPE_SETS: usize = 64;
const MAX_RECIPE_SET_ITEMS: usize = 1 << 14;
const MAX_STONECUTTER_INPUTS: usize = 1 << 12;

pub const DIMENSION_TYPE: &str = "minecraft:dimension_type";
pub const BIOME: &str = "minecraft:worldgen/biome";
pub const CHAT_TYPE: &str = "minecraft:chat_type";

#[derive(Debug, Clone, PartialEq)]
pub struct RegistryItem {
    pub id: Identifier,
    pub data: Option<Compound>,
}

/// Entry order is the network id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Registry {
    pub entries: Vec<RegistryItem>,
    by_name: FxHashMap<String, u32>,
    nbt_bytes: usize,
}

impl Registry {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, id: i32) -> Option<&RegistryItem> {
        self.entries.get(usize::try_from(id).ok()?)
    }

    pub fn id_of(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Registries {
    registries: FxHashMap<String, Registry>,
    tags: FxHashMap<String, FxHashMap<String, Vec<i32>>>,
    recipe_sets: FxHashMap<String, FxHashSet<i32>>,
    stonecutter_inputs: Vec<HolderSet>,
    pub enabled_features: Vec<String>,
}

impl Registries {
    pub fn get(&self, name: &str) -> Option<&Registry> {
        self.registries.get(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.registries.keys().map(String::as_str)
    }

    pub fn tag(&self, registry: &str, tag: &str) -> Option<&[i32]> {
        self.tags.get(registry)?.get(tag).map(Vec::as_slice)
    }

    pub fn is_in_tag(&self, registry: &str, tag: &str, id: i32) -> bool {
        self.tag(registry, tag).is_some_and(|ids| ids.contains(&id))
    }

    pub fn apply_recipes(&mut self, packet: &UpdateRecipes) {
        self.recipe_sets.clear();
        for (key, items) in packet.item_sets.0.iter().take(MAX_RECIPE_SETS) {
            let set = items
                .iter()
                .take(MAX_RECIPE_SET_ITEMS)
                .map(|v| v.0)
                .collect();
            self.recipe_sets.insert(key.as_str().to_owned(), set);
        }
        self.stonecutter_inputs = packet
            .stonecutter_recipes
            .iter()
            .take(MAX_STONECUTTER_INPUTS)
            .map(|r| r.input.clone())
            .collect();
    }

    /// Whether an item is in a synced recipe property set such as `minecraft:furnace_input`.
    pub fn recipe_set_contains(&self, set: &str, item: i32) -> bool {
        self.recipe_sets.get(set).is_some_and(|s| s.contains(&item))
    }

    pub fn stonecutter_accepts(&self, item: i32) -> bool {
        self.stonecutter_inputs.iter().any(|input| match input {
            HolderSet::Ids(ids) => ids.contains(&item),
            HolderSet::Tag(tag) => self.is_in_tag("minecraft:item", tag.as_str(), item),
        })
    }

    pub fn tag_registries(&self) -> impl Iterator<Item = &str> {
        self.tags.keys().map(String::as_str)
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn apply_registry_data(&mut self, packet: &RegistryData) {
        let name = packet.registry.as_str().to_owned();
        if !self.registries.contains_key(&name) && self.registries.len() >= MAX_REGISTRIES {
            return;
        }
        let (mut entries_used, mut bytes_used) = (0usize, 0usize);
        for (n, r) in &self.registries {
            if *n != name {
                entries_used += r.entries.len();
                bytes_used += r.nbt_bytes;
            }
        }
        let mut registry = Registry::default();
        for (index, entry) in packet.entries.iter().take(MAX_ENTRIES).enumerate() {
            if entries_used + index >= MAX_TOTAL_ENTRIES {
                break;
            }
            let mut data = entry
                .data
                .as_ref()
                .and_then(|nbt| nbt.0.clone().into_compound());
            if let Some(compound) = &data {
                let mut bytes = Vec::new();
                mcr_nbt::write_network(
                    &mut bytes,
                    Some(&mcr_nbt::Value::Compound(compound.clone())),
                );
                if bytes.len() > MAX_ENTRY_NBT_BYTES
                    || bytes_used + registry.nbt_bytes + bytes.len() > MAX_TOTAL_NBT_BYTES
                {
                    data = None;
                } else {
                    registry.nbt_bytes += bytes.len();
                }
            }
            registry
                .by_name
                .insert(entry.id.as_str().to_owned(), index as u32);
            registry.entries.push(RegistryItem {
                id: entry.id.clone(),
                data,
            });
        }
        self.registries.insert(name, registry);
    }

    pub fn apply_tags(&mut self, packet: &UpdateTags) {
        self.tags.clear();
        let mut budget = MAX_TAG_IDS;
        let mut tag_budget = MAX_TOTAL_TAGS;
        for registry in packet.registries.iter().take(MAX_TAG_REGISTRIES) {
            let mut tags = FxHashMap::default();
            for tag in registry.tags.iter().take(MAX_TAGS_PER_REGISTRY) {
                if tag.ids.0.len() > budget || tag_budget == 0 {
                    break;
                }
                budget -= tag.ids.0.len();
                tag_budget -= 1;
                tags.insert(tag.name.as_str().to_owned(), tag.ids.0.clone());
            }
            self.tags
                .insert(registry.registry.as_str().to_owned(), tags);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    pub name: Identifier,
    pub type_id: i32,
    pub min_y: i32,
    pub height: i32,
    pub logical_height: i32,
    pub sea_level: i32,
    pub seed: i64,
    pub is_flat: bool,
    pub is_debug: bool,
}

impl Default for Dimension {
    fn default() -> Self {
        Self {
            name: Identifier::parse("minecraft:overworld").expect("valid"),
            type_id: 0,
            min_y: -64,
            height: 384,
            logical_height: 384,
            sea_level: 63,
            seed: 0,
            is_flat: false,
            is_debug: false,
        }
    }
}

/// `(min_y, height, logical_height)`; used when the server skipped the data through known packs.
fn vanilla_dimension_shape(path: &str) -> Option<(i32, i32, i32)> {
    Some(match path {
        "overworld" => (-64, 384, 384),
        "overworld_caves" => (-64, 384, 192),
        "the_nether" => (0, 256, 128),
        "the_end" => (0, 256, 256),
        _ => return None,
    })
}

pub fn dimension_shape(registries: &Registries, type_id: i32) -> Option<(i32, i32, i32)> {
    let item = registries.get(DIMENSION_TYPE)?.get(type_id)?;
    let from_nbt = item.data.as_ref().and_then(|c| {
        let min_y = c.get_i32("min_y")?;
        let height = c.get_i32("height")?;
        let logical = c.get_i32("logical_height").unwrap_or(height);
        Some((min_y, height, logical))
    });
    from_nbt.or_else(|| vanilla_dimension_shape(item.id.path()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_nbt::{NetworkNbt, Value};
    use mcr_protocol::types::registry::RegistryEntry;

    fn entry(id: &str, data: Option<Compound>) -> RegistryEntry {
        RegistryEntry {
            id: Identifier::parse(id).unwrap(),
            data: data.map(|c| NetworkNbt(Value::Compound(c))),
        }
    }

    #[test]
    fn total_entries_are_capped_across_registries_and_packets() {
        let mut regs = Registries::default();
        for r in 0..200 {
            regs.apply_registry_data(&RegistryData {
                registry: Identifier::parse(&format!("minecraft:r{r}")).unwrap(),
                entries: (0..MAX_ENTRIES)
                    .map(|i| entry(&format!("minecraft:e{i}"), None))
                    .collect(),
            });
        }
        let total: usize = regs.names().map(|n| regs.get(n).unwrap().len()).sum();
        assert!(total <= MAX_TOTAL_ENTRIES);
        regs.apply_registry_data(&RegistryData {
            registry: Identifier::parse("minecraft:r0").unwrap(),
            entries: vec![entry("minecraft:a", None)],
        });
        assert_eq!(regs.get("minecraft:r0").unwrap().len(), 1);
    }

    #[test]
    fn oversized_entry_nbt_is_dropped() {
        let mut regs = Registries::default();
        let mut nbt = Compound::new();
        for i in 0..3 {
            nbt.insert(format!("big{i}"), "x".repeat(30_000));
        }
        regs.apply_registry_data(&RegistryData {
            registry: Identifier::parse("minecraft:r").unwrap(),
            entries: vec![entry("minecraft:a", Some(nbt))],
        });
        assert!(
            regs.get("minecraft:r")
                .unwrap()
                .get(0)
                .unwrap()
                .data
                .is_none()
        );
    }

    #[test]
    fn clearing_resets_everything() {
        let mut regs = Registries::default();
        regs.apply_registry_data(&RegistryData {
            registry: Identifier::parse("minecraft:r").unwrap(),
            entries: vec![entry("minecraft:a", None)],
        });
        regs.clear();
        assert!(regs.get("minecraft:r").is_none());
    }

    #[test]
    fn network_ids_follow_entry_order() {
        let mut regs = Registries::default();
        regs.apply_registry_data(&RegistryData {
            registry: Identifier::parse(BIOME).unwrap(),
            entries: vec![
                entry("minecraft:plains", None),
                entry("minecraft:desert", None),
            ],
        });
        let biomes = regs.get(BIOME).unwrap();
        assert_eq!(biomes.id_of("minecraft:desert"), Some(1));
        assert_eq!(biomes.get(0).unwrap().id.as_str(), "minecraft:plains");
        assert!(biomes.get(-1).is_none());
    }

    #[test]
    fn dimension_shape_prefers_nbt_then_vanilla_table() {
        let mut regs = Registries::default();
        let mut nbt = Compound::new();
        nbt.insert("min_y", -32i32);
        nbt.insert("height", 64i32);
        regs.apply_registry_data(&RegistryData {
            registry: Identifier::parse(DIMENSION_TYPE).unwrap(),
            entries: vec![
                entry("minecraft:custom", Some(nbt)),
                entry("minecraft:the_nether", None),
                entry("minecraft:unknown", None),
            ],
        });
        assert_eq!(dimension_shape(&regs, 0), Some((-32, 64, 64)));
        assert_eq!(dimension_shape(&regs, 1), Some((0, 256, 128)));
        assert_eq!(dimension_shape(&regs, 2), None);
        assert_eq!(dimension_shape(&regs, 9), None);
    }
}
