//! Client-side `InteractionResult` prediction: which right-click outcomes swing the hand.

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn is_sign(block: &str) -> bool {
    block.ends_with("_sign") || block.ends_with("_hanging_sign")
}

fn is_log(block: &str) -> bool {
    !block.starts_with("stripped_")
        && ["_log", "_wood", "_stem", "_hyphae"]
            .iter()
            .any(|s| block.ends_with(s))
}

fn is_copper(block: &str) -> bool {
    block.contains("copper")
}

fn is_bone_meal_target(block: &str) -> bool {
    block.ends_with("_sapling")
        || block.ends_with("_mushroom")
        || block.ends_with("_fungus")
        || block.ends_with("_vines")
        || block.ends_with("_vines_plant")
        || block.ends_with("_propagule")
        || matches!(
            block,
            "wheat"
                | "carrots"
                | "potatoes"
                | "beetroots"
                | "melon_stem"
                | "pumpkin_stem"
                | "torchflower_crop"
                | "pitcher_crop"
                | "torchflower"
                | "grass_block"
                | "short_grass"
                | "tall_grass"
                | "fern"
                | "large_fern"
                | "cocoa"
                | "sweet_berry_bush"
                | "bamboo"
                | "bamboo_sapling"
                | "kelp"
                | "kelp_plant"
                | "sea_pickle"
                | "seagrass"
                | "moss_block"
                | "pale_moss_block"
                | "azalea"
                | "flowering_azalea"
                | "big_dripleaf"
                | "big_dripleaf_stem"
                | "small_dripleaf"
                | "glow_lichen"
                | "hanging_roots"
                | "cave_vines"
                | "cave_vines_plant"
                | "crimson_nylium"
                | "warped_nylium"
                | "nether_wart"
                | "dandelion"
                | "poppy"
                | "lilac"
                | "rose_bush"
                | "peony"
                | "sunflower"
                | "pink_petals"
                | "wildflowers"
                | "leaf_litter"
                | "bush"
                | "firefly_bush"
        )
}

/// Whether `Item#useOn` of `item` on `block` returns a swinging `SUCCESS`.
pub(crate) fn item_use_on_swings(item: &str, block: &str) -> bool {
    let item = short(item);
    let block = short(block);
    if item.ends_with("_spawn_egg") {
        return true;
    }
    if item.ends_with("_dye") {
        return is_sign(block);
    }
    if item.ends_with("_hoe") {
        return matches!(
            block,
            "grass_block" | "dirt" | "dirt_path" | "coarse_dirt" | "rooted_dirt"
        );
    }
    if item.ends_with("_shovel") {
        return matches!(
            block,
            "grass_block"
                | "dirt"
                | "podzol"
                | "mycelium"
                | "coarse_dirt"
                | "rooted_dirt"
                | "campfire"
                | "soul_campfire"
        );
    }
    if item.ends_with("_axe") {
        return is_log(block) || block == "bamboo_block" || is_copper(block) || is_sign(block);
    }
    if item.ends_with("minecart") {
        return block.ends_with("rail");
    }
    match item {
        "flint_and_steel" | "fire_charge" | "item_frame" | "glow_item_frame" | "painting"
        | "armor_stand" => true,
        "bone_meal" => is_bone_meal_target(block),
        "honeycomb" => is_copper(block) || is_sign(block),
        "glow_ink_sac" | "ink_sac" => is_sign(block),
        "lead" => block.ends_with("_fence"),
        "end_crystal" => matches!(block, "obsidian" | "bedrock"),
        "ender_eye" => block == "end_portal_frame",
        "shears" => matches!(block, "pumpkin" | "beehive" | "bee_nest"),
        _ => false,
    }
}

/// Whether `Item#use` of an instant item returns a swinging `SUCCESS`; held and consumed uses never swing.
pub(crate) fn item_use_swings(item: &str, block_target: bool, gliding: bool) -> bool {
    let item = short(item);
    match item {
        "firework_rocket" => gliding,
        "crossbow" => false,
        _ if item.ends_with("bucket") || item.ends_with("_boat") || item.ends_with("_raft") => {
            block_target
        }
        _ => true,
    }
}

/// Whether `Player#interactOn` on the entity returns a swinging `SUCCESS` for the held item.
pub(crate) fn entity_interact_swings(entity: &str, held: Option<&str>) -> bool {
    let entity = short(entity);
    let held = held.map(short);
    if matches!(held, Some("name_tag" | "lead")) {
        return true;
    }
    if entity.ends_with("boat") || entity.ends_with("_raft") {
        return true;
    }
    if matches!(
        entity,
        "villager"
            | "wandering_trader"
            | "horse"
            | "donkey"
            | "mule"
            | "skeleton_horse"
            | "zombie_horse"
            | "camel"
            | "camel_husk"
            | "llama"
            | "trader_llama"
            | "happy_ghast"
            | "armor_stand"
            | "item_frame"
            | "glow_item_frame"
            | "minecart"
            | "allay"
            | "copper_golem"
            | "snow_golem"
            | "wolf"
            | "cat"
            | "parrot"
    ) {
        return true;
    }
    match entity {
        "pig" | "strider" => held == Some("saddle"),
        "sheep" => held == Some("shears") || held.is_some_and(|h| h.ends_with("_dye")),
        "cow" | "mooshroom" | "goat" => held == Some("bucket"),
        "chicken" | "rabbit" | "fox" | "bee" | "panda" | "ocelot" | "turtle" | "frog"
        | "axolotl" | "hoglin" | "sniffer" | "armadillo" => held.is_some_and(is_animal_food),
        _ => false,
    }
}

fn is_animal_food(item: &str) -> bool {
    item.ends_with("_seeds")
        || matches!(
            item,
            "wheat"
                | "carrot"
                | "potato"
                | "beetroot"
                | "pitcher_pod"
                | "sweet_berries"
                | "glow_berries"
                | "golden_carrot"
                | "dandelion"
                | "cod"
                | "salmon"
                | "slime_ball"
                | "crimson_fungus"
                | "cactus"
                | "spider_eye"
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_blocks_do_not_swing_for_unrelated_items() {
        assert!(!item_use_on_swings("minecraft:apple", "stone"));
        assert!(!item_use_on_swings("minecraft:diamond_hoe", "stone"));
        assert!(!item_use_on_swings("minecraft:diamond_axe", "stone"));
        assert!(!item_use_on_swings("minecraft:bone_meal", "stone"));
        assert!(!item_use_on_swings("minecraft:brush", "sand"));
    }

    #[test]
    fn tools_swing_on_convertible_blocks() {
        assert!(item_use_on_swings("minecraft:iron_hoe", "grass_block"));
        assert!(item_use_on_swings("minecraft:iron_shovel", "dirt"));
        assert!(item_use_on_swings("minecraft:iron_axe", "oak_log"));
        assert!(!item_use_on_swings(
            "minecraft:iron_axe",
            "stripped_oak_log"
        ));
        assert!(item_use_on_swings("minecraft:bone_meal", "wheat"));
        assert!(item_use_on_swings("minecraft:cow_spawn_egg", "stone"));
    }

    #[test]
    fn dyes_only_swing_on_signs() {
        assert!(item_use_on_swings("minecraft:red_dye", "oak_sign"));
        assert!(!item_use_on_swings("minecraft:red_dye", "stone"));
    }

    #[test]
    fn instant_items_swing_except_conditional_ones() {
        assert!(item_use_swings("minecraft:snowball", false, false));
        assert!(item_use_swings("minecraft:ender_pearl", false, false));
        assert!(!item_use_swings("minecraft:bucket", false, false));
        assert!(item_use_swings("minecraft:water_bucket", true, false));
        assert!(!item_use_swings("minecraft:firework_rocket", false, false));
        assert!(item_use_swings("minecraft:firework_rocket", false, true));
        assert!(!item_use_swings("minecraft:crossbow", false, false));
    }

    #[test]
    fn entities_swing_only_when_interactive() {
        assert!(entity_interact_swings("minecraft:villager", None));
        assert!(entity_interact_swings("minecraft:oak_boat", None));
        assert!(!entity_interact_swings("minecraft:zombie", None));
        assert!(!entity_interact_swings(
            "minecraft:cow",
            Some("minecraft:apple")
        ));
        assert!(entity_interact_swings(
            "minecraft:cow",
            Some("minecraft:bucket")
        ));
        assert!(entity_interact_swings(
            "minecraft:zombie",
            Some("minecraft:name_tag")
        ));
        assert!(entity_interact_swings(
            "minecraft:chicken",
            Some("minecraft:wheat_seeds")
        ));
        assert!(!entity_interact_swings("minecraft:chicken", None));
    }
}
