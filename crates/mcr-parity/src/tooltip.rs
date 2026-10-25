//! Item tooltips: the lines and styles of `ItemStack.getTooltipLines` (vanilla's data-pack registries,
//! a player with default attributes, advanced and creative flags) against `tooltip_lines` over the
//! same stack bytes, with registry entries and tags read from the jar the way a vanilla server's
//! known packs reach our client. Lines compare as draw runs like the text harness.

use std::collections::HashMap;

use mcr_assets::ResourceManager;
use mcr_buf::{BoundedString, IdOr, Identifier as WireId, LengthPrefixed, VarInt};
use mcr_nbt::{Compound, NetworkNbt, Value as Nbt};
use mcr_protocol::types::Component;
use mcr_protocol::types::component::{
    ArmorTrim, AttributeDisplay, AttributeModifier, AttributeModifierEntry, AttributeOperation,
    BannerPatternLayer, DataComponent, DyeColor, EnchantmentLevel, EquipmentSlotGroup, Filterable,
    FireworkExplosion, FireworkShape, Fireworks, ItemAttributeModifiers, ItemEnchantments,
    ItemLore, MobEffectDetails, MobEffectInstance, PotionContents, Rarity, SuspiciousStewEffect,
    TooltipDisplay, Unit, WrittenBookContent, component_type_id,
};
use mcr_protocol::types::item::{ItemStack, ItemStackTemplate};
use mcr_ui::{Lang, RegistryRef, TooltipFlags, TooltipSource};
use serde_json::{Value, json};

use crate::item_model::{ENCHANTMENTS, hex, item_id, random_item, registry_id, unhex};
use crate::{Case, Env, Harness, Result, Rng};

const CASES: usize = 3000;
const MAX_TAG_DEPTH: u32 = 8;

const ITEMS: &[&str] = &[
    "minecraft:diamond_sword",
    "minecraft:netherite_axe",
    "minecraft:iron_pickaxe",
    "minecraft:mace",
    "minecraft:trident",
    "minecraft:bow",
    "minecraft:crossbow",
    "minecraft:shield",
    "minecraft:elytra",
    "minecraft:diamond_chestplate",
    "minecraft:leather_helmet",
    "minecraft:turtle_helmet",
    "minecraft:wolf_armor",
    "minecraft:leather_horse_armor",
    "minecraft:potion",
    "minecraft:splash_potion",
    "minecraft:lingering_potion",
    "minecraft:tipped_arrow",
    "minecraft:enchanted_book",
    "minecraft:written_book",
    "minecraft:writable_book",
    "minecraft:firework_rocket",
    "minecraft:firework_star",
    "minecraft:shulker_box",
    "minecraft:goat_horn",
    "minecraft:music_disc_cat",
    "minecraft:music_disc_pigstep",
    "minecraft:white_banner",
    "minecraft:bundle",
    "minecraft:suspicious_stew",
    "minecraft:ominous_bottle",
    "minecraft:decorated_pot",
    "minecraft:player_head",
    "minecraft:filled_map",
    "minecraft:bee_nest",
    "minecraft:tropical_fish_bucket",
    "minecraft:painting",
    "minecraft:smithing_template",
    "minecraft:coast_armor_trim_smithing_template",
    "minecraft:light",
    "minecraft:compass",
    "minecraft:golden_apple",
    "minecraft:enchanted_golden_apple",
    "minecraft:wind_charge",
    "minecraft:creeper_banner_pattern",
    "minecraft:stone",
    "minecraft:diamond",
];

/// Items whose hover text depends on operator permissions or a block entity; the fake player is not one.
const SKIPPED: &[&str] = &[
    "minecraft:command_block",
    "minecraft:chain_command_block",
    "minecraft:repeating_command_block",
    "minecraft:command_block_minecart",
    "minecraft:structure_block",
    "minecraft:jigsaw",
    "minecraft:test_block",
    "minecraft:test_instance_block",
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
const SONGS: &[&str] = &[
    "11", "13", "5", "blocks", "cat", "pigstep", "relic", "tears",
];
const INSTRUMENTS: &[&str] = &[
    "admire_goat_horn",
    "call_goat_horn",
    "dream_goat_horn",
    "feel_goat_horn",
    "ponder_goat_horn",
    "seek_goat_horn",
    "sing_goat_horn",
    "yearn_goat_horn",
];
const BANNER_PATTERNS: &[&str] = &[
    "base",
    "border",
    "bricks",
    "circle",
    "creeper",
    "cross",
    "flower",
    "globe",
    "mojang",
    "skull",
    "stripe_top",
    "triangle_top",
];
/// Registries the tooltip reads, with their whole entry lists so ids line up with vanilla's.
const SYNCED: &[&str] = &[
    "enchantment",
    "trim_material",
    "trim_pattern",
    "jukebox_song",
    "instrument",
    "banner_pattern",
];

const WORDS: &[&str] = &[
    "Sword", "of", "the", "Night", "Ω", "épée", "龍", "Lv.", "99", "  ", "§cred",
];
const KEYS: &[&str] = &[
    "item.minecraft.diamond",
    "block.minecraft.stone",
    "gui.done",
    "enchantment.minecraft.sharpness",
    "item.minecraft.mcr_missing",
];
const NAMED: &[&str] = &["red", "gold", "aqua", "dark_purple", "white", "gray"];

#[derive(Debug)]
pub struct TooltipHarness;

fn text_json(rng: &mut Rng, depth: u32) -> Value {
    let mut c = if rng.chance(15) {
        let mut t = json!({"translate": rng.pick(KEYS)});
        if rng.chance(30) {
            t["fallback"] = json!("fb %s");
        }
        t
    } else {
        let words: Vec<&str> = (0..rng.range(1, 3)).map(|_| *rng.pick(WORDS)).collect();
        json!({"text": words.join(" ")})
    };
    if rng.chance(30) {
        c["color"] = if rng.chance(70) {
            json!(rng.pick(NAMED))
        } else {
            json!(format!("#{:06X}", rng.range(0, 0xFF_FFFF)))
        };
    }
    for flag in ["bold", "italic", "underlined"] {
        if rng.chance(12) {
            c[flag] = json!(rng.chance(50));
        }
    }
    if depth < 2 && rng.chance(20) {
        c["extra"] = json!(
            (0..rng.range(1, 2))
                .map(|_| text_json(rng, depth + 1))
                .collect::<Vec<_>>()
        );
    }
    c
}

fn text(rng: &mut Rng) -> Component {
    crate::text::to_nbt(&text_json(rng, 0))
        .ok()
        .and_then(|n| Component::from_nbt(&n).ok())
        .unwrap_or_else(|| Component::from_nbt_lenient(&Nbt::String("?".into())))
}

fn dye(rng: &mut Rng) -> DyeColor {
    *rng.pick(&[
        DyeColor::White,
        DyeColor::Orange,
        DyeColor::Magenta,
        DyeColor::Lime,
        DyeColor::Gray,
        DyeColor::Cyan,
        DyeColor::Blue,
        DyeColor::Red,
        DyeColor::Black,
    ])
}

fn effect(rng: &mut Rng) -> Option<MobEffectInstance> {
    let effects = mcr_data::v26_3().registry("minecraft:mob_effect")?;
    let duration = match rng.below(4) {
        0 => -1,
        1 => rng.range(1, 40) as i32,
        2 => rng.range(20, 20 * 600) as i32,
        _ => rng.range(20 * 3600, 20 * 7200) as i32,
    };
    Some(MobEffectInstance {
        effect: rng.below(effects.len()) as i32,
        details: MobEffectDetails {
            amplifier: if rng.chance(10) {
                rng.range(5, 300) as i32
            } else {
                rng.range(0, 3) as i32
            },
            duration,
            ambient: rng.chance(10),
            show_particles: rng.chance(80),
            show_icon: rng.chance(80),
            hidden_effect: None,
        },
    })
}

fn enchantments(rng: &mut Rng) -> ItemEnchantments {
    ItemEnchantments(
        (0..rng.range(1, 4))
            .map(|_| EnchantmentLevel {
                enchantment: registry_id(ENCHANTMENTS, rng.pick(ENCHANTMENTS)),
                level: match rng.below(10) {
                    0 => rng.range(6, 255) as i32,
                    _ => rng.range(1, 5) as i32,
                },
            })
            .collect(),
    )
}

fn modifiers(rng: &mut Rng) -> Option<ItemAttributeModifiers> {
    let attributes = mcr_data::v26_3().registry("minecraft:attribute")?;
    let base = ["base_attack_damage", "base_attack_speed", "mcr_bonus"];
    Some(ItemAttributeModifiers(
        (0..rng.range(1, 3))
            .map(|_| {
                let attribute = if rng.chance(40) {
                    attributes
                        .id_of(rng.pick(&["minecraft:attack_damage", "minecraft:attack_speed"]))
                        .unwrap_or(0) as i32
                } else {
                    rng.below(attributes.len()) as i32
                };
                AttributeModifierEntry {
                    attribute,
                    modifier: AttributeModifier {
                        id: WireId::parse(&format!("minecraft:{}", rng.pick(&base))).expect("id"),
                        amount: rng.range(-40, 80) as f64 / 8.0,
                        operation: *rng.pick(&[
                            AttributeOperation::AddValue,
                            AttributeOperation::AddMultipliedBase,
                            AttributeOperation::AddMultipliedTotal,
                        ]),
                    },
                    slot: *rng.pick(&[
                        EquipmentSlotGroup::Any,
                        EquipmentSlotGroup::MainHand,
                        EquipmentSlotGroup::OffHand,
                        EquipmentSlotGroup::Head,
                        EquipmentSlotGroup::Armor,
                        EquipmentSlotGroup::Body,
                    ]),
                    display: match rng.below(10) {
                        0 => AttributeDisplay::Hidden,
                        1 => AttributeDisplay::Override(text(rng)),
                        _ => AttributeDisplay::Default,
                    },
                }
            })
            .collect(),
    ))
}

fn template(rng: &mut Rng) -> ItemStackTemplate {
    let (item, _) = random_item(rng);
    let max = mcr_data::v26_3()
        .item_info(item as u32)
        .map_or(64, |i| i64::from(i.max_stack_size));
    let mut t = ItemStackTemplate {
        item,
        count: if rng.chance(5) {
            rng.range(1, 16) as i32
        } else {
            rng.range(1, max.clamp(1, 16)) as i32
        },
        components: Default::default(),
    };
    if rng.chance(20) {
        t.components
            .added
            .push(DataComponent::CustomName(text(rng)));
    }
    t
}

fn explosion(rng: &mut Rng) -> FireworkExplosion {
    FireworkExplosion {
        shape: *rng.pick(&[
            FireworkShape::SmallBall,
            FireworkShape::LargeBall,
            FireworkShape::Star,
            FireworkShape::Creeper,
            FireworkShape::Burst,
        ]),
        colors: (0..rng.range(0, 3))
            .map(|_| {
                if rng.chance(50) {
                    *rng.pick(&[0x1E1B1B, 0xB3312C, 0x3B511A, 0xF0F0F0, 0x41CD34])
                } else {
                    rng.range(0, 0xFF_FFFF) as i32
                }
            })
            .collect(),
        fade_colors: (0..rng.range(0, 2))
            .map(|_| rng.range(0, 0xFF_FFFF) as i32)
            .collect(),
        has_trail: rng.chance(50),
        has_twinkle: rng.chance(50),
    }
}

fn component(rng: &mut Rng, max_damage: i32) -> Option<DataComponent> {
    Some(match rng.below(30) {
        0 | 1 => DataComponent::CustomName(text(rng)),
        2 => DataComponent::ItemName(text(rng)),
        3 | 4 => DataComponent::Lore(ItemLore(LengthPrefixed(
            (0..rng.range(1, 3)).map(|_| text(rng)).collect(),
        ))),
        5 => DataComponent::Rarity(*rng.pick(&[
            Rarity::Common,
            Rarity::Uncommon,
            Rarity::Rare,
            Rarity::Epic,
        ])),
        6 | 7 => DataComponent::Enchantments(enchantments(rng)),
        8 => DataComponent::StoredEnchantments(enchantments(rng)),
        9 => DataComponent::Unbreakable(Unit),
        10 => DataComponent::Damage(VarInt(rng.range(0, i64::from(max_damage.max(20))) as i32)),
        11 => DataComponent::MaxDamage(VarInt(rng.range(1, 500) as i32)),
        12 => DataComponent::DyedColor(rng.range(0, 0xFF_FFFF) as i32),
        13 => DataComponent::Trim(ArmorTrim {
            material: IdOr::Id(registry_id(TRIM_MATERIALS, rng.pick(TRIM_MATERIALS))),
            pattern: IdOr::Id(registry_id(TRIM_PATTERNS, rng.pick(TRIM_PATTERNS))),
        }),
        14 | 15 => {
            let potions = mcr_data::v26_3().registry("minecraft:potion")?;
            DataComponent::PotionContents(PotionContents {
                potion: rng
                    .chance(70)
                    .then(|| VarInt(rng.below(potions.len()) as i32)),
                custom_color: rng.chance(20).then(|| rng.range(0, 0xFF_FFFF) as i32),
                custom_effects: (0..rng.range(0, 2)).filter_map(|_| effect(rng)).collect(),
                custom_name: rng.chance(10).then(|| "mcr_custom".to_owned()),
            })
        }
        16 => {
            let mut hidden = Vec::new();
            for name in [
                "enchantments",
                "attribute_modifiers",
                "lore",
                "unbreakable",
                "dyed_color",
                "potion_contents",
                "trim",
                "jukebox_playable",
                "damage",
            ] {
                if rng.chance(25)
                    && let Some(id) = component_type_id(name)
                {
                    hidden.push(VarInt(id));
                }
            }
            DataComponent::TooltipDisplay(TooltipDisplay {
                hide_tooltip: rng.chance(10),
                hidden_components: hidden,
            })
        }
        17 => {
            let mut c = Compound::new();
            c.insert("mcr", Nbt::Int(rng.range(0, 9) as i32));
            DataComponent::CustomData(NetworkNbt(Nbt::Compound(c)))
        }
        18 => DataComponent::EnchantmentGlintOverride(rng.chance(50)),
        19 => DataComponent::ChargedProjectiles(LengthPrefixed(
            (0..rng.range(1, 3))
                .map(|_| {
                    let mut t = template(rng);
                    if rng.chance(50) {
                        t.item =
                            item_id(rng.pick(&["minecraft:arrow", "minecraft:firework_rocket"]));
                    }
                    t
                })
                .collect(),
        )),
        20 => DataComponent::BundleContents((0..rng.range(0, 3)).map(|_| template(rng)).collect()),
        21 => DataComponent::Container(LengthPrefixed(
            (0..rng.range(0, 8))
                .map(|_| rng.chance(80).then(|| template(rng)))
                .collect(),
        )),
        22 => DataComponent::Fireworks(Fireworks {
            flight_duration: rng.range(0, 4) as i32,
            explosions: LengthPrefixed((0..rng.range(0, 2)).map(|_| explosion(rng)).collect()),
        }),
        23 => DataComponent::FireworkExplosion(explosion(rng)),
        24 => DataComponent::OminousBottleAmplifier(VarInt(rng.range(0, 4) as i32)),
        25 => DataComponent::JukeboxPlayable(IdOr::Id(song_id(rng.pick(SONGS)))),
        26 => DataComponent::Instrument(IdOr::Id(registry_id(INSTRUMENTS, rng.pick(INSTRUMENTS)))),
        27 => DataComponent::AttributeModifiers(modifiers(rng)?),
        28 => DataComponent::WrittenBookContent(WrittenBookContent {
            title: Filterable {
                raw: BoundedString(format!("Book {}", rng.range(0, 99))),
                filtered: None,
            },
            author: (*rng.pick(&["Steve", "Alex", ""])).to_owned(),
            generation: rng.range(0, 3) as i32,
            pages: Vec::new(),
            resolved: rng.chance(50),
        }),
        _ => match rng.below(3) {
            0 => DataComponent::SuspiciousStewEffects(
                (0..rng.range(1, 2))
                    .filter_map(|_| {
                        let e = effect(rng)?;
                        Some(SuspiciousStewEffect {
                            effect: e.effect,
                            duration: e.details.duration.max(1),
                        })
                    })
                    .collect(),
            ),
            1 => DataComponent::BannerPatterns(
                (0..rng.range(1, 3))
                    .map(|_| BannerPatternLayer {
                        pattern: IdOr::Id(banner_id(rng.pick(BANNER_PATTERNS))),
                        color: dye(rng),
                    })
                    .collect(),
            ),
            _ => DataComponent::BaseColor(dye(rng)),
        },
    })
}

/// Jukebox songs and banner patterns are only partly listed above; their ids come from the full lists.
fn song_id(name: &str) -> i32 {
    registry_id(ALL_SONGS, name)
}

fn banner_id(name: &str) -> i32 {
    registry_id(ALL_BANNER_PATTERNS, name)
}

const ALL_SONGS: &[&str] = &[
    "11",
    "13",
    "5",
    "blocks",
    "bounce",
    "cat",
    "chirp",
    "creator",
    "creator_music_box",
    "far",
    "lava_chicken",
    "mall",
    "mellohi",
    "otherside",
    "pigstep",
    "precipice",
    "relic",
    "stal",
    "strad",
    "tears",
    "wait",
    "ward",
];
const ALL_BANNER_PATTERNS: &[&str] = &[
    "base",
    "border",
    "bricks",
    "circle",
    "creeper",
    "cross",
    "curly_border",
    "diagonal_left",
    "diagonal_right",
    "diagonal_up_left",
    "diagonal_up_right",
    "flow",
    "flower",
    "globe",
    "gradient",
    "gradient_up",
    "guster",
    "half_horizontal",
    "half_horizontal_bottom",
    "half_vertical",
    "half_vertical_right",
    "mojang",
    "piglin",
    "rhombus",
    "skull",
    "small_stripes",
    "square_bottom_left",
    "square_bottom_right",
    "square_top_left",
    "square_top_right",
    "straight_cross",
    "stripe_bottom",
    "stripe_center",
    "stripe_downleft",
    "stripe_downright",
    "stripe_left",
    "stripe_middle",
    "stripe_right",
    "stripe_top",
    "triangle_bottom",
    "triangle_top",
    "triangles_bottom",
    "triangles_top",
];

/// Data-pack registries and tags from the jar, in vanilla's id order.
#[derive(Debug, Default)]
struct JarRegistries {
    entries: HashMap<String, Vec<(String, Option<Compound>)>>,
    tags: HashMap<(String, String), Vec<i32>>,
}

impl JarRegistries {
    fn load(resources: &ResourceManager) -> Result<Self> {
        let mut out = Self::default();
        for registry in SYNCED {
            let prefix = format!("data/minecraft/{registry}/");
            let mut names: Vec<String> = resources
                .list_prefix(&prefix)
                .filter_map(|p| p.strip_prefix(&prefix)?.strip_suffix(".json"))
                .map(|n| format!("minecraft:{n}"))
                .collect();
            names.sort_unstable();
            let mut list = Vec::with_capacity(names.len());
            for name in names {
                let path = format!("{prefix}{}.json", &name["minecraft:".len()..]);
                let data = resources
                    .read(&path)?
                    .and_then(|b| mcr_nbt::json::compound_from_slice(&b));
                list.push((name, data));
            }
            out.entries.insert(format!("minecraft:{registry}"), list);
        }
        let prefix = "data/minecraft/tags/enchantment/";
        let tags: Vec<String> = resources
            .list_prefix(prefix)
            .filter_map(|p| p.strip_prefix(prefix)?.strip_suffix(".json"))
            .map(str::to_owned)
            .collect();
        for tag in tags {
            let ids = out.resolve_tag(resources, "enchantment", &tag, 0);
            out.tags.insert(
                ("minecraft:enchantment".into(), format!("minecraft:{tag}")),
                ids,
            );
        }
        Ok(out)
    }

    fn resolve_tag(
        &self,
        resources: &ResourceManager,
        dir: &str,
        tag: &str,
        depth: u32,
    ) -> Vec<i32> {
        let path = format!(
            "data/minecraft/tags/{dir}/{}.json",
            tag.trim_start_matches("minecraft:")
        );
        let Some(json) = resources
            .read(&path)
            .ok()
            .flatten()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        else {
            return Vec::new();
        };
        let entries = &self.entries[&format!("minecraft:{dir}")];
        let mut out = Vec::new();
        for v in json["values"].as_array().into_iter().flatten() {
            let name = v.as_str().or_else(|| v["id"].as_str()).unwrap_or("");
            if let Some(inner) = name.strip_prefix('#') {
                if depth < MAX_TAG_DEPTH {
                    out.extend(self.resolve_tag(resources, dir, inner, depth + 1));
                }
            } else if let Some(i) = entries.iter().position(|(n, _)| n == name) {
                out.push(i as i32);
            }
        }
        out
    }
}

impl TooltipSource for JarRegistries {
    fn entry(&self, registry: &str, id: i32) -> Option<RegistryRef<'_>> {
        let (name, data) = self.entries.get(registry)?.get(usize::try_from(id).ok()?)?;
        Some(RegistryRef {
            id,
            name,
            data: data.as_ref(),
        })
    }

    fn entry_named(&self, registry: &str, name: &str) -> Option<RegistryRef<'_>> {
        let list = self.entries.get(registry)?;
        let id = list.iter().position(|(n, _)| n == name)?;
        self.entry(registry, id as i32)
    }

    fn tag(&self, registry: &str, tag: &str) -> Option<&[i32]> {
        self.tags
            .get(&(registry.to_owned(), tag.to_owned()))
            .map(Vec::as_slice)
    }
}

/// A plain space draws no pixels, so its colour and shadow are not compared.
fn visible_style(cell: &mcr_ui::Cell) -> Value {
    let mut style = crate::text::run_style(cell, None, None);
    let s = cell.style;
    if cell.ch == ' '
        && !s.underlined()
        && !s.strikethrough()
        && !s.obfuscated()
        && let Value::Object(map) = &mut style
    {
        map.remove("color");
        map.remove("shadow");
    }
    style
}

fn probe_case(lang: &Lang, src: &JarRegistries, case: &Case) -> Value {
    let Some(bytes) = case.case["stack"].as_str().and_then(unhex) else {
        return json!({"error": "decode"});
    };
    let Ok(stack) = mcr_buf::from_bytes_exact::<ItemStack>(&bytes) else {
        return json!({"error": "decode"});
    };
    let flags = TooltipFlags {
        advanced: case.case["advanced"].as_bool().unwrap_or(false),
        creative: case.case["creative"].as_bool().unwrap_or(false),
    };
    let item = mcr_ui::ItemRef::of_stack(&stack);
    let lines: Vec<Value> = mcr_ui::tooltip_lines(&item, src, flags)
        .iter()
        .map(|line| {
            let cells = mcr_ui::flatten(lang, line);
            crate::text::runs(cells.iter().map(|c| (c.ch, visible_style(c))))
        })
        .collect();
    json!({"lines": lines})
}

impl Harness for TooltipHarness {
    fn name(&self) -> &'static str {
        "tooltip"
    }

    fn scope(&self) -> &'static [&'static str] {
        &[
            "crates/mcr-ui/src/items",
            "crates/mcr-ui/src/text.rs",
            "crates/mcr-ui/src/style.rs",
            "crates/mcr-ui/src/lang.rs",
            "crates/mcr-protocol/src/types/component.rs",
            "crates/mcr-protocol/src/types/component",
            "crates/mcr-protocol/src/types/text.rs",
            "crates/mcr-nbt/src",
            "crates/mcr-data/src",
            "crates/mcr-parity/src/tooltip.rs",
        ]
    }

    fn count(&self) -> usize {
        CASES
    }

    fn case(&self, _id: usize, rng: &mut Rng) -> Value {
        let (item, name) = loop {
            let pick = if rng.chance(50) {
                let name = *rng.pick(ITEMS);
                (item_id(name), name.to_owned())
            } else {
                random_item(rng)
            };
            if !SKIPPED.contains(&pick.1.as_str()) {
                break pick;
            }
        };
        let info = mcr_data::v26_3().item_info(item as u32);
        let max_damage = info.map_or(0, |i| i.max_damage as i32);
        let max_stack = info.map_or(64, |i| i32::from(i.max_stack_size));
        let mut stack = ItemStack::new(item, rng.range(1, i64::from(max_stack.max(1))) as i32);
        for _ in 0..rng.range(0, 5) {
            if let Some(c) = component(rng, max_damage)
                && stack.components.get(c.type_id()).is_none()
            {
                stack.components.added.push(c);
            }
        }
        if rng.chance(8) {
            let removable = [
                "attribute_modifiers",
                "enchantments",
                "rarity",
                "jukebox_playable",
                "item_name",
                "max_damage",
                "potion_contents",
            ];
            let id = component_type_id(rng.pick(&removable)).unwrap_or(0);
            if stack.components.get(id).is_none() {
                stack.components.removed.push(id);
            }
        }
        json!({
            "item": name,
            "components": stack.components.added.iter().map(DataComponent::name).collect::<Vec<_>>(),
            "removed": stack.components.removed.len(),
            "advanced": rng.chance(50),
            "creative": rng.chance(20),
            "stack": hex(&mcr_buf::to_bytes(&stack)),
        })
    }

    fn probe(&self, env: &Env, cases: &[Case]) -> Result<Vec<Value>> {
        let resources = ResourceManager::vanilla(&env.client_jar)?;
        let src = JarRegistries::load(&resources)?;
        let ui = mcr_ui::vanilla_resources(&env.client_jar, None)?;
        let lang = Lang::load(&ui, mcr_ui::DEFAULT_LANGUAGE)?;
        Ok(cases
            .iter()
            .map(|c| crate::guarded(|| probe_case(&lang, &src, c)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases_decode_and_cover_the_tooltip_components() {
        let cases = crate::generate(&TooltipHarness);
        let all: String = cases.iter().map(|c| c.case.to_string()).collect();
        for needle in [
            "\"lore\"",
            "\"enchantments\"",
            "\"attribute_modifiers\"",
            "\"tooltip_display\"",
            "\"potion_contents\"",
            "\"container\"",
            "\"advanced\":true",
        ] {
            assert!(all.contains(needle), "{needle}");
        }
        for c in &cases {
            let bytes = unhex(c.case["stack"].as_str().unwrap()).unwrap();
            mcr_buf::from_bytes_exact::<ItemStack>(&bytes).unwrap();
        }
    }

    #[test]
    fn partial_lists_take_ids_from_the_full_registries() {
        assert_eq!(song_id("11"), 0);
        assert_eq!(banner_id("triangle_top"), 40);
    }
}
