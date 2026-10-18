//! Item data components: the typed values a stack carries, keyed by `data_component_type` id.

mod common;
mod data;
mod predicate;

use mcr_buf::{
    Decode, DecodeError, Encode, IdOr, Identifier, LengthPrefixed, Result, VarInt, read_len,
    read_varint, take, write_varint,
};
use mcr_nbt::{NetworkCompound, NetworkNbt};

pub use common::{
    AttributeOperation, DyeColor, EquipmentSlot, EquipmentSlotGroup, Filterable, FireworkShape,
    GlobalPos, HolderSet, ItemUseAnimation, MapPostProcessing, Rarity, ResolvableFloat,
    ResolvableInt, SwingAnimationType, Unit,
};
pub use data::*;
pub use predicate::*;

use super::Component;
use super::entity_data::PaintingVariantData;
use super::item::ItemStackTemplate;

macro_rules! components {
    ($($id:literal $name:literal $variant:ident($ty:ty);)*) => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum DataComponent {
            $($variant($ty),)*
        }

        /// Registry names indexed by type id.
        pub const COMPONENT_NAMES: &[&str] = &[$($name,)*];

        impl DataComponent {
            pub fn type_id(&self) -> i32 {
                match self {
                    $(Self::$variant(_) => $id,)*
                }
            }

            fn encode_value(&self, out: &mut Vec<u8>) {
                match self {
                    $(Self::$variant(v) => Encode::encode(v, out),)*
                }
            }

            fn decode_value(id: i32, input: &mut &[u8]) -> Result<Self> {
                let _depth = common::DepthGuard::enter()?;
                match id {
                    $($id => Ok(Self::$variant(Decode::decode(input)?)),)*
                    _ => Err(unknown_type(id)),
                }
            }
        }
    };
}

components! {
    0 "custom_data" CustomData(NetworkNbt);
    1 "max_stack_size" MaxStackSize(VarInt);
    2 "max_damage" MaxDamage(VarInt);
    3 "damage" Damage(VarInt);
    4 "unbreakable" Unbreakable(Unit);
    5 "use_effects" UseEffects(UseEffects);
    6 "custom_name" CustomName(Component);
    7 "minimum_attack_charge" MinimumAttackCharge(f32);
    8 "damage_type" DamageType(VarInt);
    9 "item_name" ItemName(Component);
    10 "item_model" ItemModel(Identifier);
    11 "lore" Lore(ItemLore);
    12 "rarity" Rarity(Rarity);
    13 "enchantments" Enchantments(ItemEnchantments);
    14 "can_place_on" CanPlaceOn(AdventureModePredicate);
    15 "can_break" CanBreak(AdventureModePredicate);
    16 "attribute_modifiers" AttributeModifiers(ItemAttributeModifiers);
    17 "custom_model_data" CustomModelData(CustomModelData);
    18 "tooltip_display" TooltipDisplay(TooltipDisplay);
    19 "repair_cost" RepairCost(VarInt);
    20 "creative_slot_lock" CreativeSlotLock(Unit);
    21 "enchantment_glint_override" EnchantmentGlintOverride(bool);
    22 "intangible_projectile" IntangibleProjectile(NetworkNbt);
    23 "food" Food(FoodProperties);
    24 "consumable" Consumable(Consumable);
    25 "use_remainder" UseRemainder(ItemStackTemplate);
    26 "use_cooldown" UseCooldown(UseCooldown);
    27 "damage_resistant" DamageResistant(HolderSet);
    28 "tool" Tool(Tool);
    29 "weapon" Weapon(Weapon);
    30 "attack_range" AttackRange(AttackRange);
    31 "enchantable" Enchantable(VarInt);
    32 "equippable" Equippable(Equippable);
    33 "repairable" Repairable(HolderSet);
    34 "glider" Glider(Unit);
    35 "tooltip_style" TooltipStyle(Identifier);
    36 "death_protection" DeathProtection(DeathProtection);
    37 "blocks_attacks" BlocksAttacks(BlocksAttacks);
    38 "piercing_weapon" PiercingWeapon(PiercingWeapon);
    39 "kinetic_weapon" KineticWeapon(KineticWeapon);
    40 "attack_animation" AttackAnimation(SwingAnimation);
    41 "interact_animation" InteractAnimation(SwingAnimation);
    42 "additional_trade_cost" AdditionalTradeCost(VarInt);
    43 "block_transformer" BlockTransformer(VarInt);
    44 "villager_food" VillagerFood(VarInt);
    45 "stored_enchantments" StoredEnchantments(ItemEnchantments);
    46 "dye" Dye(DyeColor);
    47 "dyed_color" DyedColor(i32);
    48 "map_id" MapId(VarInt);
    49 "map_decorations" MapDecorations(NetworkNbt);
    50 "map_post_processing" MapPostProcessing(MapPostProcessing);
    51 "charged_projectiles" ChargedProjectiles(LengthPrefixed<ItemStackTemplate, 1024>);
    52 "bundle_contents" BundleContents(Vec<ItemStackTemplate>);
    53 "potion_contents" PotionContents(PotionContents);
    54 "potion_duration_scale" PotionDurationScale(f32);
    55 "suspicious_stew_effects" SuspiciousStewEffects(Vec<SuspiciousStewEffect>);
    56 "writable_book_content" WritableBookContent(WritableBookContent);
    57 "written_book_content" WrittenBookContent(WrittenBookContent);
    58 "trim" Trim(ArmorTrim);
    59 "debug_stick_state" DebugStickState(NetworkNbt);
    60 "entity_data" EntityData(TypedEntityData);
    61 "bucket_entity_data" BucketEntityData(NetworkCompound);
    62 "block_entity_data" BlockEntityData(TypedEntityData);
    63 "instrument" Instrument(IdOr<Instrument>);
    64 "provides_trim_material" ProvidesTrimMaterial(IdOr<TrimMaterial>);
    65 "ominous_bottle_amplifier" OminousBottleAmplifier(VarInt);
    66 "jukebox_playable" JukeboxPlayable(IdOr<JukeboxSong>);
    67 "provides_banner_patterns" ProvidesBannerPatterns(HolderSet);
    68 "recipes" Recipes(NetworkNbt);
    69 "lodestone_tracker" LodestoneTracker(LodestoneTracker);
    70 "firework_explosion" FireworkExplosion(FireworkExplosion);
    71 "fireworks" Fireworks(Fireworks);
    72 "profile" Profile(ResolvableProfile);
    73 "note_block_sound" NoteBlockSound(Identifier);
    74 "banner_patterns" BannerPatterns(Vec<BannerPatternLayer>);
    75 "base_color" BaseColor(DyeColor);
    76 "pot_decorations" PotDecorations(PotDecorations);
    77 "container" Container(LengthPrefixed<Option<ItemStackTemplate>, 256>);
    78 "block_state" BlockState(Vec<BlockStateProperty>);
    79 "bees" Bees(Vec<BeeOccupant>);
    80 "sulfur_cube_content" SulfurCubeContent(ItemStackTemplate);
    81 "lock" Lock(NetworkNbt);
    82 "container_loot" ContainerLoot(NetworkNbt);
    83 "break_sound" BreakSound(SoundHolder);
    84 "compostable" Compostable(Compostable);
    85 "cooking_fuel" CookingFuel(CookingFuel);
    86 "brewing_fuel" BrewingFuel(BrewingFuel);
    87 "mob_visibility" MobVisibility(MobVisibility);
    88 "villager/variant" VillagerVariant(VarInt);
    89 "wolf/variant" WolfVariant(VarInt);
    90 "wolf/sound_variant" WolfSoundVariant(VarInt);
    91 "wolf/collar" WolfCollar(DyeColor);
    92 "fox/variant" FoxVariant(VarInt);
    93 "salmon/size" SalmonSize(VarInt);
    94 "parrot/variant" ParrotVariant(VarInt);
    95 "tropical_fish/pattern" TropicalFishPattern(VarInt);
    96 "tropical_fish/base_color" TropicalFishBaseColor(DyeColor);
    97 "tropical_fish/pattern_color" TropicalFishPatternColor(DyeColor);
    98 "mooshroom/variant" MooshroomVariant(VarInt);
    99 "rabbit/variant" RabbitVariant(VarInt);
    100 "pig/variant" PigVariant(VarInt);
    101 "pig/sound_variant" PigSoundVariant(VarInt);
    102 "cow/variant" CowVariant(VarInt);
    103 "cow/sound_variant" CowSoundVariant(VarInt);
    104 "chicken/variant" ChickenVariant(VarInt);
    105 "chicken/sound_variant" ChickenSoundVariant(VarInt);
    106 "zombie_nautilus/variant" ZombieNautilusVariant(VarInt);
    107 "frog/variant" FrogVariant(VarInt);
    108 "horse/variant" HorseVariant(VarInt);
    109 "painting/variant" PaintingVariant(IdOr<PaintingVariantData>);
    110 "llama/variant" LlamaVariant(VarInt);
    111 "axolotl/variant" AxolotlVariant(VarInt);
    112 "cat/variant" CatVariant(VarInt);
    113 "cat/sound_variant" CatSoundVariant(VarInt);
    114 "cat/collar" CatCollar(DyeColor);
    115 "sheep/color" SheepColor(DyeColor);
    116 "shulker/color" ShulkerColor(DyeColor);
    117 "provides_pottery_pattern" ProvidesPotteryPattern(VarInt);
    118 "sign_text_front" SignTextFront(SignText);
    119 "sign_text_back" SignTextBack(SignText);
    120 "waxed" Waxed(Unit);
    121 "cushion/color" CushionColor(DyeColor);
}

pub const COMPONENT_COUNT: usize = COMPONENT_NAMES.len();

fn unknown_type(id: i32) -> DecodeError {
    DecodeError::InvalidValue {
        what: "data component type",
        value: id.into(),
    }
}

fn check_type(id: i32) -> Result<i32> {
    if (0..COMPONENT_COUNT as i32).contains(&id) {
        Ok(id)
    } else {
        Err(unknown_type(id))
    }
}

/// Registry id of a component name such as `minecraft:damage`.
pub fn component_type_id(name: &str) -> Option<i32> {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    COMPONENT_NAMES
        .iter()
        .position(|&n| n == name)
        .map(|i| i as i32)
}

impl DataComponent {
    pub fn name(&self) -> &'static str {
        COMPONENT_NAMES[self.type_id() as usize]
    }
}

/// Type id followed by the payload, as in a patch entry or an exact component predicate.
impl Encode for DataComponent {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.type_id());
        self.encode_value(out);
    }
}

impl Decode<'_> for DataComponent {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let id = read_varint(input)?;
        Self::decode_value(id, input)
    }
}

/// Validates one value of component type `id` without keeping it.
pub(crate) fn skip_value(id: i32, input: &mut &[u8]) -> Result<()> {
    DataComponent::decode_value(id, input).map(drop)
}

/// Components set on a stack beyond its item's defaults, and defaults it removes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataComponentPatch {
    pub added: Vec<DataComponent>,
    pub removed: Vec<i32>,
}

impl DataComponentPatch {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }

    pub fn get(&self, type_id: i32) -> Option<&DataComponent> {
        self.added.iter().find(|c| c.type_id() == type_id)
    }

    pub(crate) fn encode_with(&self, out: &mut Vec<u8>, delimited: bool) {
        write_varint(out, self.added.len() as i32);
        write_varint(out, self.removed.len() as i32);
        for component in &self.added {
            write_varint(out, component.type_id());
            if delimited {
                let mut payload = Vec::new();
                component.encode_value(&mut payload);
                write_varint(out, payload.len() as i32);
                out.extend_from_slice(&payload);
            } else {
                component.encode_value(out);
            }
        }
        for &id in &self.removed {
            write_varint(out, id);
        }
    }

    pub(crate) fn decode_with(input: &mut &[u8], delimited: bool) -> Result<Self> {
        let added_len = read_len(input, input.len())?;
        let removed_len = read_len(input, input.len())?;
        let mut added = Vec::with_capacity(mcr_buf::prealloc_capacity::<DataComponent>(added_len));
        for _ in 0..added_len {
            let id = read_varint(input)?;
            if delimited {
                check_type(id)?;
                let len = read_len(input, input.len())?;
                let mut payload = take(input, len)?;
                added.push(DataComponent::decode_value(id, &mut payload)?);
            } else {
                added.push(DataComponent::decode_value(id, input)?);
            }
        }
        let mut removed = Vec::with_capacity(mcr_buf::prealloc_capacity::<i32>(removed_len));
        for _ in 0..removed_len {
            removed.push(check_type(read_varint(input)?)?);
        }
        Ok(Self { added, removed })
    }
}

impl Encode for DataComponentPatch {
    fn encode(&self, out: &mut Vec<u8>) {
        self.encode_with(out, false);
    }
}

impl Decode<'_> for DataComponentPatch {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        Self::decode_with(input, false)
    }
}

#[cfg(test)]
mod tests {
    use mcr_buf::{from_bytes_exact, to_bytes};

    use super::*;
    use crate::test_util::roundtrip;

    fn registry_entries(report: &str) -> Vec<(i32, String)> {
        let section = &report[report.find("\"minecraft:data_component_type\"").unwrap()..];
        let end = section.find("\n    }").unwrap();
        let mut entries = Vec::new();
        for chunk in section[..end].split("\"minecraft:").skip(2) {
            let name = chunk[..chunk.find('"').unwrap()].to_owned();
            let rest = &chunk[chunk.find("\"protocol_id\":").unwrap() + 14..];
            let digits: String = rest
                .trim_start()
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            entries.push((digits.parse().unwrap(), name));
        }
        entries
    }

    #[test]
    fn names_match_registry_report() {
        let Some(path) = std::env::current_dir().ok().and_then(|dir| {
            dir.ancestors()
                .map(|d| d.join(".cache/versions/26.3/generated/reports/registries.json"))
                .find(|p| p.is_file())
        }) else {
            return;
        };
        let entries = registry_entries(&std::fs::read_to_string(path).unwrap());
        assert_eq!(entries.len(), COMPONENT_COUNT);
        for (id, name) in entries {
            assert_eq!(COMPONENT_NAMES[id as usize], name);
        }
    }

    #[test]
    fn type_ids_follow_table_order() {
        assert_eq!(component_type_id("minecraft:damage"), Some(3));
        assert_eq!(component_type_id("cushion/color"), Some(121));
        assert_eq!(component_type_id("nope"), None);
        let c = DataComponent::Waxed(Unit);
        assert_eq!((c.type_id(), c.name()), (120, "waxed"));
    }

    #[test]
    fn patch_bytes() {
        let patch = DataComponentPatch {
            added: vec![
                DataComponent::Damage(VarInt(5)),
                DataComponent::Unbreakable(Unit),
            ],
            removed: vec![1],
        };
        assert_eq!(roundtrip(&patch), [2, 1, 3, 5, 4, 1]);
    }

    #[test]
    fn unknown_component_fails() {
        assert!(from_bytes_exact::<DataComponentPatch>(&[1, 0, 122]).is_err());
        assert!(from_bytes_exact::<DataComponentPatch>(&[0, 1, 122]).is_err());
        assert!(
            from_bytes_exact::<DataComponentPatch>(&[0, 1, 0xff, 0xff, 0xff, 0xff, 0x0f]).is_err()
        );
    }

    #[test]
    fn many_components_roundtrip() {
        let patch = DataComponentPatch {
            added: vec![
                DataComponent::MaxStackSize(VarInt(16)),
                DataComponent::Rarity(Rarity::Epic),
                DataComponent::Enchantments(ItemEnchantments(vec![
                    EnchantmentLevel {
                        enchantment: 9,
                        level: 5,
                    },
                    EnchantmentLevel {
                        enchantment: 2,
                        level: 1,
                    },
                ])),
                DataComponent::Food(FoodProperties {
                    nutrition: 4,
                    saturation: 2.4,
                    can_always_eat: true,
                }),
                DataComponent::AttributeModifiers(ItemAttributeModifiers(vec![
                    AttributeModifierEntry {
                        attribute: 3,
                        modifier: AttributeModifier {
                            id: Identifier::parse("base_attack_damage").unwrap(),
                            amount: 7.0,
                            operation: AttributeOperation::AddValue,
                        },
                        slot: EquipmentSlotGroup::MainHand,
                        display: AttributeDisplay::Hidden,
                    },
                ])),
                DataComponent::TooltipDisplay(TooltipDisplay {
                    hide_tooltip: false,
                    hidden_components: vec![VarInt(13), VarInt(16)],
                }),
                DataComponent::CustomModelData(CustomModelData {
                    floats: vec![1.5],
                    flags: vec![true, false],
                    strings: vec!["a".into()],
                    colors: vec![-1],
                }),
                DataComponent::Tool(Tool {
                    rules: vec![ToolRule {
                        blocks: HolderSet::Tag(Identifier::parse("mineable/pickaxe").unwrap()),
                        speed: Some(8.0),
                        correct_for_drops: None,
                    }],
                    default_mining_speed: 1.0,
                    damage_per_block: 1,
                    can_destroy_blocks_in_creative: false,
                }),
                DataComponent::DamageResistant(HolderSet::Ids(vec![0, 7, 300])),
                DataComponent::PotionContents(PotionContents {
                    potion: Some(VarInt(4)),
                    custom_color: Some(0x00ff_00ff),
                    custom_effects: vec![MobEffectInstance {
                        effect: 1,
                        details: MobEffectDetails {
                            amplifier: 1,
                            duration: 600,
                            ambient: false,
                            show_particles: true,
                            show_icon: true,
                            hidden_effect: Some(Box::new(MobEffectDetails {
                                amplifier: 0,
                                duration: 20,
                                ambient: true,
                                show_particles: false,
                                show_icon: false,
                                hidden_effect: None,
                            })),
                        },
                    }],
                    custom_name: None,
                }),
                DataComponent::Fireworks(Fireworks {
                    flight_duration: 2,
                    explosions: LengthPrefixed(vec![FireworkExplosion {
                        shape: FireworkShape::Star,
                        colors: vec![0xff0000],
                        fade_colors: vec![],
                        has_trail: true,
                        has_twinkle: false,
                    }]),
                }),
                DataComponent::BlockState(vec![BlockStateProperty {
                    name: "facing".into(),
                    value: "north".into(),
                }]),
                DataComponent::Profile(ResolvableProfile {
                    source: ProfileSource::Partial(PartialProfile {
                        name: Some(mcr_buf::BoundedString("Steve".into())),
                        id: None,
                        properties: LengthPrefixed(vec![]),
                    }),
                    skin: SkinPatch {
                        slim_model: Some(true),
                        ..SkinPatch::default()
                    },
                }),
            ],
            removed: vec![5, 6],
        };
        roundtrip(&patch);
    }

    #[test]
    fn nbt_fallback_component() {
        let bytes = to_bytes(&DataComponent::Recipes(NetworkNbt(mcr_nbt::Value::Int(7))));
        assert_eq!(bytes, [68, 3, 0, 0, 0, 7]);
        assert_eq!(
            from_bytes_exact::<DataComponent>(&bytes).unwrap(),
            DataComponent::Recipes(NetworkNbt(mcr_nbt::Value::Int(7)))
        );
    }

    #[test]
    fn nested_container_roundtrip_and_depth_limit() {
        use crate::types::item::ItemStackTemplate;
        let mut stack = ItemStackTemplate {
            item: 1,
            count: 1,
            components: DataComponentPatch::default(),
        };
        for _ in 0..8 {
            stack = ItemStackTemplate {
                item: 2,
                count: 1,
                components: DataComponentPatch {
                    added: vec![DataComponent::BundleContents(vec![stack])],
                    removed: vec![],
                },
            };
        }
        roundtrip(&stack);

        let mut bytes = Vec::new();
        for _ in 0..5000 {
            bytes.extend_from_slice(&[1, 1, 1, 0, 52, 1]);
        }
        let err = ItemStackTemplate::decode(&mut &bytes[..]).unwrap_err();
        assert!(matches!(err, DecodeError::Other(_)));
    }

    #[test]
    fn huge_counts_fail() {
        let bytes = [0xff, 0xff, 0xff, 0xff, 0x07, 0];
        assert!(DataComponentPatch::decode(&mut &bytes[..]).is_err());
        let bytes = [0, 0xff, 0xff, 0xff, 0xff, 0x07];
        assert!(DataComponentPatch::decode(&mut &bytes[..]).is_err());
        let bytes = [1, 0, 51, 0xff, 0xff, 0xff, 0xff, 0x07];
        assert!(DataComponentPatch::decode(&mut &bytes[..]).is_err());
    }

    fn id(s: &str) -> Identifier {
        Identifier::parse(s).unwrap()
    }

    fn text(s: &str) -> Vec<u8> {
        let mut v = vec![8, 0, s.len() as u8];
        v.extend_from_slice(s.as_bytes());
        v
    }

    fn str_bytes(s: &str) -> Vec<u8> {
        let mut v = vec![s.len() as u8];
        v.extend_from_slice(s.as_bytes());
        v
    }

    fn assert_component(component: DataComponent, expected: &[&[u8]]) {
        assert_eq!(roundtrip(&component), expected.concat());
    }

    #[test]
    fn painting_variant_is_holder() {
        assert_component(DataComponent::PaintingVariant(IdOr::Id(4)), &[&[109, 5]]);
        assert_component(
            DataComponent::PaintingVariant(IdOr::Inline(PaintingVariantData {
                width: 2,
                height: 1,
                asset_id: id("a:b"),
                title: Some(Component::text("t")),
                author: None,
            })),
            &[&[109, 0, 2, 1], &str_bytes("a:b"), &[1], &text("t"), &[0]],
        );
    }

    #[test]
    fn consumable_bytes() {
        use crate::types::SoundEvent;
        assert_component(
            DataComponent::Consumable(Consumable {
                consume_seconds: 1.6,
                animation: ItemUseAnimation::Eat,
                sound: IdOr::Id(10),
                has_consume_particles: true,
                on_consume_effects: vec![
                    ConsumeEffect::ApplyEffects {
                        effects: vec![MobEffectInstance {
                            effect: 3,
                            details: MobEffectDetails {
                                amplifier: 0,
                                duration: 100,
                                ambient: false,
                                show_particles: true,
                                show_icon: true,
                                hidden_effect: None,
                            },
                        }],
                        probability: 1.0,
                    },
                    ConsumeEffect::ClearAllEffects,
                    ConsumeEffect::PlaySound(IdOr::Inline(SoundEvent {
                        location: id("a:b"),
                        fixed_range: Some(2.0),
                    })),
                ],
            }),
            &[
                &[24, 0x3f, 0xcc, 0xcc, 0xcd, 1, 11, 1, 3],
                &[0, 1, 3, 0, 100, 0, 1, 1, 0, 0x3f, 0x80, 0, 0],
                &[2],
                &[4, 0],
                &str_bytes("a:b"),
                &[1, 0x40, 0, 0, 0],
            ],
        );
    }

    #[test]
    fn equippable_bytes() {
        assert_component(
            DataComponent::Equippable(Equippable {
                slot: EquipmentSlot::Head,
                equip_sound: IdOr::Id(0),
                asset_id: Some(id("a:b")),
                camera_overlay: None,
                allowed_entities: Some(HolderSet::Tag(id("a:c"))),
                dispensable: true,
                swappable: false,
                damage_on_hurt: true,
                equip_on_interact: false,
                can_be_sheared: false,
                shearing_sound: IdOr::Id(1),
            }),
            &[
                &[32, 4, 1, 1],
                &str_bytes("a:b"),
                &[0, 1, 0],
                &str_bytes("a:c"),
                &[1, 0, 1, 0, 0, 2],
            ],
        );
    }

    #[test]
    fn attribute_modifiers_bytes() {
        assert_component(
            DataComponent::AttributeModifiers(ItemAttributeModifiers(vec![
                AttributeModifierEntry {
                    attribute: 3,
                    modifier: AttributeModifier {
                        id: id("a:m"),
                        amount: 1.0,
                        operation: AttributeOperation::AddMultipliedTotal,
                    },
                    slot: EquipmentSlotGroup::Armor,
                    display: AttributeDisplay::Override(Component::text("o")),
                },
            ])),
            &[
                &[16, 1, 3],
                &str_bytes("a:m"),
                &[0x3f, 0xf0, 0, 0, 0, 0, 0, 0, 2, 8, 2],
                &text("o"),
            ],
        );
    }

    #[test]
    fn profile_bytes() {
        assert_component(
            DataComponent::Profile(ResolvableProfile {
                source: ProfileSource::Partial(PartialProfile {
                    name: Some(mcr_buf::BoundedString("Bo".into())),
                    id: None,
                    properties: LengthPrefixed(vec![]),
                }),
                skin: SkinPatch {
                    elytra: Some(id("a:e")),
                    slim_model: Some(false),
                    ..SkinPatch::default()
                },
            }),
            &[
                &[72, 0, 1, 2, b'B', b'o', 0, 0, 0, 0, 1],
                &str_bytes("a:e"),
                &[1, 0],
            ],
        );
        let uuid = uuid::Uuid::from_u128(1);
        assert_component(
            DataComponent::Profile(ResolvableProfile {
                source: ProfileSource::Full(crate::types::profile::GameProfile {
                    id: uuid,
                    name: "N".into(),
                    properties: vec![],
                }),
                skin: SkinPatch::default(),
            }),
            &[&[72, 1], uuid.as_bytes(), &[1, b'N', 0, 0, 0, 0, 0]],
        );
    }

    #[test]
    fn sign_text_bytes() {
        let blank = text("");
        assert_component(
            DataComponent::SignTextFront(SignText {
                messages: Default::default(),
                filtered_messages: None,
                color: DyeColor::Black,
                has_glowing_text: true,
            }),
            &[&[118], &blank, &blank, &blank, &blank, &[0, 15, 1]],
        );
    }

    #[test]
    fn can_break_predicate_bytes() {
        let empty = NetworkNbt(mcr_nbt::Value::Compound(mcr_nbt::Compound::new()));
        assert_component(
            DataComponent::CanBreak(AdventureModePredicate(vec![BlockPredicate {
                blocks: Some(HolderSet::Ids(vec![1])),
                properties: Some(StatePropertiesPredicate(vec![PropertyMatcher {
                    name: "f".into(),
                    matcher: ValueMatcher::Ranged(RangedMatcher {
                        min: Some("1".into()),
                        max: None,
                    }),
                }])),
                nbt: None,
                components: DataComponentMatchers {
                    exact: vec![DataComponent::Damage(VarInt(2))],
                    partial: LengthPrefixed(vec![ComponentPredicate {
                        kind: PredicateType::AnyValue(3),
                        value: empty,
                    }]),
                },
            }])),
            &[&[
                15, 1, 1, 2, 1, 1, 1, 1, b'f', 0, 1, 1, b'1', 0, 0, 1, 3, 2, 1, 0, 3, 0x0a, 0,
            ]],
        );
    }

    #[test]
    fn fuel_and_container_bytes() {
        assert_component(
            DataComponent::CookingFuel(CookingFuel {
                burn_time: ResolvableInt::Constant(200),
                speed_multiplier: ResolvableFloat::Reference(id("a:s")),
            }),
            &[&[85, 1, 0, 0, 0, 200, 0], &str_bytes("a:s")],
        );
        assert_component(
            DataComponent::Container(LengthPrefixed(vec![
                Some(ItemStackTemplate {
                    item: 1,
                    count: 1,
                    components: DataComponentPatch::default(),
                }),
                None,
            ])),
            &[&[77, 2, 1, 1, 1, 0, 0, 0]],
        );
    }

    #[test]
    fn book_and_trim_bytes() {
        assert_component(
            DataComponent::WrittenBookContent(WrittenBookContent {
                title: Filterable {
                    raw: mcr_buf::BoundedString("T".into()),
                    filtered: None,
                },
                author: "A".into(),
                generation: 0,
                pages: vec![Filterable {
                    raw: Component::text("p"),
                    filtered: Some(Component::text("")),
                }],
                resolved: true,
            }),
            &[
                &[57, 1, b'T', 0, 1, b'A', 0, 1],
                &text("p"),
                &[1],
                &text(""),
                &[1],
            ],
        );
        assert_component(
            DataComponent::Trim(ArmorTrim {
                material: IdOr::Inline(TrimMaterial {
                    palette_id: id("a:p"),
                    description: Component::text(""),
                }),
                pattern: IdOr::Id(2),
            }),
            &[&[58, 0], &str_bytes("a:p"), &text(""), &[3]],
        );
    }

    #[test]
    fn weapon_and_potion_bytes() {
        assert_component(
            DataComponent::KineticWeapon(KineticWeapon {
                contact_cooldown_ticks: 10,
                delay_ticks: 0,
                dismount_conditions: Some(KineticCondition {
                    max_duration_ticks: 5,
                    min_speed: 0.0,
                    min_relative_speed: 0.0,
                }),
                knockback_conditions: None,
                damage_conditions: None,
                forward_movement: 1.0,
                damage_multiplier: 2.0,
                sound: None,
                hit_sound: Some(IdOr::Id(0)),
            }),
            &[&[
                39, 10, 0, 1, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x3f, 0x80, 0, 0, 0x40, 0, 0, 0, 0,
                1, 1,
            ]],
        );
        assert_component(
            DataComponent::PotionContents(PotionContents {
                potion: Some(VarInt(5)),
                custom_color: None,
                custom_effects: vec![],
                custom_name: Some("x".into()),
            }),
            &[&[53, 1, 5, 0, 0, 1, 1, b'x']],
        );
    }
}
