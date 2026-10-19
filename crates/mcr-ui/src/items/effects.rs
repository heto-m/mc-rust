use mcr_protocol::types::component::AttributeOperation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectCategory {
    Beneficial,
    Harmful,
    Neutral,
}

/// One attribute modifier a mob effect applies per level: `amount * (amplifier + 1)`.
#[derive(Clone, Copy, Debug)]
pub struct EffectModifier {
    pub attribute: &'static str,
    pub amount: f64,
    pub operation: AttributeOperation,
}

#[derive(Clone, Copy, Debug)]
pub struct EffectInfo {
    pub name: &'static str,
    pub category: EffectCategory,
    pub color: i32,
    pub modifier: Option<EffectModifier>,
}

const fn fx(name: &'static str, category: EffectCategory, color: i32) -> EffectInfo {
    EffectInfo {
        name,
        category,
        color,
        modifier: None,
    }
}

const fn fxm(
    name: &'static str,
    category: EffectCategory,
    color: i32,
    attribute: &'static str,
    amount: f64,
    operation: AttributeOperation,
) -> EffectInfo {
    EffectInfo {
        name,
        category,
        color,
        modifier: Some(EffectModifier {
            attribute,
            amount,
            operation,
        }),
    }
}

use AttributeOperation::{AddMultipliedTotal as TOTAL, AddValue as ADD};
use EffectCategory::{Beneficial as B, Harmful as H, Neutral as N};

/// Vanilla 26.3 mob effects; amounts are the float literals vanilla widens to double.
const EFFECTS: &[EffectInfo] = &[
    fxm("speed", B, 3402751, "movement_speed", 0.2f32 as f64, TOTAL),
    fxm(
        "slowness",
        H,
        9154528,
        "movement_speed",
        -0.15f32 as f64,
        TOTAL,
    ),
    fxm("haste", B, 14270531, "attack_speed", 0.1f32 as f64, TOTAL),
    fxm(
        "mining_fatigue",
        H,
        4866583,
        "attack_speed",
        -0.1f32 as f64,
        TOTAL,
    ),
    fxm("strength", B, 16762624, "attack_damage", 3.0, ADD),
    fx("instant_health", B, 16262179),
    fx("instant_damage", H, 11101546),
    fxm("jump_boost", B, 16646020, "safe_fall_distance", 1.0, ADD),
    fx("nausea", H, 5578058),
    fx("regeneration", B, 13458603),
    fx("resistance", B, 9520880),
    fx("fire_resistance", B, 16750848),
    fx("water_breathing", B, 10017472),
    fxm(
        "invisibility",
        B,
        16185078,
        "waypoint_transmit_range",
        -1.0,
        TOTAL,
    ),
    fx("blindness", H, 2039587),
    fx("night_vision", B, 12779366),
    fx("hunger", H, 5797459),
    fxm("weakness", H, 4738376, "attack_damage", -4.0, ADD),
    fx("poison", H, 8889187),
    fx("wither", H, 7561558),
    fxm("health_boost", B, 16284963, "max_health", 4.0, ADD),
    fxm("absorption", B, 2445989, "max_absorption", 4.0, ADD),
    fx("saturation", B, 16262179),
    fx("glowing", N, 9740385),
    fx("levitation", H, 13565951),
    fxm("luck", B, 5882118, "luck", 1.0, ADD),
    fxm("unluck", H, 12624973, "luck", -1.0, ADD),
    fx("slow_falling", B, 15978425),
    fx("conduit_power", B, 1950417),
    fx("dolphins_grace", B, 8954814),
    fx("bad_omen", N, 745784),
    fx("hero_of_the_village", B, 4521796),
    fx("darkness", H, 2696993),
    fx("trial_omen", N, 1484454),
    fx("raid_omen", N, 14565464),
    fx("wind_charged", H, 12438015),
    fx("weaving", H, 7891290),
    fx("oozing", H, 10092451),
    fx("infested", H, 9214860),
    fx("breath_of_the_nautilus", B, 65518),
];

/// `(effect, duration ticks, amplifier)` of each vanilla potion.
type PotionEffects = &'static [(&'static str, i32, i32)];

const POTIONS: &[(&str, PotionEffects)] = &[
    ("water", &[]),
    ("mundane", &[]),
    ("thick", &[]),
    ("awkward", &[]),
    ("night_vision", &[("night_vision", 3600, 0)]),
    ("long_night_vision", &[("night_vision", 9600, 0)]),
    ("invisibility", &[("invisibility", 3600, 0)]),
    ("long_invisibility", &[("invisibility", 9600, 0)]),
    ("leaping", &[("jump_boost", 3600, 0)]),
    ("long_leaping", &[("jump_boost", 9600, 0)]),
    ("strong_leaping", &[("jump_boost", 1800, 1)]),
    ("fire_resistance", &[("fire_resistance", 3600, 0)]),
    ("long_fire_resistance", &[("fire_resistance", 9600, 0)]),
    ("swiftness", &[("speed", 3600, 0)]),
    ("long_swiftness", &[("speed", 9600, 0)]),
    ("strong_swiftness", &[("speed", 1800, 1)]),
    ("slowness", &[("slowness", 1800, 0)]),
    ("long_slowness", &[("slowness", 4800, 0)]),
    ("strong_slowness", &[("slowness", 400, 3)]),
    (
        "turtle_master",
        &[("slowness", 400, 3), ("resistance", 400, 2)],
    ),
    (
        "long_turtle_master",
        &[("slowness", 800, 3), ("resistance", 800, 2)],
    ),
    (
        "strong_turtle_master",
        &[("slowness", 400, 5), ("resistance", 400, 3)],
    ),
    ("water_breathing", &[("water_breathing", 3600, 0)]),
    ("long_water_breathing", &[("water_breathing", 9600, 0)]),
    ("healing", &[("instant_health", 1, 0)]),
    ("strong_healing", &[("instant_health", 1, 1)]),
    ("harming", &[("instant_damage", 1, 0)]),
    ("strong_harming", &[("instant_damage", 1, 1)]),
    ("poison", &[("poison", 900, 0)]),
    ("long_poison", &[("poison", 1800, 0)]),
    ("strong_poison", &[("poison", 432, 1)]),
    ("regeneration", &[("regeneration", 900, 0)]),
    ("long_regeneration", &[("regeneration", 1800, 0)]),
    ("strong_regeneration", &[("regeneration", 450, 1)]),
    ("strength", &[("strength", 3600, 0)]),
    ("long_strength", &[("strength", 9600, 0)]),
    ("strong_strength", &[("strength", 1800, 1)]),
    ("weakness", &[("weakness", 1800, 0)]),
    ("long_weakness", &[("weakness", 4800, 0)]),
    ("luck", &[("luck", 6000, 0)]),
    ("slow_falling", &[("slow_falling", 1800, 0)]),
    ("long_slow_falling", &[("slow_falling", 4800, 0)]),
    ("wind_charged", &[("wind_charged", 3600, 0)]),
    ("weaving", &[("weaving", 3600, 0)]),
    ("oozing", &[("oozing", 3600, 0)]),
    ("infested", &[("infested", 3600, 0)]),
];

/// `Potion#name`: the shared suffix of the base, long and strong variants.
const POTION_NAMES: &[(&str, &str)] = &[
    ("long_night_vision", "night_vision"),
    ("long_invisibility", "invisibility"),
    ("long_leaping", "leaping"),
    ("strong_leaping", "leaping"),
    ("long_fire_resistance", "fire_resistance"),
    ("long_swiftness", "swiftness"),
    ("strong_swiftness", "swiftness"),
    ("long_slowness", "slowness"),
    ("strong_slowness", "slowness"),
    ("long_turtle_master", "turtle_master"),
    ("strong_turtle_master", "turtle_master"),
    ("long_water_breathing", "water_breathing"),
    ("strong_healing", "healing"),
    ("strong_harming", "harming"),
    ("long_poison", "poison"),
    ("strong_poison", "poison"),
    ("long_regeneration", "regeneration"),
    ("strong_regeneration", "regeneration"),
    ("long_strength", "strength"),
    ("strong_strength", "strength"),
    ("long_weakness", "weakness"),
    ("long_slow_falling", "slow_falling"),
];

fn path(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

pub fn effect_info(name: &str) -> Option<&'static EffectInfo> {
    let name = path(name);
    EFFECTS.iter().find(|e| e.name == name)
}

pub fn potion_effects(name: &str) -> PotionEffects {
    let name = path(name);
    POTIONS.iter().find(|p| p.0 == name).map_or(&[], |p| p.1)
}

pub fn potion_name(name: &str) -> &str {
    let name = path(name);
    POTION_NAMES
        .iter()
        .find(|p| p.0 == name)
        .map_or(name, |p| p.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vanilla_effect_and_potion_is_known() {
        let v = mcr_data::v26_3();
        let effects = v.registry("minecraft:mob_effect").unwrap();
        for id in 0..effects.len() as u32 {
            let name = effects.name_of(id).unwrap();
            assert!(effect_info(name).is_some(), "{name}");
        }
        let potions = v.registry("minecraft:potion").unwrap();
        for id in 0..potions.len() as u32 {
            let name = potions.name_of(id).unwrap();
            assert!(POTIONS.iter().any(|p| p.0 == path(name)), "{name}");
            for (effect, _, _) in potion_effects(name) {
                assert!(effect_info(effect).is_some(), "{effect}");
            }
        }
        assert_eq!(
            potion_name("minecraft:strong_turtle_master"),
            "turtle_master"
        );
    }
}
