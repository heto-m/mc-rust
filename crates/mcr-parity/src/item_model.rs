//! Item model selection: the layers an item definition picks for a stack (model or special identity,
//! tints, local transform, glint). Vanilla's definitions plus generated ones (select, condition,
//! range dispatch, composite, specials, every tint source) baked by `ModelBakery` and updated with no
//! level and no owner, against `ItemModels::evaluate` through `StackView` (GUI) or the world path
//! (`world_layers`). Stacks are network bytes from our encoder, decoded by each side.

use std::collections::HashMap;

use mcr_assets::{
    AssetSource, DisplayContext, Identifier, ItemLayer, ItemModels, LayerKind, ResourceManager,
    SpecialKind,
};
use mcr_buf::{IdOr, Identifier as WireId, LengthPrefixed, VarInt};
use mcr_protocol::types::component::{
    ArmorTrim, BlockStateProperty, CustomModelData, DataComponent, EnchantmentLevel,
    FireworkExplosion, FireworkShape, ItemEnchantments, PotionContents, Rarity, Unit,
    component_type_id,
};
use mcr_protocol::types::item::{ItemStack, ItemStackTemplate};
use serde_json::{Map, Value, json};

use crate::{Case, Env, Harness, Result, Rng, round};

const CASES: usize = 3000;
const STEP: f64 = 1.0 / 1024.0;
const DEF_NAMESPACE: &str = "mcr_parity";
const MAX_DEPTH: u32 = 3;

/// Definitions that read the level or the clock; with no level vanilla and we agree only by accident.
const WORLD_ITEMS: &[&str] = &[
    "minecraft:compass",
    "minecraft:recovery_compass",
    "minecraft:clock",
];

/// Items whose vanilla definitions select on stack data.
const INTERESTING: &[&str] = &[
    "minecraft:bow",
    "minecraft:crossbow",
    "minecraft:trident",
    "minecraft:shield",
    "minecraft:fishing_rod",
    "minecraft:elytra",
    "minecraft:leather_helmet",
    "minecraft:leather_chestplate",
    "minecraft:leather_horse_armor",
    "minecraft:diamond_chestplate",
    "minecraft:iron_boots",
    "minecraft:turtle_helmet",
    "minecraft:potion",
    "minecraft:splash_potion",
    "minecraft:lingering_potion",
    "minecraft:tipped_arrow",
    "minecraft:firework_star",
    "minecraft:chest",
    "minecraft:trapped_chest",
    "minecraft:shulker_box",
    "minecraft:red_shulker_box",
    "minecraft:player_head",
    "minecraft:skeleton_skull",
    "minecraft:dragon_head",
    "minecraft:white_banner",
    "minecraft:black_banner",
    "minecraft:bundle",
    "minecraft:red_bundle",
    "minecraft:light",
    "minecraft:grass_block",
    "minecraft:fern",
    "minecraft:decorated_pot",
    "minecraft:conduit",
    "minecraft:copper_golem_statue",
    "minecraft:spyglass",
    "minecraft:brush",
    "minecraft:goat_horn",
    "minecraft:enchanted_book",
    "minecraft:enchanted_golden_apple",
    "minecraft:bee_nest",
    "minecraft:wolf_armor",
];

const MODELS: &[&str] = &[
    "minecraft:item/diamond",
    "minecraft:item/stick",
    "minecraft:item/apple",
    "minecraft:item/leather_helmet",
    "minecraft:item/potion",
    "minecraft:item/firework_star",
    "minecraft:item/bow_pulling_1",
    "minecraft:block/stone",
    "minecraft:block/oak_planks",
    "minecraft:block/grass_block",
    "minecraft:item/iron_sword",
    "minecraft:item/carrot",
    "minecraft:item/tipped_arrow",
    "item/leather_horse_armor",
    "minecraft:item/filled_map",
];

const SPECIAL_BASES: &[&str] = &[
    "minecraft:item/chest",
    "minecraft:item/template_skull",
    "minecraft:item/shield",
    "minecraft:item/trident",
    "minecraft:item/shulker_box",
    "minecraft:item/conduit",
    "minecraft:item/decorated_pot",
    "minecraft:item/bell",
];

const TRIM_MATERIALS: &[&str] = &[
    "amethyst",
    "copper",
    "diamond",
    "emerald",
    "gold",
    "iron",
    "lapis",
    "netherite",
    "quartz",
    "redstone",
    "resin",
];
const TRIM_PATTERNS: &[&str] = &[
    "bolt",
    "coast",
    "dune",
    "eye",
    "flow",
    "host",
    "raiser",
    "rib",
    "sentry",
    "shaper",
    "silence",
    "snout",
    "spire",
    "tide",
    "vex",
    "ward",
    "wayfinder",
    "wild",
];
pub(crate) const ENCHANTMENTS: &[&str] = &[
    "aqua_affinity",
    "bane_of_arthropods",
    "binding_curse",
    "blast_protection",
    "breach",
    "channeling",
    "density",
    "depth_strider",
    "efficiency",
    "feather_falling",
    "fire_aspect",
    "fire_protection",
    "flame",
    "fortune",
    "frost_walker",
    "impaling",
    "infinity",
    "knockback",
    "looting",
    "loyalty",
    "luck_of_the_sea",
    "lunge",
    "lure",
    "mending",
    "multishot",
    "piercing",
    "power",
    "projectile_protection",
    "protection",
    "punch",
    "quick_charge",
    "respiration",
    "riptide",
    "sharpness",
    "silk_touch",
    "smite",
    "soul_speed",
    "sweeping_edge",
    "swift_sneak",
    "thorns",
    "unbreaking",
    "vanishing_curse",
    "wind_burst",
];

const DISPLAY: &[&str] = &[
    "none",
    "thirdperson_lefthand",
    "thirdperson_righthand",
    "firstperson_lefthand",
    "firstperson_righthand",
    "head",
    "gui",
    "ground",
    "fixed",
    "on_shelf",
];

const STRINGS: &[&str] = &["a", "b", "red", "minecraft:x", "", "1"];
const COMPONENTS: &[&str] = &[
    "minecraft:damage",
    "minecraft:custom_model_data",
    "minecraft:dyed_color",
    "minecraft:enchantments",
    "minecraft:trim",
    "minecraft:max_damage",
    "minecraft:rarity",
    "minecraft:potion_contents",
];
const STATE_PROPS: &[(&str, &[&str])] = &[
    ("level", &["0", "3", "7", "15"]),
    ("honey_level", &["0", "5"]),
    ("age", &["0", "1", "2"]),
    ("facing", &["north", "east"]),
];

#[derive(Debug)]
pub struct ItemModelHarness;

/// Registry id of `name` in a list sorted the way vanilla's loader registers data-pack entries.
pub(crate) fn registry_id(names: &[&str], name: &str) -> i32 {
    let mut sorted = names.to_vec();
    sorted.sort_unstable();
    sorted.iter().position(|n| *n == name).unwrap_or(0) as i32
}

pub(crate) fn sorted_names(names: &[&str]) -> Vec<String> {
    let mut sorted: Vec<String> = names.iter().map(|n| format!("minecraft:{n}")).collect();
    sorted.sort_unstable();
    sorted
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn unhex(s: &str) -> Option<Vec<u8>> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

pub(crate) fn item_id(name: &str) -> i32 {
    mcr_data::v26_3()
        .item_by_name(name)
        .and_then(|id| i32::try_from(id).ok())
        .unwrap_or(1)
}

pub(crate) fn random_item(rng: &mut Rng) -> (i32, String) {
    let items = mcr_data::v26_3().items();
    loop {
        let id = rng.range(1, items.len() as i64 - 1) as u32;
        let name = items.name_of(id).unwrap_or("minecraft:stone");
        if !WORLD_ITEMS.contains(&name) {
            return (id as i32, name.to_owned());
        }
    }
}

fn template(item: &str, count: i32) -> ItemStackTemplate {
    ItemStackTemplate {
        item: item_id(item),
        count,
        components: Default::default(),
    }
}

fn eighths(rng: &mut Rng, lo: i64, hi: i64) -> f32 {
    rng.range(lo * 8, hi * 8) as f32 / 8.0
}

/// One generated component; `None` when its registry is unknown.
pub(crate) fn component(rng: &mut Rng, max_damage: i32) -> Option<DataComponent> {
    Some(match rng.below(15) {
        0 => DataComponent::Damage(VarInt(rng.range(0, i64::from(max_damage.max(8)) + 2) as i32)),
        1 => DataComponent::MaxDamage(VarInt(rng.range(1, 64) as i32)),
        2 => DataComponent::MaxStackSize(VarInt(rng.range(1, 99) as i32)),
        3 => DataComponent::Unbreakable(Unit),
        4 => DataComponent::CustomModelData(CustomModelData {
            floats: (0..rng.range(0, 3)).map(|_| eighths(rng, -2, 10)).collect(),
            flags: (0..rng.range(0, 3)).map(|_| rng.chance(50)).collect(),
            strings: (0..rng.range(0, 3))
                .map(|_| (*rng.pick(STRINGS)).to_owned())
                .collect(),
            colors: (0..rng.range(0, 2))
                .map(|_| rng.range(0, 0xFF_FFFF) as i32)
                .collect(),
        }),
        5 => DataComponent::DyedColor(rng.range(0, 0xFF_FFFF) as i32),
        6 => {
            let potions = mcr_data::v26_3().registry("minecraft:potion")?;
            DataComponent::PotionContents(PotionContents {
                potion: rng
                    .chance(60)
                    .then(|| VarInt(rng.below(potions.len()) as i32)),
                custom_color: rng.chance(40).then(|| rng.range(0, 0xFF_FFFF) as i32),
                custom_effects: Vec::new(),
                custom_name: None,
            })
        }
        7 => DataComponent::FireworkExplosion(FireworkExplosion {
            shape: *rng.pick(&[
                FireworkShape::SmallBall,
                FireworkShape::LargeBall,
                FireworkShape::Star,
                FireworkShape::Creeper,
                FireworkShape::Burst,
            ]),
            colors: (0..rng.range(0, 3))
                .map(|_| rng.range(0, 0xFF_FFFF) as i32)
                .collect(),
            fade_colors: (0..rng.range(0, 2))
                .map(|_| rng.range(0, 0xFF_FFFF) as i32)
                .collect(),
            has_trail: rng.chance(50),
            has_twinkle: rng.chance(50),
        }),
        8 => DataComponent::EnchantmentGlintOverride(rng.chance(50)),
        9 => DataComponent::Enchantments(ItemEnchantments(
            (0..rng.range(1, 3))
                .map(|_| EnchantmentLevel {
                    enchantment: registry_id(ENCHANTMENTS, rng.pick(ENCHANTMENTS)),
                    level: rng.range(1, 5) as i32,
                })
                .collect(),
        )),
        10 => DataComponent::BlockState(
            (0..rng.range(1, 2))
                .map(|_| {
                    let (name, values) = rng.pick(STATE_PROPS);
                    BlockStateProperty {
                        name: (*name).to_owned(),
                        value: (*rng.pick(values)).to_owned(),
                    }
                })
                .collect(),
        ),
        11 => DataComponent::ChargedProjectiles(LengthPrefixed(
            (0..rng.range(0, 2))
                .map(|_| {
                    template(
                        rng.pick(&[
                            "minecraft:arrow",
                            "minecraft:firework_rocket",
                            "minecraft:spectral_arrow",
                        ]),
                        1,
                    )
                })
                .collect(),
        )),
        12 => DataComponent::Trim(ArmorTrim {
            material: IdOr::Id(registry_id(TRIM_MATERIALS, rng.pick(TRIM_MATERIALS))),
            pattern: IdOr::Id(registry_id(TRIM_PATTERNS, rng.pick(TRIM_PATTERNS))),
        }),
        13 => DataComponent::BundleContents(
            (0..rng.range(0, 3))
                .map(|_| {
                    let (_, name) = random_item(rng);
                    template(&name, rng.range(1, 16) as i32)
                })
                .collect(),
        ),
        _ => DataComponent::Rarity(*rng.pick(&[
            Rarity::Common,
            Rarity::Uncommon,
            Rarity::Rare,
            Rarity::Epic,
        ])),
    })
}

fn ns(rng: &mut Rng, name: &str) -> String {
    if rng.chance(20) {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

fn color(rng: &mut Rng) -> Value {
    if rng.chance(10) {
        json!([
            rng.range(0, 8) as f64 / 8.0,
            rng.range(0, 8) as f64 / 8.0,
            rng.range(0, 8) as f64 / 8.0
        ])
    } else {
        json!(rng.range(0, 0xFF_FFFF))
    }
}

fn tint(rng: &mut Rng) -> Value {
    match rng.below(7) {
        0 => {
            json!({"type": ns(rng, "constant"), "value": if rng.chance(50) { json!(rng.range(i64::from(i32::MIN), i64::from(i32::MAX))) } else { color(rng) }})
        }
        1 => json!({"type": ns(rng, "dye"), "default": color(rng)}),
        2 => {
            json!({"type": ns(rng, "grass"), "temperature": rng.range(0, 8) as f64 / 8.0, "downfall": rng.range(0, 8) as f64 / 8.0})
        }
        3 => json!({"type": ns(rng, "firework"), "default": color(rng)}),
        4 => json!({"type": ns(rng, "potion"), "default": color(rng)}),
        5 => json!({"type": ns(rng, "team"), "default": color(rng)}),
        _ => {
            let mut t = json!({"type": ns(rng, "custom_model_data"), "default": color(rng)});
            if rng.chance(60) {
                t["index"] = json!(rng.range(0, 2));
            }
            t
        }
    }
}

fn quaternion(rng: &mut Rng) -> Value {
    if rng.chance(5) {
        json!({"axis": [0.0, 1.0, 0.0], "angle": rng.range(-4, 4) as f64 * 0.5})
    } else {
        let q = [
            rng.range(-4, 4) as f64 / 4.0,
            rng.range(-4, 4) as f64 / 4.0,
            rng.range(-4, 4) as f64 / 4.0,
            rng.range(1, 4) as f64 / 4.0,
        ];
        let len = q.iter().map(|v| v * v).sum::<f64>().sqrt();
        json!(q.map(|v| round(v / len, 1.0 / 4096.0)))
    }
}

fn transformation(rng: &mut Rng) -> Value {
    if rng.chance(40) {
        let m: Vec<f64> = (0..16)
            .map(|i| {
                if i % 5 == 0 {
                    rng.range(2, 8) as f64 / 4.0
                } else if rng.chance(25) {
                    rng.range(-4, 4) as f64 / 8.0
                } else {
                    0.0
                }
            })
            .collect();
        return json!(m);
    }
    let mut t = Map::new();
    t.insert(
        "translation".into(),
        json!([
            eighths(rng, -1, 1),
            eighths(rng, -1, 1),
            eighths(rng, -1, 1)
        ]),
    );
    t.insert("left_rotation".into(), quaternion(rng));
    t.insert(
        "scale".into(),
        json!([eighths(rng, 0, 2), eighths(rng, 0, 2), eighths(rng, 0, 2)]),
    );
    t.insert("right_rotation".into(), quaternion(rng));
    if rng.chance(3) {
        let keys = ["translation", "left_rotation", "scale", "right_rotation"];
        t.remove(*rng.pick(&keys));
    }
    Value::Object(t)
}

fn special_model(rng: &mut Rng) -> Value {
    let texture = |rng: &mut Rng, options: &[&str]| json!(rng.pick(options).to_owned());
    match rng.weighted(&[3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 1, 3]) {
        0 => {
            let mut m =
                json!({"type": "minecraft:banner", "color": rng.pick(&["red", "white", "blue"])});
            if rng.chance(20) {
                m["attachment"] = json!("wall");
            }
            m
        }
        1 => {
            let mut m = json!({"type": "minecraft:chest", "texture": texture(rng, &["normal", "minecraft:christmas", "ender"])});
            if rng.chance(20) {
                m["openness"] = json!(0.5);
            }
            if rng.chance(20) {
                m["chest_type"] = json!("left");
            }
            m
        }
        2 => {
            json!({"type": "minecraft:copper_golem_statue", "texture": "minecraft:textures/entity/copper_golem/copper_golem.png", "pose": rng.pick(&["standing", "sitting", "running", "star"])})
        }
        3 => {
            let mut m = json!({"type": "minecraft:head", "kind": rng.pick(&["skeleton", "zombie", "creeper", "dragon", "piglin", "wither_skeleton"])});
            if rng.chance(20) {
                m["texture"] = json!("minecraft:entity/zombie/zombie");
            }
            if rng.chance(20) {
                m["animation"] = json!(0.25);
            }
            m
        }
        4 => {
            let mut m = json!({"type": "minecraft:shulker_box", "texture": texture(rng, &["shulker", "shulker_red"])});
            if rng.chance(20) {
                m["openness"] = json!(0.5);
            }
            m
        }
        5 => json!({"type": "minecraft:conduit"}),
        6 => json!({"type": "minecraft:decorated_pot"}),
        7 => json!({"type": "minecraft:player_head"}),
        8 => json!({"type": "minecraft:shield"}),
        9 => json!({"type": "minecraft:trident"}),
        10 => json!({"type": "minecraft:bell"}),
        11 => json!({"type": "minecraft:book", "open_angle": 0.5, "page1": 0.25, "page2": 0.75}),
        12 => json!({"type": "minecraft:end_cube", "effect": rng.pick(&["portal", "gateway"])}),
        13 => json!({"type": "minecraft:bed", "texture": "minecraft:red"}),
        _ => json!({"type": "shield"}),
    }
}

fn index_prop(rng: &mut Rng, property: &str) -> Value {
    let mut v = json!({"property": property});
    if rng.chance(60) {
        v["index"] = json!(rng.range(0, 2));
    }
    v
}

fn condition(rng: &mut Rng) -> Value {
    match rng.below(5) {
        0 => index_prop(rng, "minecraft:custom_model_data"),
        1 => json!({"property": ns(rng, "broken")}),
        2 => json!({"property": "minecraft:damaged"}),
        3 => {
            let mut v =
                json!({"property": "minecraft:has_component", "component": rng.pick(COMPONENTS)});
            if rng.chance(50) {
                v["ignore_default"] = json!(rng.chance(50));
            }
            v
        }
        _ => json!({"property": "minecraft:using_item"}),
    }
}

/// A case's `when`; values already taken by an earlier case are mostly avoided, since vanilla rejects
/// a select that repeats one.
fn when<'a>(rng: &mut Rng, values: &[&'a str], used: &mut Vec<&'a str>) -> Value {
    let mut pick = |rng: &mut Rng| {
        let mut v = *rng.pick(values);
        for _ in 0..8 {
            if !used.contains(&v) || rng.chance(3) {
                break;
            }
            v = *rng.pick(values);
        }
        used.push(v);
        v
    };
    if rng.chance(30) {
        let n = rng.range(1, 2);
        json!((0..n).map(|_| pick(rng)).collect::<Vec<_>>())
    } else {
        json!(pick(rng))
    }
}

fn select(rng: &mut Rng, depth: u32) -> Value {
    let (mut v, values): (Value, Vec<&str>) = match rng.below(6) {
        0 => (
            index_prop(rng, "minecraft:custom_model_data"),
            STRINGS.to_vec(),
        ),
        1 => (
            json!({"property": "minecraft:charge_type"}),
            vec!["none", "arrow", "rocket"],
        ),
        2 => (
            json!({"property": "minecraft:trim_material"}),
            vec![
                "minecraft:gold",
                "minecraft:iron",
                "minecraft:diamond",
                "redstone",
            ],
        ),
        3 => {
            let (name, values) = rng.pick(STATE_PROPS);
            (
                json!({"property": "minecraft:block_state", "block_state_property": name}),
                values.to_vec(),
            )
        }
        4 => (
            json!({"property": "minecraft:display_context"}),
            DISPLAY.to_vec(),
        ),
        _ => (
            json!({"property": "minecraft:component", "component": "minecraft:rarity"}),
            vec!["common", "uncommon", "rare", "epic"],
        ),
    };
    v["type"] = json!(ns(rng, "select"));
    let mut used = Vec::new();
    v["cases"] = json!(
        (0..rng.range(1, 3))
            .map(|_| json!({"when": when(rng, &values, &mut used), "model": node(rng, depth + 1)}))
            .collect::<Vec<_>>()
    );
    if rng.chance(70) {
        v["fallback"] = node(rng, depth + 1);
    }
    v
}

fn range(rng: &mut Rng, depth: u32) -> Value {
    let mut v = match rng.below(5) {
        0 => index_prop(rng, "minecraft:custom_model_data"),
        1 => json!({"property": "minecraft:damage", "normalize": rng.chance(50)}),
        2 => json!({"property": "minecraft:count", "normalize": rng.chance(50)}),
        3 => json!({"property": "minecraft:bundle/fullness"}),
        _ => json!({"property": "minecraft:cooldown"}),
    };
    v["type"] = json!("minecraft:range_dispatch");
    if rng.chance(30) {
        v["scale"] = json!(rng.range(1, 16) as f64 / 4.0);
    }
    v["entries"] = json!(
        (0..rng.range(1, 5))
            .map(|_| json!({"threshold": eighths(rng, -1, 8), "model": node(rng, depth + 1)}))
            .collect::<Vec<_>>()
    );
    if rng.chance(60) {
        v["fallback"] = node(rng, depth + 1);
    }
    v
}

fn node(rng: &mut Rng, depth: u32) -> Value {
    let weights: &[u32] = if depth >= MAX_DEPTH {
        &[70, 0, 0, 0, 0, 20, 8, 1, 1]
    } else {
        &[35, 8, 15, 15, 12, 8, 3, 2, 1]
    };
    let mut v = match rng.weighted(weights) {
        0 => {
            let mut m = json!({"type": ns(rng, "model"), "model": rng.pick(MODELS)});
            if rng.chance(50) {
                m["tints"] = json!((0..rng.range(1, 3)).map(|_| tint(rng)).collect::<Vec<_>>());
            }
            m
        }
        1 => json!({
            "type": "minecraft:composite",
            "models": (0..rng.range(1, 3)).map(|_| node(rng, depth + 1)).collect::<Vec<_>>(),
        }),
        2 => {
            let mut c = condition(rng);
            c["type"] = json!("minecraft:condition");
            c["on_true"] = node(rng, depth + 1);
            c["on_false"] = node(rng, depth + 1);
            c
        }
        3 => select(rng, depth),
        4 => range(rng, depth),
        5 => {
            json!({"type": "minecraft:special", "base": rng.pick(SPECIAL_BASES), "model": special_model(rng)})
        }
        6 => json!({"type": "minecraft:empty"}),
        7 => json!({"type": "minecraft:bundle/selected_item"}),
        _ => json!({"type": "minecraft:mcr_unknown"}),
    };
    if rng.chance(15) {
        v["transformation"] = transformation(rng);
    }
    v
}

fn def_id(id: usize) -> String {
    format!("{DEF_NAMESPACE}:case_{id}")
}

/// Definitions generated for cases, served as a resource pack the way a server pack would.
#[derive(Debug)]
struct CasePack(HashMap<String, Vec<u8>>);

impl AssetSource for CasePack {
    fn read(&self, path: &str) -> mcr_assets::Result<Option<Vec<u8>>> {
        Ok(self.0.get(path).cloned())
    }

    fn list(&self) -> mcr_assets::Result<Vec<String>> {
        Ok(self.0.keys().cloned().collect())
    }
}

fn texture_id(s: &str) -> String {
    Identifier::parse(s).to_string()
}

fn special_json(kind: &SpecialKind) -> Value {
    let mut v = json!({"type": format!("minecraft:{}", kind.name())});
    match kind {
        SpecialKind::Banner { color } => v["color"] = json!(color),
        SpecialKind::Bed { texture }
        | SpecialKind::Chest { texture }
        | SpecialKind::ShulkerBox { texture } => v["texture"] = json!(texture_id(texture)),
        SpecialKind::CopperGolemStatue { pose, texture } => {
            v["pose"] = json!(pose);
            v["texture"] = json!(texture_id(texture));
        }
        SpecialKind::Head { kind } => v["kind"] = json!(kind),
        _ => {}
    }
    v
}

fn matrix(m: &mcr_assets::Mat4) -> Value {
    json!(
        m.iter()
            .flatten()
            .map(|&c| round(f64::from(c), STEP))
            .collect::<Vec<_>>()
    )
}

fn layer(layer: &ItemLayer, foil: bool) -> Option<Value> {
    let mut v = match &layer.kind {
        LayerKind::Model(id) => json!({"model": id.to_string()}),
        LayerKind::Special { base, kind } => {
            json!({"special": special_json(kind), "base": base.to_string()})
        }
        LayerKind::BundleSelectedItem => return None,
    };
    v["tints"] = json!(
        layer
            .tints
            .iter()
            .map(|t| format!("#{t:08X}"))
            .collect::<Vec<_>>()
    );
    v["transform"] = matrix(&layer.transform);
    v["foil"] = json!(foil);
    Some(v)
}

fn display(name: &str) -> DisplayContext {
    DisplayContext::ALL
        .into_iter()
        .find(|d| d.name() == name)
        .unwrap_or(DisplayContext::Gui)
}

fn probe_case(models: &ItemModels, trims: &[String], case: &Case) -> Value {
    let Some(bytes) = case.case["stack"].as_str().and_then(unhex) else {
        return json!({"error": "decode"});
    };
    let Ok(stack) = mcr_buf::from_bytes_exact::<ItemStack>(&bytes) else {
        return json!({"error": "decode"});
    };
    let context = display(case.case["display"].as_str().unwrap_or("gui"));
    let (layers, foil) = if context == DisplayContext::Gui {
        let Some(view) = mcr_ui::StackView::new(&stack) else {
            return json!({"layers": []});
        };
        let view = view.with_trim_materials(trims);
        let ctx = mcr_assets::ItemContext::default();
        (
            models.evaluate(&Identifier::parse(view.name()), &view, &ctx),
            view.has_foil(),
        )
    } else {
        (
            mcr_entity_model::world_layers(models, &stack, context),
            mcr_entity_model::has_glint(&stack),
        )
    };
    json!({"layers": layers.iter().filter_map(|l| layer(l, foil)).collect::<Vec<_>>()})
}

impl Harness for ItemModelHarness {
    fn name(&self) -> &'static str {
        "item_model"
    }

    fn scope(&self) -> &'static [&'static str] {
        &[
            "crates/mcr-assets/src/item_def.rs",
            "crates/mcr-assets/src/item_eval.rs",
            "crates/mcr-assets/src/resources.rs",
            "crates/mcr-ui/src/items/stack.rs",
            "crates/mcr-entity-model/src/items.rs",
            "crates/mcr-entity-model/src/equipment.rs",
            "crates/mcr-protocol/src/types/component.rs",
            "crates/mcr-protocol/src/types/component",
            "crates/mcr-protocol/src/types/item.rs",
            "crates/mcr-parity/src/item_model.rs",
        ]
    }

    fn count(&self) -> usize {
        CASES
    }

    fn tolerance(&self, field: &str) -> f64 {
        if field == "transform" {
            2.0 * STEP
        } else {
            0.0
        }
    }

    fn case(&self, id: usize, rng: &mut Rng) -> Value {
        let synthetic = rng.chance(40);
        let (item, name) = if rng.chance(35) {
            let name = *rng.pick(INTERESTING);
            (item_id(name), name.to_owned())
        } else {
            random_item(rng)
        };
        let info = mcr_data::v26_3().item_info(item as u32);
        let max_damage = info.map_or(0, |i| i.max_damage as i32);
        let max_stack = info.map_or(64, |i| i32::from(i.max_stack_size));
        let mut stack = ItemStack::new(item, rng.range(1, i64::from(max_stack.max(1))) as i32);
        if rng.chance(5) {
            stack.count = rng.range(1, 99) as i32;
        }
        for _ in 0..rng.range(0, 4) {
            if let Some(c) = component(rng, max_damage)
                && stack.components.get(c.type_id()).is_none()
            {
                stack.components.added.push(c);
            }
        }
        let mut out = json!({"item": name, "display": if rng.chance(50) { "gui" } else { *rng.pick(DISPLAY) }});
        if synthetic {
            out["def_id"] = json!(def_id(id));
            out["definition"] = json!({"model": node(rng, 0)});
            stack.components.added.push(DataComponent::ItemModel(
                WireId::parse(&def_id(id)).expect("id"),
            ));
        } else if rng.chance(8) {
            let (_, other) = random_item(rng);
            let model = if rng.chance(20) {
                "minecraft:mcr_none".to_owned()
            } else {
                other
            };
            stack
                .components
                .added
                .push(DataComponent::ItemModel(WireId::parse(&model).expect("id")));
        }
        if rng.chance(8) {
            let removable = [
                "item_model",
                "enchantment_glint_override",
                "max_damage",
                "max_stack_size",
                "rarity",
            ];
            let id = component_type_id(rng.pick(&removable)).unwrap_or(0);
            if stack.components.get(id).is_none() {
                stack.components.removed.push(id);
            }
        }
        out["components"] = json!(
            stack
                .components
                .added
                .iter()
                .map(|c| c.name())
                .chain(stack.components.removed.iter().map(|_| "!removed"))
                .collect::<Vec<_>>()
        );
        out["stack"] = json!(hex(&mcr_buf::to_bytes(&stack)));
        out
    }

    fn probe(&self, env: &Env, cases: &[Case]) -> Result<Vec<Value>> {
        let mut resources = ResourceManager::vanilla(&env.client_jar)?;
        let defs: HashMap<String, Vec<u8>> = cases
            .iter()
            .filter_map(|c| {
                let id = Identifier::parse(c.case["def_id"].as_str()?);
                let path = format!("assets/{}/items/{}.json", id.namespace(), id.path());
                Some((path, c.case["definition"].to_string().into_bytes()))
            })
            .collect();
        resources.push_pack("parity-cases", Box::new(CasePack(defs)))?;
        let models = ItemModels::load(&resources)?;
        let trims = sorted_names(TRIM_MATERIALS);
        Ok(cases
            .iter()
            .map(|c| crate::guarded(|| probe_case(&models, &trims, c)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_cover_every_node_type_and_decode() {
        let cases = crate::generate(&ItemModelHarness);
        let all: String = cases.iter().map(|c| c.case.to_string()).collect();
        for needle in [
            "minecraft:select",
            "minecraft:condition",
            "minecraft:range_dispatch",
            "minecraft:composite",
            "minecraft:special",
            "\"tints\"",
            "minecraft:trim_material",
            "transformation",
        ] {
            assert!(all.contains(needle), "{needle}");
        }
        for c in &cases {
            let bytes = unhex(c.case["stack"].as_str().unwrap()).unwrap();
            mcr_buf::from_bytes_exact::<ItemStack>(&bytes).unwrap();
        }
    }

    #[test]
    fn registry_ids_follow_sorted_names() {
        assert_eq!(registry_id(TRIM_MATERIALS, "amethyst"), 0);
        assert_eq!(registry_id(&["b", "a_c", "a"], "a_c"), 1);
        assert_eq!(sorted_names(&["b", "a"]), ["minecraft:a", "minecraft:b"]);
    }
}
