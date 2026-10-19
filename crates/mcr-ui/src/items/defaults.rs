use std::sync::OnceLock;

use mcr_protocol::types::component::{AttributeOperation, EquipmentSlotGroup, Rarity};
use rustc_hash::FxHashMap;

#[derive(Clone, Debug, PartialEq)]
pub struct DefaultModifier {
    pub attribute: String,
    pub id: String,
    pub amount: f64,
    pub operation: AttributeOperation,
    pub slot: EquipmentSlotGroup,
    pub hidden: bool,
}

/// The default components of a vanilla item that change its tooltip; the server sends only the patch.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemDefaults {
    pub rarity: Rarity,
    pub block_name: bool,
    pub modifiers: Vec<DefaultModifier>,
    pub jukebox_song: Option<String>,
    pub instrument: Option<String>,
    pub flight: Option<i32>,
    pub ominous: Option<i32>,
    pub potion_scale: Option<f32>,
    pub honey: Option<i32>,
    pub bees: bool,
    pub potion: bool,
    pub stew: bool,
}

impl Default for ItemDefaults {
    fn default() -> Self {
        Self {
            rarity: Rarity::Common,
            block_name: false,
            modifiers: Vec::new(),
            jukebox_song: None,
            instrument: None,
            flight: None,
            ominous: None,
            potion_scale: None,
            honey: None,
            bees: false,
            potion: false,
            stew: false,
        }
    }
}

fn operation(s: &str) -> Option<AttributeOperation> {
    Some(match s {
        "add_value" => AttributeOperation::AddValue,
        "add_multiplied_base" => AttributeOperation::AddMultipliedBase,
        "add_multiplied_total" => AttributeOperation::AddMultipliedTotal,
        _ => return None,
    })
}

pub fn slot_group_name(slot: EquipmentSlotGroup) -> &'static str {
    use EquipmentSlotGroup as G;
    match slot {
        G::Any => "any",
        G::MainHand => "mainhand",
        G::OffHand => "offhand",
        G::Hand => "hand",
        G::Feet => "feet",
        G::Legs => "legs",
        G::Chest => "chest",
        G::Head => "head",
        G::Armor => "armor",
        G::Body => "body",
        G::Saddle => "saddle",
    }
}

pub const SLOT_GROUPS: [EquipmentSlotGroup; 11] = {
    use EquipmentSlotGroup as G;
    [
        G::Any,
        G::MainHand,
        G::OffHand,
        G::Hand,
        G::Feet,
        G::Legs,
        G::Chest,
        G::Head,
        G::Armor,
        G::Body,
        G::Saddle,
    ]
};

pub fn slot_group(name: &str) -> Option<EquipmentSlotGroup> {
    SLOT_GROUPS
        .into_iter()
        .find(|&g| slot_group_name(g) == name)
}

fn modifier(spec: &str) -> Option<DefaultModifier> {
    let mut parts = spec.split(',');
    let attribute = parts.next()?.to_owned();
    let id = parts.next()?.to_owned();
    let amount = parts.next()?.parse().ok()?;
    let operation = operation(parts.next()?)?;
    let slot = slot_group(parts.next()?)?;
    let hidden = parts.next() == Some("hidden");
    Some(DefaultModifier {
        attribute,
        id,
        amount,
        operation,
        slot,
        hidden,
    })
}

fn parse_line(line: &str) -> Option<(&str, ItemDefaults)> {
    let mut tokens = line.split_ascii_whitespace();
    let name = tokens.next()?;
    let mut d = ItemDefaults::default();
    for token in tokens {
        let (key, value) = token.split_once('=').unwrap_or((token, ""));
        match key {
            "r1" => d.rarity = Rarity::Uncommon,
            "r2" => d.rarity = Rarity::Rare,
            "r3" => d.rarity = Rarity::Epic,
            "b" => d.block_name = true,
            "a" => d.modifiers.extend(modifier(value)),
            "j" => d.jukebox_song = Some(value.to_owned()),
            "h" => d.instrument = Some(value.to_owned()),
            "f" => d.flight = value.parse().ok(),
            "o" => d.ominous = value.parse().ok(),
            "scale" => d.potion_scale = value.parse().ok(),
            "honey" => d.honey = value.parse().ok(),
            "bees" => d.bees = true,
            "potion" => d.potion = true,
            "stew" => d.stew = true,
            _ => {}
        }
    }
    Some((name, d))
}

fn table() -> &'static FxHashMap<&'static str, ItemDefaults> {
    static TABLE: OnceLock<FxHashMap<&'static str, ItemDefaults>> = OnceLock::new();
    TABLE.get_or_init(|| {
        mcr_data::item_tooltips_26_3()
            .lines()
            .filter_map(parse_line)
            .collect()
    })
}

/// Defaults of the item named `name` (`minecraft:` prefix optional); modded items get the empty defaults.
pub fn item_defaults(name: &str) -> &'static ItemDefaults {
    static EMPTY: OnceLock<ItemDefaults> = OnceLock::new();
    let path = name.strip_prefix("minecraft:");
    path.and_then(|p| table().get(p))
        .or_else(|| (!name.contains(':')).then(|| table().get(name)).flatten())
        .unwrap_or_else(|| EMPTY.get_or_init(ItemDefaults::default))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sword_defaults_carry_its_base_modifiers() {
        let d = item_defaults("minecraft:diamond_sword");
        assert_eq!(d.rarity, Rarity::Common);
        assert_eq!(d.modifiers.len(), 2);
        assert_eq!(d.modifiers[0].attribute, "attack_damage");
        assert_eq!(d.modifiers[0].id, "base_attack_damage");
        assert_eq!(d.modifiers[0].amount, 6.0);
        assert_eq!(d.modifiers[0].slot, EquipmentSlotGroup::MainHand);
        assert!(item_defaults("minecraft:stone").block_name);
        assert_eq!(item_defaults("minecraft:elytra").rarity, Rarity::Epic);
        assert_eq!(
            item_defaults("minecraft:music_disc_cat")
                .jukebox_song
                .as_deref(),
            Some("cat")
        );
        assert_eq!(item_defaults("modded:thing"), &ItemDefaults::default());
    }
}
