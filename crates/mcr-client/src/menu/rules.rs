//! Item facts the menus need: effective components (patch over item defaults), tags and recipe sets.

use mcr_data::Version;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{DataComponent, EquipmentSlot, HolderSet, component_type_id};

use crate::registry::Registries;

pub struct Rules<'a> {
    pub data: &'a Version,
    pub registries: &'a Registries,
    pub creative: bool,
}

pub struct Equip<'a> {
    pub slot: EquipmentSlot,
    pub allowed_entities: Option<&'a HolderSet>,
}

const CRAFT_REMAINDERS: [(&str, &str); 5] = [
    ("minecraft:water_bucket", "minecraft:bucket"),
    ("minecraft:lava_bucket", "minecraft:bucket"),
    ("minecraft:milk_bucket", "minecraft:bucket"),
    ("minecraft:dragon_breath", "minecraft:glass_bottle"),
    ("minecraft:honey_bottle", "minecraft:glass_bottle"),
];

fn slot_from_name(name: &str) -> Option<EquipmentSlot> {
    Some(match name {
        "head" => EquipmentSlot::Head,
        "chest" => EquipmentSlot::Chest,
        "legs" => EquipmentSlot::Legs,
        "feet" => EquipmentSlot::Feet,
        "body" => EquipmentSlot::Body,
        "saddle" => EquipmentSlot::Saddle,
        "mainhand" => EquipmentSlot::MainHand,
        "offhand" => EquipmentSlot::OffHand,
        _ => return None,
    })
}

impl Rules<'_> {
    fn item_id(s: &ItemStack) -> Option<u32> {
        u32::try_from(s.item).ok()
    }

    pub fn item_name(&self, s: &ItemStack) -> Option<&str> {
        self.data.item_name(Self::item_id(s)?)
    }

    pub fn is_item(&self, s: &ItemStack, name: &str) -> bool {
        self.item_name(s) == Some(name)
    }

    pub fn in_tag(&self, s: &ItemStack, tag: &str) -> bool {
        self.registries.is_in_tag("minecraft:item", tag, s.item)
    }

    /// Whether the effective components (item defaults edited by the patch) include `name`.
    pub fn has(&self, s: &ItemStack, name: &str) -> bool {
        let Some(id) = component_type_id(name) else {
            return false;
        };
        if s.components.removed.contains(&id) {
            return false;
        }
        if s.components.added.iter().any(|c| c.type_id() == id) {
            return true;
        }
        Self::item_id(s).is_some_and(|i| self.data.item_has_default_component(i, id as u32))
    }

    pub fn max_stack(&self, s: &ItemStack) -> i32 {
        for c in &s.components.added {
            if let DataComponent::MaxStackSize(v) = c {
                return v.0.clamp(1, 99);
            }
        }
        Self::item_id(s)
            .and_then(|i| self.data.item_info(i))
            .map_or(64, |i| i32::from(i.max_stack_size).max(1))
    }

    pub fn equip<'s>(&self, s: &'s ItemStack) -> Option<Equip<'s>> {
        for c in &s.components.added {
            if let DataComponent::Equippable(e) = c {
                return Some(Equip {
                    slot: e.slot,
                    allowed_entities: e.allowed_entities.as_ref(),
                });
            }
        }
        if s.components
            .removed
            .contains(&component_type_id("minecraft:equippable")?)
        {
            return None;
        }
        let slot = slot_from_name(self.data.item_equip_slot(Self::item_id(s)?)?)?;
        Some(Equip {
            slot,
            allowed_entities: None,
        })
    }

    pub fn is_bucket(&self, s: &ItemStack) -> bool {
        self.is_item(s, "minecraft:bucket")
    }

    pub fn is_bundle(&self, s: &ItemStack) -> bool {
        !s.is_empty() && self.has(s, "minecraft:bundle_contents")
    }

    pub fn is_shulker(&self, s: &ItemStack) -> bool {
        self.item_name(s)
            .is_some_and(|n| n.ends_with("shulker_box"))
    }

    pub fn is_fuel(&self, s: &ItemStack) -> bool {
        self.has(s, "minecraft:cooking_fuel")
    }

    pub fn is_damageable(&self, s: &ItemStack) -> bool {
        if self.has(s, "minecraft:unbreakable") {
            return false;
        }
        for c in &s.components.added {
            if let DataComponent::MaxDamage(v) = c {
                return v.0 > 0;
            }
        }
        Self::item_id(s)
            .and_then(|i| self.data.item_info(i))
            .is_some_and(|i| i.max_damage > 0)
    }

    pub fn is_enchanted(&self, s: &ItemStack) -> bool {
        s.components.added.iter().any(|c| match c {
            DataComponent::Enchantments(e) | DataComponent::StoredEnchantments(e) => {
                !e.0.is_empty()
            }
            _ => false,
        })
    }

    pub fn is_banner(&self, s: &ItemStack) -> bool {
        self.item_name(s).is_some_and(|n| n.ends_with("_banner"))
    }

    pub fn is_loom_dye(&self, s: &ItemStack) -> bool {
        self.in_tag(s, "minecraft:loom_dyes") && self.has(s, "minecraft:dye")
    }

    pub fn is_loom_pattern(&self, s: &ItemStack) -> bool {
        self.in_tag(s, "minecraft:loom_patterns")
            && self.has(s, "minecraft:provides_banner_patterns")
    }

    /// Banner patterns the loom offers for a banner and dye, given the pattern slot (`LoomMenu#getSelectablePatterns`).
    pub fn loom_pattern_count(&self, pattern_item: &ItemStack) -> usize {
        let registry = "minecraft:banner_pattern";
        let tag_len = |tag: &str| self.registries.tag(registry, tag).map_or(0, <[i32]>::len);
        if pattern_item.is_empty() {
            return tag_len("minecraft:no_item_required");
        }
        let explicit = component_type_id("provides_banner_patterns")
            .and_then(|id| pattern_item.components.get(id));
        match explicit {
            Some(DataComponent::ProvidesBannerPatterns(HolderSet::Tag(tag))) => {
                tag_len(tag.as_str())
            }
            Some(DataComponent::ProvidesBannerPatterns(HolderSet::Ids(ids))) => ids.len(),
            _ => self
                .item_name(pattern_item)
                .filter(|_| self.has(pattern_item, "minecraft:provides_banner_patterns"))
                .and_then(|n| {
                    n.strip_prefix("minecraft:")?
                        .strip_suffix("_banner_pattern")
                })
                .map_or(0, |n| tag_len(&format!("minecraft:pattern_item/{n}"))),
        }
    }

    pub fn in_recipe_set(&self, set: &str, s: &ItemStack) -> bool {
        self.registries.recipe_set_contains(set, s.item)
    }

    pub fn is_potion_input(&self, s: &ItemStack) -> bool {
        self.in_recipe_set("minecraft:brewing_input", s)
            || self.in_tag(s, "minecraft:brewing_potion_inputs")
    }

    /// `Item#getCraftingRemainder` for the few items that have one.
    pub fn craft_remainder(&self, s: &ItemStack) -> Option<ItemStack> {
        let name = self.item_name(s)?;
        let (_, remainder) = CRAFT_REMAINDERS.iter().find(|(from, _)| *from == name)?;
        let id = self.data.item_by_name(remainder)?;
        Some(ItemStack::new(id as i32, 1))
    }

    pub fn entity_in_set(&self, set: &HolderSet, entity_type: i32) -> bool {
        match set {
            HolderSet::Ids(ids) => ids.contains(&entity_type),
            HolderSet::Tag(tag) => {
                self.registries
                    .is_in_tag("minecraft:entity_type", tag.as_str(), entity_type)
            }
        }
    }

    pub fn has_binding_curse(&self, s: &ItemStack) -> bool {
        s.components.added.iter().any(|c| match c {
            DataComponent::Enchantments(e) => e.0.iter().any(|l| {
                self.registries
                    .get("minecraft:enchantment")
                    .and_then(|r| r.get(l.enchantment))
                    .is_some_and(|i| i.id.as_str() == "minecraft:binding_curse")
            }),
            _ => false,
        })
    }
}
