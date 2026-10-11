//! Vanilla's entity class hierarchy, as far as `handleEntityEvent` dispatch needs it.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Class {
    pub living: bool,
    pub mob: bool,
    pub animal: bool,
    pub tamable: bool,
    pub horse: bool,
}

const NON_LIVING: &[&str] = &[
    "area_effect_cloud",
    "arrow",
    "breeze_wind_charge",
    "chest_minecart",
    "command_block_minecart",
    "cushion",
    "dragon_fireball",
    "egg",
    "ender_pearl",
    "end_crystal",
    "evoker_fangs",
    "experience_bottle",
    "experience_orb",
    "eye_of_ender",
    "falling_block",
    "fireball",
    "firework_rocket",
    "furnace_minecart",
    "glow_item_frame",
    "hopper_minecart",
    "interaction",
    "item",
    "item_frame",
    "leash_knot",
    "lightning_bolt",
    "llama_spit",
    "marker",
    "minecart",
    "ominous_item_spawner",
    "painting",
    "splash_potion",
    "lingering_potion",
    "shulker_bullet",
    "small_fireball",
    "snowball",
    "spawner_minecart",
    "spectral_arrow",
    "tnt",
    "tnt_minecart",
    "trident",
    "wind_charge",
    "wither_skull",
    "fishing_bobber",
    "block_display",
    "item_display",
    "text_display",
];

const LIVING_NOT_MOB: &[&str] = &["armor_stand", "player", "mannequin"];

const ANIMALS: &[&str] = &[
    "armadillo",
    "axolotl",
    "bee",
    "camel",
    "camel_husk",
    "cat",
    "chicken",
    "cow",
    "donkey",
    "fox",
    "frog",
    "happy_ghast",
    "goat",
    "hoglin",
    "horse",
    "llama",
    "mooshroom",
    "mule",
    "nautilus",
    "ocelot",
    "panda",
    "parrot",
    "pig",
    "polar_bear",
    "rabbit",
    "sheep",
    "skeleton_horse",
    "sniffer",
    "strider",
    "trader_llama",
    "turtle",
    "wolf",
    "zombie_horse",
    "zombie_nautilus",
];

const TAMABLE: &[&str] = &["cat", "wolf", "parrot", "nautilus", "zombie_nautilus"];

const HORSES: &[&str] = &[
    "camel",
    "camel_husk",
    "donkey",
    "horse",
    "llama",
    "mule",
    "skeleton_horse",
    "trader_llama",
    "zombie_horse",
];

/// `name` without the `minecraft:` prefix.
pub fn class_of(name: &str) -> Class {
    let boat = name.ends_with("_boat") || name.ends_with("_raft");
    let living = !boat && !NON_LIVING.contains(&name);
    Class {
        living,
        mob: living && !LIVING_NOT_MOB.contains(&name),
        animal: ANIMALS.contains(&name),
        tamable: TAMABLE.contains(&name),
        horse: HORSES.contains(&name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classes_follow_the_vanilla_hierarchy() {
        let wolf = class_of("wolf");
        assert!(wolf.living && wolf.mob && wolf.animal && wolf.tamable && !wolf.horse);
        let camel = class_of("camel");
        assert!(camel.animal && camel.horse && !camel.tamable);
        assert!(!class_of("oak_boat").living);
        assert!(!class_of("snowball").living);
        let stand = class_of("armor_stand");
        assert!(stand.living && !stand.mob);
        assert!(class_of("zombie").mob && !class_of("zombie").animal);
    }

    #[test]
    fn every_registry_type_is_classified_consistently() {
        let data = mcr_data::v26_3();
        let mut id = 0;
        while let Some(name) = data.entity_type_name(id) {
            let c = class_of(name.strip_prefix("minecraft:").unwrap_or(name));
            assert!(!c.animal || c.mob, "{name}");
            assert!(!c.tamable || c.animal, "{name}");
            assert!(!c.horse || c.animal, "{name}");
            id += 1;
        }
    }
}
