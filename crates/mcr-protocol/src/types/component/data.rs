use mcr_buf::{
    BoundedString, Decode, Encode, IdOr, Identifier, LengthPrefixed, Result, VarInt, read_varint,
    write_varint,
};
use mcr_nbt::NetworkCompound;
use uuid::Uuid;

use super::common::{
    AttributeOperation, DepthGuard, DyeColor, EquipmentSlot, EquipmentSlotGroup, Filterable,
    FireworkShape, GlobalPos, HolderSet, ItemUseAnimation, ResolvableFloat, ResolvableInt,
    SwingAnimationType, either_codec,
};
use crate::types::{
    Component, SoundEvent,
    item::ItemStackTemplate,
    profile::{GameProfile, ProfileProperty},
};

pub type SoundHolder = IdOr<SoundEvent>;

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct UseEffects {
    pub can_sprint: bool,
    pub interact_vibrations: bool,
    pub speed_multiplier: f32,
}

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct ItemLore(pub LengthPrefixed<Component, 256>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct EnchantmentLevel {
    #[mcr(varint)]
    pub enchantment: i32,
    #[mcr(varint)]
    pub level: i32,
}

/// Entries keep wire order so re-encoding is byte-identical.
#[derive(Debug, Clone, PartialEq, Eq, Default, Encode, Decode)]
pub struct ItemEnchantments(pub Vec<EnchantmentLevel>);

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct AttributeModifier {
    pub id: Identifier,
    pub amount: f64,
    pub operation: AttributeOperation,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub enum AttributeDisplay {
    Default,
    Hidden,
    Override(Component),
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct AttributeModifierEntry {
    #[mcr(varint)]
    pub attribute: i32,
    pub modifier: AttributeModifier,
    pub slot: EquipmentSlotGroup,
    pub display: AttributeDisplay,
}

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct ItemAttributeModifiers(pub Vec<AttributeModifierEntry>);

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct CustomModelData {
    pub floats: Vec<f32>,
    pub flags: Vec<bool>,
    pub strings: Vec<String>,
    pub colors: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct TooltipDisplay {
    pub hide_tooltip: bool,
    pub hidden_components: Vec<VarInt>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct FoodProperties {
    #[mcr(varint)]
    pub nutrition: i32,
    pub saturation: f32,
    pub can_always_eat: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobEffectDetails {
    pub amplifier: i32,
    pub duration: i32,
    pub ambient: bool,
    pub show_particles: bool,
    pub show_icon: bool,
    pub hidden_effect: Option<Box<MobEffectDetails>>,
}

impl Encode for MobEffectDetails {
    fn encode(&self, out: &mut Vec<u8>) {
        write_varint(out, self.amplifier);
        write_varint(out, self.duration);
        self.ambient.encode(out);
        self.show_particles.encode(out);
        self.show_icon.encode(out);
        self.hidden_effect.encode(out);
    }
}

impl Decode<'_> for MobEffectDetails {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let _depth = DepthGuard::enter()?;
        Ok(Self {
            amplifier: read_varint(input)?,
            duration: read_varint(input)?,
            ambient: bool::decode(input)?,
            show_particles: bool::decode(input)?,
            show_icon: bool::decode(input)?,
            hidden_effect: Option::decode(input)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MobEffectInstance {
    #[mcr(varint)]
    pub effect: i32,
    pub details: MobEffectDetails,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub enum ConsumeEffect {
    ApplyEffects {
        effects: Vec<MobEffectInstance>,
        probability: f32,
    },
    RemoveEffects(HolderSet),
    ClearAllEffects,
    TeleportRandomly {
        diameter: f32,
        directional_particles: bool,
    },
    PlaySound(SoundHolder),
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Consumable {
    pub consume_seconds: f32,
    pub animation: ItemUseAnimation,
    pub sound: SoundHolder,
    pub has_consume_particles: bool,
    pub on_consume_effects: Vec<ConsumeEffect>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct UseCooldown {
    pub seconds: f32,
    pub cooldown_group: Option<Identifier>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ToolRule {
    pub blocks: HolderSet,
    pub speed: Option<f32>,
    pub correct_for_drops: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Tool {
    pub rules: Vec<ToolRule>,
    pub default_mining_speed: f32,
    #[mcr(varint)]
    pub damage_per_block: i32,
    pub can_destroy_blocks_in_creative: bool,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Weapon {
    #[mcr(varint)]
    pub item_damage_per_attack: i32,
    pub disable_blocking_for_seconds: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct AttackRange {
    pub min_reach: f32,
    pub max_reach: f32,
    pub min_creative_reach: f32,
    pub max_creative_reach: f32,
    pub hitbox_margin: f32,
    pub mob_factor: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Equippable {
    pub slot: EquipmentSlot,
    pub equip_sound: SoundHolder,
    pub asset_id: Option<Identifier>,
    pub camera_overlay: Option<Identifier>,
    pub allowed_entities: Option<HolderSet>,
    pub dispensable: bool,
    pub swappable: bool,
    pub damage_on_hurt: bool,
    pub equip_on_interact: bool,
    pub can_be_sheared: bool,
    pub shearing_sound: SoundHolder,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DeathProtection {
    pub death_effects: Vec<ConsumeEffect>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct DamageReduction {
    pub horizontal_blocking_angle: f32,
    pub damage_types: Option<HolderSet>,
    pub base: f32,
    pub factor: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ItemDamageFunction {
    pub threshold: f32,
    pub base: f32,
    pub factor: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BlocksAttacks {
    pub block_delay_seconds: f32,
    pub disable_cooldown_scale: f32,
    pub damage_reductions: Vec<DamageReduction>,
    pub item_damage: ItemDamageFunction,
    pub bypassed_by: Option<HolderSet>,
    pub block_sound: Option<SoundHolder>,
    pub disable_sound: Option<SoundHolder>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PiercingWeapon {
    pub deals_knockback: bool,
    pub dismounts: bool,
    pub sound: Option<SoundHolder>,
    pub hit_sound: Option<SoundHolder>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct KineticCondition {
    #[mcr(varint)]
    pub max_duration_ticks: i32,
    pub min_speed: f32,
    pub min_relative_speed: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct KineticWeapon {
    #[mcr(varint)]
    pub contact_cooldown_ticks: i32,
    #[mcr(varint)]
    pub delay_ticks: i32,
    pub dismount_conditions: Option<KineticCondition>,
    pub knockback_conditions: Option<KineticCondition>,
    pub damage_conditions: Option<KineticCondition>,
    pub forward_movement: f32,
    pub damage_multiplier: f32,
    pub sound: Option<SoundHolder>,
    pub hit_sound: Option<SoundHolder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SwingAnimation {
    pub kind: SwingAnimationType,
    #[mcr(varint)]
    pub duration: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct PotionContents {
    pub potion: Option<VarInt>,
    pub custom_color: Option<i32>,
    pub custom_effects: Vec<MobEffectInstance>,
    pub custom_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SuspiciousStewEffect {
    #[mcr(varint)]
    pub effect: i32,
    #[mcr(varint)]
    pub duration: i32,
}

pub type WritableBookContent = LengthPrefixed<Filterable<BoundedString<1024>>, 100>;

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct WrittenBookContent {
    pub title: Filterable<BoundedString<32>>,
    pub author: String,
    #[mcr(varint)]
    pub generation: i32,
    pub pages: Vec<Filterable<Component>>,
    pub resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TrimMaterial {
    pub palette_id: Identifier,
    pub description: Component,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TrimPattern {
    pub asset_id: Identifier,
    pub description: Component,
    pub decal: bool,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ArmorTrim {
    pub material: IdOr<TrimMaterial>,
    pub pattern: IdOr<TrimPattern>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Instrument {
    pub sound_event: SoundHolder,
    pub use_duration: f32,
    pub range: f32,
    #[mcr(varint)]
    pub durability_damage: i32,
    pub description: Component,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct JukeboxSong {
    pub sound_event: SoundHolder,
    pub description: Component,
    pub length_in_seconds: f32,
    #[mcr(varint)]
    pub comparator_output: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BannerPattern {
    pub asset_id: Identifier,
    pub translation_key: String,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BannerPatternLayer {
    pub pattern: IdOr<BannerPattern>,
    pub color: DyeColor,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct TypedEntityData {
    #[mcr(varint)]
    pub kind: i32,
    pub tag: NetworkCompound,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct LodestoneTracker {
    pub target: Option<GlobalPos>,
    pub tracked: bool,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct FireworkExplosion {
    pub shape: FireworkShape,
    pub colors: Vec<i32>,
    pub fade_colors: Vec<i32>,
    pub has_trail: bool,
    pub has_twinkle: bool,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Fireworks {
    #[mcr(varint)]
    pub flight_duration: i32,
    pub explosions: LengthPrefixed<FireworkExplosion, 256>,
}

pub type ProfileProperties = LengthPrefixed<ProfileProperty, 16>;

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct PartialProfile {
    pub name: Option<BoundedString<16>>,
    pub id: Option<Uuid>,
    pub properties: ProfileProperties,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProfileSource {
    Full(GameProfile),
    Partial(PartialProfile),
}
either_codec!(ProfileSource, Full(GameProfile), Partial(PartialProfile));

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct SkinPatch {
    pub body: Option<Identifier>,
    pub cape: Option<Identifier>,
    pub elytra: Option<Identifier>,
    pub slim_model: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct ResolvableProfile {
    pub source: ProfileSource,
    pub skin: SkinPatch,
}

#[derive(Debug, Clone, PartialEq, Default, Encode, Decode)]
pub struct PotDecorations {
    pub back: Option<ItemStackTemplate>,
    pub left: Option<ItemStackTemplate>,
    pub right: Option<ItemStackTemplate>,
    pub front: Option<ItemStackTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct BlockStateProperty {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BeeOccupant {
    pub entity_data: TypedEntityData,
    #[mcr(varint)]
    pub ticks_in_hive: i32,
    #[mcr(varint)]
    pub min_ticks_in_hive: i32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct Compostable {
    pub layers: ResolvableInt,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct CookingFuel {
    pub burn_time: ResolvableInt,
    pub speed_multiplier: ResolvableFloat,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct BrewingFuel {
    pub uses: ResolvableInt,
    pub speed_multiplier: ResolvableFloat,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct MobVisibility {
    pub targeting_entity_types: HolderSet,
    pub visibility: f32,
}

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
pub struct SignText {
    pub messages: [Component; 4],
    pub filtered_messages: Option<[Component; 4]>,
    pub color: DyeColor,
    pub has_glowing_text: bool,
}
