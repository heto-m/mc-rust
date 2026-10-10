use mcr_buf::{BoundedString, IdOr, Identifier, LengthPrefixed, VarInt};
use mcr_nbt::{Compound, NetworkCompound, NetworkNbt, Value};
use mcr_protocol::configuration::RegistryData;
use mcr_protocol::types::component::*;
use mcr_protocol::types::item::ItemStackTemplate;
use mcr_protocol::types::registry::RegistryEntry;
use mcr_protocol::types::text::{
    ClickEvent, Content, HoverEvent, NbtContent, NbtSource, ObjectContent,
};
use mcr_protocol::types::{Component, SoundEvent};

use crate::hashing::HashCx;
use crate::registry::Registries;

const DYNAMIC: &[&str] = &[
    "minecraft:enchantment",
    "minecraft:trim_material",
    "minecraft:trim_pattern",
    "minecraft:instrument",
    "minecraft:jukebox_song",
    "minecraft:banner_pattern",
    "minecraft:painting_variant",
    "minecraft:damage_type",
    "minecraft:block_transformer",
    "minecraft:decorated_pot_pattern",
    "minecraft:wolf_variant",
    "minecraft:wolf_sound_variant",
    "minecraft:pig_variant",
    "minecraft:pig_sound_variant",
    "minecraft:cow_variant",
    "minecraft:cow_sound_variant",
    "minecraft:chicken_variant",
    "minecraft:chicken_sound_variant",
    "minecraft:zombie_nautilus_variant",
    "minecraft:frog_variant",
    "minecraft:cat_variant",
    "minecraft:cat_sound_variant",
];

fn registries() -> Registries {
    let mut r = Registries::default();
    for name in DYNAMIC {
        r.apply_registry_data(&RegistryData {
            registry: id(name),
            entries: (0..4)
                .map(|i| RegistryEntry {
                    id: id(&format!("minecraft:e{i}")),
                    data: None,
                })
                .collect(),
        });
    }
    r
}

fn id(s: &str) -> Identifier {
    Identifier::parse(s).unwrap()
}

fn text(s: &str) -> Component {
    Component::text(s)
}

fn styled() -> Component {
    let mut c = Component::translatable("a.b", vec![]);
    c.style.bold = Some(true);
    c.style.click_event = Some(Box::new(ClickEvent::RunCommand("/x".into())));
    c.style.hover_event = Some(Box::new(HoverEvent::ShowText(Box::new(text("h")))));
    c.extra.push(text("e"));
    c
}

fn template() -> ItemStackTemplate {
    ItemStackTemplate {
        item: 1,
        count: 3,
        components: DataComponentPatch {
            added: vec![DataComponent::Damage(VarInt(2))],
            removed: vec![],
        },
    }
}

fn nbt() -> NetworkNbt {
    let mut c = Compound::new();
    c.insert("a", Value::Int(1));
    NetworkNbt(Value::Compound(c))
}

fn compound() -> NetworkCompound {
    let mut c = Compound::new();
    c.insert("a", Value::Byte(1));
    NetworkCompound(c)
}

fn effect() -> MobEffectInstance {
    MobEffectInstance {
        effect: 1,
        details: MobEffectDetails {
            amplifier: 2,
            duration: 100,
            ambient: true,
            show_particles: false,
            show_icon: true,
            hidden_effect: Some(Box::new(MobEffectDetails {
                amplifier: 0,
                duration: 5,
                ambient: false,
                show_particles: true,
                show_icon: false,
                hidden_effect: None,
            })),
        },
    }
}

fn sound() -> SoundHolder {
    IdOr::Id(3)
}

fn tag() -> HolderSet {
    HolderSet::Tag(id("minecraft:tag"))
}

fn explosion() -> FireworkExplosion {
    FireworkExplosion {
        shape: FireworkShape::Star,
        colors: vec![1, 2],
        fade_colors: vec![3],
        has_trail: true,
        has_twinkle: true,
    }
}

fn sign() -> SignText {
    SignText {
        messages: [text("a"), text("b"), text("c"), text("d")],
        filtered_messages: Some([text("1"), text("2"), text("3"), text("4")]),
        color: DyeColor::Red,
        has_glowing_text: true,
    }
}

fn profile() -> ResolvableProfile {
    ResolvableProfile {
        source: ProfileSource::Partial(PartialProfile {
            name: Some(BoundedString("Steve".into())),
            id: Some(uuid::Uuid::from_u128(5)),
            properties: LengthPrefixed(vec![]),
        }),
        skin: SkinPatch {
            slim_model: Some(true),
            ..SkinPatch::default()
        },
    }
}

fn kinetic_condition() -> KineticCondition {
    KineticCondition {
        max_duration_ticks: 7,
        min_speed: 1.0,
        min_relative_speed: 2.0,
    }
}

fn block_predicate() -> BlockPredicate {
    BlockPredicate {
        blocks: Some(HolderSet::Ids(vec![1, 2])),
        properties: Some(StatePropertiesPredicate(vec![PropertyMatcher {
            name: "facing".into(),
            matcher: ValueMatcher::Exact("north".into()),
        }])),
        nbt: Some(compound()),
        components: DataComponentMatchers {
            exact: vec![DataComponent::Damage(VarInt(2))],
            partial: LengthPrefixed(vec![ComponentPredicate {
                kind: PredicateType::AnyValue(3),
                value: nbt(),
            }]),
        },
    }
}

fn sample(type_id: i32) -> DataComponent {
    use DataComponent as D;
    let v = VarInt(1);
    match COMPONENT_NAMES[type_id as usize] {
        "custom_data" => D::CustomData(nbt()),
        "max_stack_size" => D::MaxStackSize(v),
        "max_damage" => D::MaxDamage(v),
        "damage" => D::Damage(v),
        "unbreakable" => D::Unbreakable(Unit),
        "use_effects" => D::UseEffects(UseEffects {
            can_sprint: true,
            interact_vibrations: false,
            speed_multiplier: 0.5,
        }),
        "custom_name" => D::CustomName(styled()),
        "minimum_attack_charge" => D::MinimumAttackCharge(0.5),
        "damage_type" => D::DamageType(v),
        "item_name" => D::ItemName(text("n")),
        "item_model" => D::ItemModel(id("minecraft:m")),
        "lore" => D::Lore(ItemLore(LengthPrefixed(vec![text("l"), styled()]))),
        "rarity" => D::Rarity(Rarity::Rare),
        "enchantments" => D::Enchantments(ItemEnchantments(vec![EnchantmentLevel {
            enchantment: 1,
            level: 3,
        }])),
        "can_place_on" => D::CanPlaceOn(AdventureModePredicate(vec![block_predicate()])),
        "can_break" => D::CanBreak(AdventureModePredicate(vec![
            block_predicate(),
            block_predicate(),
        ])),
        "attribute_modifiers" => {
            D::AttributeModifiers(ItemAttributeModifiers(vec![AttributeModifierEntry {
                attribute: 1,
                modifier: AttributeModifier {
                    id: id("minecraft:x"),
                    amount: 1.5,
                    operation: AttributeOperation::AddValue,
                },
                slot: EquipmentSlotGroup::Hand,
                display: AttributeDisplay::Override(text("o")),
            }]))
        }
        "custom_model_data" => D::CustomModelData(CustomModelData {
            floats: vec![1.0],
            flags: vec![true],
            strings: vec!["s".into()],
            colors: vec![5],
        }),
        "tooltip_display" => D::TooltipDisplay(TooltipDisplay {
            hide_tooltip: true,
            hidden_components: vec![VarInt(3)],
        }),
        "repair_cost" => D::RepairCost(v),
        "creative_slot_lock" => D::CreativeSlotLock(Unit),
        "enchantment_glint_override" => D::EnchantmentGlintOverride(true),
        "intangible_projectile" => D::IntangibleProjectile(nbt()),
        "food" => D::Food(FoodProperties {
            nutrition: 4,
            saturation: 2.5,
            can_always_eat: true,
        }),
        "consumable" => D::Consumable(Consumable {
            consume_seconds: 2.0,
            animation: ItemUseAnimation::Drink,
            sound: sound(),
            has_consume_particles: false,
            on_consume_effects: vec![
                ConsumeEffect::ApplyEffects {
                    effects: vec![effect()],
                    probability: 0.5,
                },
                ConsumeEffect::RemoveEffects(tag()),
                ConsumeEffect::ClearAllEffects,
                ConsumeEffect::TeleportRandomly {
                    diameter: 8.0,
                    directional_particles: false,
                },
                ConsumeEffect::PlaySound(IdOr::Inline(SoundEvent {
                    location: id("minecraft:s"),
                    fixed_range: Some(2.0),
                })),
            ],
        }),
        "use_remainder" => D::UseRemainder(template()),
        "use_cooldown" => D::UseCooldown(UseCooldown {
            seconds: 1.0,
            cooldown_group: Some(id("minecraft:g")),
        }),
        "damage_resistant" => D::DamageResistant(tag()),
        "tool" => D::Tool(Tool {
            rules: vec![ToolRule {
                blocks: tag(),
                speed: Some(4.0),
                correct_for_drops: Some(true),
            }],
            default_mining_speed: 2.0,
            damage_per_block: 2,
            can_destroy_blocks_in_creative: false,
        }),
        "weapon" => D::Weapon(Weapon {
            item_damage_per_attack: 2,
            disable_blocking_for_seconds: 5.0,
        }),
        "attack_range" => D::AttackRange(AttackRange {
            min_reach: 1.0,
            max_reach: 4.0,
            min_creative_reach: 1.0,
            max_creative_reach: 6.0,
            hitbox_margin: 0.5,
            mob_factor: 2.0,
        }),
        "enchantable" => D::Enchantable(v),
        "equippable" => D::Equippable(Equippable {
            slot: EquipmentSlot::Chest,
            equip_sound: sound(),
            asset_id: Some(id("minecraft:elytra")),
            camera_overlay: Some(id("minecraft:o")),
            allowed_entities: Some(HolderSet::Ids(vec![1])),
            dispensable: false,
            swappable: false,
            damage_on_hurt: false,
            equip_on_interact: true,
            can_be_sheared: true,
            shearing_sound: sound(),
        }),
        "repairable" => D::Repairable(HolderSet::Ids(vec![1, 2])),
        "glider" => D::Glider(Unit),
        "tooltip_style" => D::TooltipStyle(id("minecraft:t")),
        "death_protection" => D::DeathProtection(DeathProtection {
            death_effects: vec![ConsumeEffect::ClearAllEffects],
        }),
        "blocks_attacks" => D::BlocksAttacks(BlocksAttacks {
            block_delay_seconds: 0.25,
            disable_cooldown_scale: 2.0,
            damage_reductions: vec![DamageReduction {
                horizontal_blocking_angle: 45.0,
                damage_types: Some(tag()),
                base: 1.0,
                factor: 0.5,
            }],
            item_damage: ItemDamageFunction {
                threshold: 2.0,
                base: 1.0,
                factor: 0.5,
            },
            bypassed_by: Some(tag()),
            block_sound: Some(sound()),
            disable_sound: Some(sound()),
        }),
        "piercing_weapon" => D::PiercingWeapon(PiercingWeapon {
            deals_knockback: false,
            dismounts: true,
            sound: Some(sound()),
            hit_sound: Some(sound()),
        }),
        "kinetic_weapon" => D::KineticWeapon(KineticWeapon {
            contact_cooldown_ticks: 5,
            delay_ticks: 2,
            dismount_conditions: Some(kinetic_condition()),
            knockback_conditions: Some(kinetic_condition()),
            damage_conditions: Some(kinetic_condition()),
            forward_movement: 1.0,
            damage_multiplier: 2.0,
            sound: Some(sound()),
            hit_sound: Some(sound()),
        }),
        "attack_animation" => D::AttackAnimation(SwingAnimation {
            kind: SwingAnimationType::Stab,
            duration: 9,
        }),
        "interact_animation" => D::InteractAnimation(SwingAnimation {
            kind: SwingAnimationType::None,
            duration: 6,
        }),
        "additional_trade_cost" => D::AdditionalTradeCost(v),
        "block_transformer" => D::BlockTransformer(v),
        "villager_food" => D::VillagerFood(v),
        "stored_enchantments" => D::StoredEnchantments(ItemEnchantments(vec![EnchantmentLevel {
            enchantment: 2,
            level: 1,
        }])),
        "dye" => D::Dye(DyeColor::Blue),
        "dyed_color" => D::DyedColor(0xff00ff),
        "map_id" => D::MapId(v),
        "map_decorations" => D::MapDecorations(nbt()),
        "map_post_processing" => D::MapPostProcessing(MapPostProcessing::Lock),
        "charged_projectiles" => D::ChargedProjectiles(LengthPrefixed(vec![template()])),
        "bundle_contents" => D::BundleContents(vec![template(), template()]),
        "potion_contents" => D::PotionContents(PotionContents {
            potion: Some(v),
            custom_color: Some(7),
            custom_effects: vec![effect()],
            custom_name: Some("n".into()),
        }),
        "potion_duration_scale" => D::PotionDurationScale(0.5),
        "suspicious_stew_effects" => D::SuspiciousStewEffects(vec![SuspiciousStewEffect {
            effect: 1,
            duration: 100,
        }]),
        "writable_book_content" => D::WritableBookContent(LengthPrefixed(vec![Filterable {
            raw: BoundedString("p".into()),
            filtered: None,
        }])),
        "written_book_content" => D::WrittenBookContent(WrittenBookContent {
            title: Filterable {
                raw: BoundedString("t".into()),
                filtered: None,
            },
            author: "a".into(),
            generation: 1,
            pages: vec![Filterable {
                raw: styled(),
                filtered: Some(text("f")),
            }],
            resolved: true,
        }),
        "trim" => D::Trim(ArmorTrim {
            material: IdOr::Id(1),
            pattern: IdOr::Inline(TrimPattern {
                asset_id: id("minecraft:a"),
                description: text("d"),
                decal: true,
            }),
        }),
        "debug_stick_state" => D::DebugStickState(nbt()),
        "entity_data" => D::EntityData(TypedEntityData {
            kind: 1,
            tag: compound(),
        }),
        "bucket_entity_data" => D::BucketEntityData(compound()),
        "block_entity_data" => D::BlockEntityData(TypedEntityData {
            kind: 1,
            tag: compound(),
        }),
        "instrument" => D::Instrument(IdOr::Inline(Instrument {
            sound_event: sound(),
            use_duration: 1.0,
            range: 2.0,
            durability_damage: 3,
            description: text("d"),
        })),
        "provides_trim_material" => D::ProvidesTrimMaterial(IdOr::Id(2)),
        "ominous_bottle_amplifier" => D::OminousBottleAmplifier(v),
        "jukebox_playable" => D::JukeboxPlayable(IdOr::Id(1)),
        "provides_banner_patterns" => D::ProvidesBannerPatterns(tag()),
        "recipes" => D::Recipes(NetworkNbt(Value::Int(1))),
        "lodestone_tracker" => D::LodestoneTracker(LodestoneTracker {
            target: Some(GlobalPos {
                dimension: id("minecraft:overworld"),
                pos: mcr_buf::BlockPos { x: 1, y: 2, z: 3 },
            }),
            tracked: false,
        }),
        "firework_explosion" => D::FireworkExplosion(explosion()),
        "fireworks" => D::Fireworks(Fireworks {
            flight_duration: 3,
            explosions: LengthPrefixed(vec![explosion()]),
        }),
        "profile" => D::Profile(profile()),
        "note_block_sound" => D::NoteBlockSound(id("minecraft:n")),
        "banner_patterns" => D::BannerPatterns(vec![BannerPatternLayer {
            pattern: IdOr::Id(1),
            color: DyeColor::Lime,
        }]),
        "base_color" => D::BaseColor(DyeColor::Pink),
        "pot_decorations" => D::PotDecorations(PotDecorations {
            back: Some(template()),
            left: None,
            right: Some(template()),
            front: None,
        }),
        "container" => D::Container(LengthPrefixed(vec![Some(template()), None])),
        "block_state" => D::BlockState(vec![BlockStateProperty {
            name: "a".into(),
            value: "b".into(),
        }]),
        "bees" => D::Bees(vec![BeeOccupant {
            entity_data: TypedEntityData {
                kind: 1,
                tag: compound(),
            },
            ticks_in_hive: 1,
            min_ticks_in_hive: 2,
        }]),
        "sulfur_cube_content" => D::SulfurCubeContent(template()),
        "lock" => D::Lock(nbt()),
        "container_loot" => D::ContainerLoot(nbt()),
        "break_sound" => D::BreakSound(sound()),
        "compostable" => D::Compostable(Compostable {
            layers: ResolvableInt::Constant(2),
        }),
        "cooking_fuel" => D::CookingFuel(CookingFuel {
            burn_time: ResolvableInt::Constant(200),
            speed_multiplier: ResolvableFloat::Reference(id("minecraft:s")),
        }),
        "brewing_fuel" => D::BrewingFuel(BrewingFuel {
            uses: ResolvableInt::Reference(id("minecraft:u")),
            speed_multiplier: ResolvableFloat::Constant(1.5),
        }),
        "mob_visibility" => D::MobVisibility(MobVisibility {
            targeting_entity_types: HolderSet::Ids(vec![1, 2]),
            visibility: 0.5,
        }),
        "villager/variant" => D::VillagerVariant(v),
        "wolf/variant" => D::WolfVariant(v),
        "wolf/sound_variant" => D::WolfSoundVariant(v),
        "wolf/collar" => D::WolfCollar(DyeColor::Red),
        "fox/variant" => D::FoxVariant(v),
        "salmon/size" => D::SalmonSize(v),
        "parrot/variant" => D::ParrotVariant(v),
        "tropical_fish/pattern" => D::TropicalFishPattern(VarInt(0x101)),
        "tropical_fish/base_color" => D::TropicalFishBaseColor(DyeColor::Cyan),
        "tropical_fish/pattern_color" => D::TropicalFishPatternColor(DyeColor::Gray),
        "mooshroom/variant" => D::MooshroomVariant(v),
        "rabbit/variant" => D::RabbitVariant(VarInt(99)),
        "pig/variant" => D::PigVariant(v),
        "pig/sound_variant" => D::PigSoundVariant(v),
        "cow/variant" => D::CowVariant(v),
        "cow/sound_variant" => D::CowSoundVariant(v),
        "chicken/variant" => D::ChickenVariant(v),
        "chicken/sound_variant" => D::ChickenSoundVariant(v),
        "zombie_nautilus/variant" => D::ZombieNautilusVariant(v),
        "frog/variant" => D::FrogVariant(v),
        "horse/variant" => D::HorseVariant(v),
        "painting/variant" => D::PaintingVariant(IdOr::Id(1)),
        "llama/variant" => D::LlamaVariant(v),
        "axolotl/variant" => D::AxolotlVariant(v),
        "cat/variant" => D::CatVariant(v),
        "cat/sound_variant" => D::CatSoundVariant(v),
        "cat/collar" => D::CatCollar(DyeColor::Black),
        "sheep/color" => D::SheepColor(DyeColor::Orange),
        "shulker/color" => D::ShulkerColor(DyeColor::Purple),
        "provides_pottery_pattern" => D::ProvidesPotteryPattern(v),
        "sign_text_front" => D::SignTextFront(sign()),
        "sign_text_back" => D::SignTextBack(sign()),
        "waxed" => D::Waxed(Unit),
        "cushion/color" => D::CushionColor(DyeColor::Green),
        other => panic!("no sample for {other}"),
    }
}

const NOT_PERSISTENT: &[&str] = &[
    "creative_slot_lock",
    "additional_trade_cost",
    "map_post_processing",
];

#[test]
fn every_persistent_component_hashes() {
    let regs = registries();
    let cx = HashCx::new(mcr_data::v26_3(), &regs);
    for type_id in 0..COMPONENT_COUNT as i32 {
        let c = sample(type_id);
        assert_eq!(c.type_id(), type_id);
        let name = COMPONENT_NAMES[type_id as usize];
        if NOT_PERSISTENT.contains(&name) {
            continue;
        }
        assert!(cx.component_hash(&c).is_some(), "{name} has no hash");
    }
}

#[test]
fn text_variants_hash() {
    let mut nbt = text("x");
    nbt.content = Content::Nbt(Box::new(NbtContent {
        path: "a.b".into(),
        interpret: true,
        plain: false,
        separator: Some(Box::new(text(","))),
        source: NbtSource::Storage(id("minecraft:s")),
    }));
    assert!(crate::hashing::component_hash(&nbt).is_some());
    let mut obj = text("x");
    obj.content = Content::Object {
        object: ObjectContent::Atlas {
            atlas: id("minecraft:blocks"),
            sprite: id("minecraft:stone"),
        },
        fallback: Some(Box::new(text("f"))),
    };
    assert!(crate::hashing::component_hash(&obj).is_some());
    let mut events = Component::translatable("k", vec![]);
    for click in [
        ClickEvent::OpenUrl("https://a.b".into()),
        ClickEvent::ChangePage(2),
        ClickEvent::Custom {
            id: id("minecraft:c"),
            payload: Some(Value::Int(1)),
        },
    ] {
        events.style.click_event = Some(Box::new(click));
        assert!(crate::hashing::component_hash(&events).is_some());
    }
    events.style.hover_event = Some(Box::new(HoverEvent::ShowEntity {
        entity: id("minecraft:pig"),
        uuid: uuid::Uuid::from_u128(9),
        name: Some(Box::new(text("p"))),
    }));
    assert!(crate::hashing::component_hash(&events).is_some());
}

#[test]
fn default_valued_fields_are_omitted() {
    let regs = registries();
    let cx = HashCx::new(mcr_data::v26_3(), &regs);
    let weapon = DataComponent::Weapon(Weapon {
        item_damage_per_attack: 1,
        disable_blocking_for_seconds: 0.0,
    });
    assert_eq!(
        cx.component_hash(&weapon),
        Some(crate::hashing::h_empty_map())
    );
}
