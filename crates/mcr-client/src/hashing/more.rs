//! Encoders for the persistent component codecs that need registry or nested-codec knowledge.

use mcr_buf::IdOr;
use mcr_protocol::types::component::{
    AdventureModePredicate, AttackRange, BlockPredicate, BlocksAttacks, Consumable, ConsumeEffect,
    DataComponent, DeathProtection, Equippable, GlobalPos, HolderSet, Instrument, ItemUseAnimation,
    KineticCondition, KineticWeapon, MobEffectDetails, MobEffectInstance, PiercingWeapon,
    PredicateType, ProfileSource, ResolvableFloat, ResolvableInt, ResolvableProfile, SignText,
    SoundHolder, SwingAnimation, SwingAnimationType, Tool, TypedEntityData, UseEffects,
    ValueMatcher, Weapon,
};
use mcr_protocol::types::item::ItemStackTemplate;
use uuid::Uuid;

use super::{
    COMPONENT_NAMES, HashCx, MapBuilder, component_hash, dye_name, equipment_slot_name, h_bool,
    h_byte, h_float, h_int, h_list, h_nbt, h_str, int_array,
};

const SOUND: &str = "minecraft:sound_event";
const DAMAGE_TYPE: &str = "minecraft:damage_type";
const DEFAULT_EQUIP_SOUND: &str = "minecraft:item.armor.equip_generic";
const DEFAULT_SHEAR_SOUND: &str = "minecraft:item.shears.snip";
const DEFAULT_EAT_SOUND: &str = "minecraft:entity.generic.eat";

impl HashCx<'_> {
    pub(super) fn component_hash_more(&self, c: &DataComponent) -> Option<u32> {
        use DataComponent as D;
        Some(match c {
            D::UseEffects(u) => use_effects(u),
            D::CanPlaceOn(p) | D::CanBreak(p) => self.adventure(p)?,
            D::Consumable(c) => self.consumable(c)?,
            D::DamageResistant(set) => {
                let mut map = MapBuilder::default();
                map.put("types", self.holder_set(set, DAMAGE_TYPE)?);
                map.build()
            }
            D::Tool(t) => self.tool(t)?,
            D::Weapon(w) => weapon(w),
            D::AttackRange(a) => attack_range(a),
            D::Equippable(e) => self.equippable(e)?,
            D::Repairable(set) => {
                let mut map = MapBuilder::default();
                map.put("items", self.holder_set(set, "minecraft:item")?);
                map.build()
            }
            D::DeathProtection(d) => self.death_protection(d)?,
            D::BlocksAttacks(b) => self.blocks_attacks(b)?,
            D::PiercingWeapon(p) => self.piercing_weapon(p)?,
            D::KineticWeapon(k) => self.kinetic_weapon(k)?,
            D::AttackAnimation(a) | D::InteractAnimation(a) => swing_animation(a),
            D::BlockTransformer(v) => self.registry_ref("minecraft:block_transformer", v.0)?,
            D::VillagerFood(v) => {
                let mut map = MapBuilder::default();
                map.put("nutrition", h_int(v.0));
                map.build()
            }
            D::MapDecorations(v)
            | D::DebugStickState(v)
            | D::Recipes(v)
            | D::Lock(v)
            | D::ContainerLoot(v) => h_nbt(&v.0),
            D::Trim(t) => {
                let mut map = MapBuilder::default();
                map.put(
                    "material",
                    self.id_or("minecraft:trim_material", &t.material, |m| {
                        let mut d = MapBuilder::default();
                        d.put("palette_id", h_str(m.palette_id.as_str()));
                        d.put("description", component_hash(&m.description)?);
                        Some(d.build())
                    })?,
                );
                map.put(
                    "pattern",
                    self.id_or("minecraft:trim_pattern", &t.pattern, |p| {
                        let mut d = MapBuilder::default();
                        d.put("asset_id", h_str(p.asset_id.as_str()));
                        d.put("description", component_hash(&p.description)?);
                        d.put("decal", h_bool(p.decal));
                        Some(d.build())
                    })?,
                );
                map.build()
            }
            D::EntityData(t) => self.typed_entity(t, "minecraft:entity_type")?,
            D::BlockEntityData(t) => self.typed_entity(t, "minecraft:block_entity_type")?,
            D::Instrument(i) => self.id_or("minecraft:instrument", i, |i| self.instrument(i))?,
            D::ProvidesTrimMaterial(m) => self.id_or("minecraft:trim_material", m, |m| {
                let mut d = MapBuilder::default();
                d.put("palette_id", h_str(m.palette_id.as_str()));
                d.put("description", component_hash(&m.description)?);
                Some(d.build())
            })?,
            D::JukeboxPlayable(IdOr::Id(id)) => self.registry_ref("minecraft:jukebox_song", *id)?,
            D::ProvidesBannerPatterns(set) => self.holder_set(set, "minecraft:banner_pattern")?,
            D::LodestoneTracker(l) => {
                let mut map = MapBuilder::default();
                if let Some(target) = &l.target {
                    map.put("target", global_pos(target));
                }
                map.put_if(!l.tracked, "tracked", || h_bool(false));
                map.build()
            }
            D::Profile(p) => self.profile(p)?,
            D::PotDecorations(p) if self.dialect.pot_decorations_as_items => {
                let brick = self.data.item_by_name("minecraft:brick")? as i32;
                let mut out = Vec::new();
                for side in [&p.back, &p.left, &p.right, &p.front] {
                    let item = side.as_ref().map_or(brick, |t| t.item);
                    out.push(self.registry_ref("minecraft:item", item)?);
                }
                h_list(out)
            }
            D::PotDecorations(p) => {
                let mut map = MapBuilder::default();
                for (key, slot) in [
                    ("back", &p.back),
                    ("left", &p.left),
                    ("right", &p.right),
                    ("front", &p.front),
                ] {
                    if let Some(t) = slot {
                        map.put(key, self.template_of(t)?);
                    }
                }
                map.build()
            }
            D::Bees(bees) => {
                let mut out = Vec::new();
                for b in bees {
                    let mut map = MapBuilder::default();
                    map.put(
                        "entity_data",
                        self.typed_entity(&b.entity_data, "minecraft:entity_type")?,
                    );
                    map.put("ticks_in_hive", h_int(b.ticks_in_hive));
                    map.put("min_ticks_in_hive", h_int(b.min_ticks_in_hive));
                    out.push(map.build());
                }
                h_list(out)
            }
            D::SulfurCubeContent(t) => self.template_of(t)?,
            D::BreakSound(s) => self.sound(s)?,
            D::Compostable(c) => {
                let mut map = MapBuilder::default();
                map.put("layers", resolvable_int(&c.layers));
                map.build()
            }
            D::CookingFuel(c) => {
                let mut map = MapBuilder::default();
                map.put("burn_time", resolvable_int(&c.burn_time));
                map.put("speed_multiplier", resolvable_float(&c.speed_multiplier));
                map.build()
            }
            D::BrewingFuel(c) => {
                let mut map = MapBuilder::default();
                map.put("uses", resolvable_int(&c.uses));
                map.put("speed_multiplier", resolvable_float(&c.speed_multiplier));
                map.build()
            }
            D::MobVisibility(m) => {
                let mut map = MapBuilder::default();
                map.put(
                    "targeting_entity_types",
                    self.holder_set(&m.targeting_entity_types, "minecraft:entity_type")?,
                );
                map.put("visibility", h_float(m.visibility));
                map.build()
            }
            D::FoxVariant(v) => enum_name(v.0, &["red", "snow"])?,
            D::SalmonSize(v) => enum_name(v.0, &["small", "medium", "large"])?,
            D::ParrotVariant(v) => {
                enum_name(v.0, &["red_blue", "blue", "green", "yellow_blue", "gray"])?
            }
            D::TropicalFishPattern(v) => tropical_pattern(v.0)?,
            D::MooshroomVariant(v) => enum_name(v.0, &["red", "brown"])?,
            D::RabbitVariant(v) => rabbit_variant(v.0)?,
            D::HorseVariant(v) => enum_name(
                v.0,
                &[
                    "white",
                    "creamy",
                    "chestnut",
                    "brown",
                    "black",
                    "gray",
                    "dark_brown",
                ],
            )?,
            D::LlamaVariant(v) => enum_name(v.0, &["creamy", "white", "brown", "gray"])?,
            D::AxolotlVariant(v) => enum_name(v.0, &["lucy", "wild", "gold", "cyan", "blue"])?,
            D::PaintingVariant(p) => self.id_or("minecraft:painting_variant", p, |p| {
                let mut map = MapBuilder::default();
                map.put("width", h_int(p.width));
                map.put("height", h_int(p.height));
                map.put("asset_id", h_str(p.asset_id.as_str()));
                if let Some(t) = &p.title {
                    map.put("title", component_hash(t)?);
                }
                if let Some(a) = &p.author {
                    map.put("author", component_hash(a)?);
                }
                Some(map.build())
            })?,
            D::ProvidesPotteryPattern(v) => {
                self.registry_ref("minecraft:decorated_pot_pattern", v.0)?
            }
            D::SignTextFront(s) | D::SignTextBack(s) => sign_text(s)?,
            _ => return None,
        })
    }

    pub(super) fn sound(&self, s: &SoundHolder) -> Option<u32> {
        match s {
            IdOr::Id(id) => self.registry_ref(SOUND, *id),
            IdOr::Inline(e) => {
                let mut map = MapBuilder::default();
                map.put("sound_id", h_str(e.location.as_str()));
                if let Some(r) = e.fixed_range {
                    map.put("range", h_float(r));
                }
                Some(map.build())
            }
        }
    }

    fn put_sound_unless_default(
        &self,
        map: &mut MapBuilder,
        key: &str,
        s: &SoundHolder,
        default: &str,
    ) -> Option<()> {
        let is_default =
            matches!(s, IdOr::Id(id) if self.registry_name(SOUND, *id).as_deref() == Some(default));
        if !is_default {
            map.put(key, self.sound(s)?);
        }
        Some(())
    }

    pub(super) fn holder_set(&self, set: &HolderSet, registry: &str) -> Option<u32> {
        match set {
            HolderSet::Tag(tag) => Some(h_str(&format!("#{}", tag.as_str()))),
            HolderSet::Ids(ids) => {
                let mut out = Vec::new();
                for &id in ids {
                    out.push(self.registry_ref(registry, id)?);
                }
                Some(if out.len() == 1 { out[0] } else { h_list(out) })
            }
        }
    }

    pub(super) fn id_or<T>(
        &self,
        registry: &str,
        value: &IdOr<T>,
        direct: impl FnOnce(&T) -> Option<u32>,
    ) -> Option<u32> {
        match value {
            IdOr::Id(id) => self.registry_ref(registry, *id),
            IdOr::Inline(v) => direct(v),
        }
    }

    pub(super) fn template_of(&self, t: &ItemStackTemplate) -> Option<u32> {
        self.template(t.item, t.count, &t.components)
    }

    fn instrument(&self, i: &Instrument) -> Option<u32> {
        let mut map = MapBuilder::default();
        map.put("sound_event", self.sound(&i.sound_event)?);
        map.put("use_duration", h_float(i.use_duration));
        map.put("range", h_float(i.range));
        map.put_if(i.durability_damage != 0, "durability_damage", || {
            h_int(i.durability_damage)
        });
        map.put("description", component_hash(&i.description)?);
        Some(map.build())
    }

    fn typed_entity(&self, t: &TypedEntityData, registry: &str) -> Option<u32> {
        let mut map = MapBuilder::default();
        for (k, v) in t.tag.0.iter() {
            map.put(k, h_nbt(v));
        }
        map.put("id", self.registry_ref(registry, t.kind)?);
        Some(map.build())
    }

    pub(super) fn mob_effect(&self, e: &MobEffectInstance) -> Option<u32> {
        let mut map = MapBuilder::default();
        map.put("id", self.registry_ref("minecraft:mob_effect", e.effect)?);
        effect_details(&mut map, &e.details);
        Some(map.build())
    }

    fn consume_effect(&self, e: &ConsumeEffect) -> Option<u32> {
        let mut map = MapBuilder::default();
        match e {
            ConsumeEffect::ApplyEffects {
                effects,
                probability,
            } => {
                map.put("type", h_str("minecraft:apply_effects"));
                let mut out = Vec::new();
                for e in effects {
                    out.push(self.mob_effect(e)?);
                }
                map.put("effects", h_list(out));
                map.put_if(*probability != 1.0, "probability", || h_float(*probability));
            }
            ConsumeEffect::RemoveEffects(set) => {
                map.put("type", h_str("minecraft:remove_effects"));
                map.put("effects", self.holder_set(set, "minecraft:mob_effect")?);
            }
            ConsumeEffect::ClearAllEffects => {
                map.put("type", h_str("minecraft:clear_all_effects"));
            }
            ConsumeEffect::TeleportRandomly {
                diameter,
                directional_particles,
            } => {
                map.put("type", h_str("minecraft:teleport_randomly"));
                map.put_if(*diameter != 16.0, "diameter", || h_float(*diameter));
                map.put_if(!directional_particles, "directional_particles", || {
                    h_bool(false)
                });
            }
            ConsumeEffect::PlaySound(s) => {
                map.put("type", h_str("minecraft:play_sound"));
                map.put("sound", self.sound(s)?);
            }
        }
        Some(map.build())
    }

    fn consume_effects(&self, effects: &[ConsumeEffect]) -> Option<Vec<u32>> {
        effects.iter().map(|e| self.consume_effect(e)).collect()
    }

    fn consumable(&self, c: &Consumable) -> Option<u32> {
        let mut map = MapBuilder::default();
        map.put_if(c.consume_seconds != 1.6, "consume_seconds", || {
            h_float(c.consume_seconds)
        });
        map.put_if(c.animation != ItemUseAnimation::Eat, "animation", || {
            h_str(use_animation_name(c.animation))
        });
        self.put_sound_unless_default(&mut map, "sound", &c.sound, DEFAULT_EAT_SOUND)?;
        map.put_if(!c.has_consume_particles, "has_consume_particles", || {
            h_bool(false)
        });
        if !c.on_consume_effects.is_empty() {
            map.put(
                "on_consume_effects",
                h_list(self.consume_effects(&c.on_consume_effects)?),
            );
        }
        Some(map.build())
    }

    fn death_protection(&self, d: &DeathProtection) -> Option<u32> {
        let mut map = MapBuilder::default();
        if !d.death_effects.is_empty() {
            map.put(
                "death_effects",
                h_list(self.consume_effects(&d.death_effects)?),
            );
        }
        Some(map.build())
    }

    fn tool(&self, t: &Tool) -> Option<u32> {
        let mut rules = Vec::new();
        for r in &t.rules {
            let mut map = MapBuilder::default();
            map.put("blocks", self.holder_set(&r.blocks, "minecraft:block")?);
            if let Some(s) = r.speed {
                map.put("speed", h_float(s));
            }
            if let Some(d) = r.correct_for_drops {
                map.put("correct_for_drops", h_bool(d));
            }
            rules.push(map.build());
        }
        let mut map = MapBuilder::default();
        map.put("rules", h_list(rules));
        map.put_if(
            t.default_mining_speed != 1.0,
            "default_mining_speed",
            || h_float(t.default_mining_speed),
        );
        map.put_if(t.damage_per_block != 1, "damage_per_block", || {
            h_int(t.damage_per_block)
        });
        map.put_if(
            !t.can_destroy_blocks_in_creative,
            "can_destroy_blocks_in_creative",
            || h_bool(false),
        );
        Some(map.build())
    }

    fn equippable(&self, e: &Equippable) -> Option<u32> {
        let mut map = MapBuilder::default();
        map.put("slot", h_str(equipment_slot_name(e.slot)));
        self.put_sound_unless_default(
            &mut map,
            "equip_sound",
            &e.equip_sound,
            DEFAULT_EQUIP_SOUND,
        )?;
        if let Some(a) = &e.asset_id {
            map.put("asset_id", h_str(a.as_str()));
        }
        if let Some(o) = &e.camera_overlay {
            map.put("camera_overlay", h_str(o.as_str()));
        }
        if let Some(set) = &e.allowed_entities {
            map.put(
                "allowed_entities",
                self.holder_set(set, "minecraft:entity_type")?,
            );
        }
        map.put_if(!e.dispensable, "dispensable", || h_bool(false));
        map.put_if(!e.swappable, "swappable", || h_bool(false));
        map.put_if(!e.damage_on_hurt, "damage_on_hurt", || h_bool(false));
        map.put_if(e.equip_on_interact, "equip_on_interact", || h_bool(true));
        map.put_if(e.can_be_sheared, "can_be_sheared", || h_bool(true));
        self.put_sound_unless_default(
            &mut map,
            "shearing_sound",
            &e.shearing_sound,
            DEFAULT_SHEAR_SOUND,
        )?;
        Some(map.build())
    }

    fn blocks_attacks(&self, b: &BlocksAttacks) -> Option<u32> {
        let default_reduction = b.damage_reductions.len() == 1 && {
            let r = &b.damage_reductions[0];
            r.horizontal_blocking_angle == 90.0
                && r.damage_types.is_none()
                && r.base == 0.0
                && r.factor == 1.0
        };
        let mut map = MapBuilder::default();
        map.put_if(b.block_delay_seconds != 0.0, "block_delay_seconds", || {
            h_float(b.block_delay_seconds)
        });
        map.put_if(
            b.disable_cooldown_scale != 1.0,
            "disable_cooldown_scale",
            || h_float(b.disable_cooldown_scale),
        );
        if !default_reduction {
            let mut out = Vec::new();
            for r in &b.damage_reductions {
                let mut d = MapBuilder::default();
                d.put_if(
                    r.horizontal_blocking_angle != 90.0,
                    "horizontal_blocking_angle",
                    || h_float(r.horizontal_blocking_angle),
                );
                if let Some(set) = &r.damage_types {
                    d.put("type", self.holder_set(set, DAMAGE_TYPE)?);
                }
                d.put("base", h_float(r.base));
                d.put("factor", h_float(r.factor));
                out.push(d.build());
            }
            map.put("damage_reductions", h_list(out));
        }
        let f = &b.item_damage;
        if (f.threshold, f.base, f.factor) != (1.0, 0.0, 1.0) {
            let mut d = MapBuilder::default();
            d.put("threshold", h_float(f.threshold));
            d.put("base", h_float(f.base));
            d.put("factor", h_float(f.factor));
            map.put("item_damage", d.build());
        }
        if let Some(set) = &b.bypassed_by {
            map.put("bypassed_by", self.holder_set(set, DAMAGE_TYPE)?);
        }
        if let Some(s) = &b.block_sound {
            map.put("block_sound", self.sound(s)?);
        }
        if let Some(s) = &b.disable_sound {
            map.put("disabled_sound", self.sound(s)?);
        }
        Some(map.build())
    }

    fn piercing_weapon(&self, p: &PiercingWeapon) -> Option<u32> {
        let mut map = MapBuilder::default();
        map.put_if(!p.deals_knockback, "deals_knockback", || h_bool(false));
        map.put_if(p.dismounts, "dismounts", || h_bool(true));
        if let Some(s) = &p.sound {
            map.put("sound", self.sound(s)?);
        }
        if let Some(s) = &p.hit_sound {
            map.put("hit_sound", self.sound(s)?);
        }
        Some(map.build())
    }

    fn kinetic_weapon(&self, k: &KineticWeapon) -> Option<u32> {
        let mut map = MapBuilder::default();
        map.put_if(
            k.contact_cooldown_ticks != 10,
            "contact_cooldown_ticks",
            || h_int(k.contact_cooldown_ticks),
        );
        map.put_if(k.delay_ticks != 0, "delay_ticks", || h_int(k.delay_ticks));
        for (key, cond) in [
            ("dismount_conditions", &k.dismount_conditions),
            ("knockback_conditions", &k.knockback_conditions),
            ("damage_conditions", &k.damage_conditions),
        ] {
            if let Some(c) = cond {
                map.put(key, kinetic_condition(c));
            }
        }
        map.put_if(k.forward_movement != 0.0, "forward_movement", || {
            h_float(k.forward_movement)
        });
        map.put_if(k.damage_multiplier != 1.0, "damage_multiplier", || {
            h_float(k.damage_multiplier)
        });
        if let Some(s) = &k.sound {
            map.put("sound", self.sound(s)?);
        }
        if let Some(s) = &k.hit_sound {
            map.put("hit_sound", self.sound(s)?);
        }
        Some(map.build())
    }

    fn profile(&self, p: &ResolvableProfile) -> Option<u32> {
        let mut map = MapBuilder::default();
        let properties = |props: &[mcr_protocol::types::profile::ProfileProperty]| {
            h_list(props.iter().map(|p| {
                let mut m = MapBuilder::default();
                m.put("name", h_str(&p.name));
                m.put("value", h_str(&p.value));
                if let Some(s) = &p.signature {
                    m.put("signature", h_str(s));
                }
                m.build()
            }))
        };
        match &p.source {
            ProfileSource::Full(g) => {
                map.put("id", uuid_hash(&g.id));
                map.put("name", h_str(&g.name));
                map.put_if(!g.properties.is_empty(), "properties", || {
                    properties(&g.properties)
                });
            }
            ProfileSource::Partial(g) => {
                if let Some(n) = &g.name {
                    map.put("name", h_str(&n.0));
                }
                if let Some(id) = &g.id {
                    map.put("id", uuid_hash(id));
                }
                map.put_if(!g.properties.0.is_empty(), "properties", || {
                    properties(&g.properties.0)
                });
            }
        }
        if let Some(t) = &p.skin.body {
            map.put("texture", h_str(t.as_str()));
        }
        if let Some(t) = &p.skin.cape {
            map.put("cape", h_str(t.as_str()));
        }
        if let Some(t) = &p.skin.elytra {
            map.put("elytra", h_str(t.as_str()));
        }
        if let Some(slim) = p.skin.slim_model {
            map.put("model", h_str(if slim { "slim" } else { "wide" }));
        }
        Some(map.build())
    }

    fn adventure(&self, p: &AdventureModePredicate) -> Option<u32> {
        let mut out = Vec::new();
        for b in &p.0 {
            out.push(self.block_predicate(b)?);
        }
        Some(if out.len() == 1 { out[0] } else { h_list(out) })
    }

    fn block_predicate(&self, b: &BlockPredicate) -> Option<u32> {
        let mut map = MapBuilder::default();
        if let Some(set) = &b.blocks {
            map.put("blocks", self.holder_set(set, "minecraft:block")?);
        }
        if let Some(props) = &b.properties {
            let mut m = MapBuilder::default();
            for p in &props.0 {
                m.put(
                    &p.name,
                    match &p.matcher {
                        ValueMatcher::Exact(s) => h_str(s),
                        ValueMatcher::Ranged(r) => {
                            let mut range = MapBuilder::default();
                            if let Some(min) = &r.min {
                                range.put("min", h_str(min));
                            }
                            if let Some(max) = &r.max {
                                range.put("max", h_str(max));
                            }
                            range.build()
                        }
                    },
                );
            }
            map.put("state", m.build());
        }
        if let Some(tag) = &b.nbt {
            map.put("nbt", h_str(&super::snbt::compound(&tag.0)));
        }
        if !b.components.exact.is_empty() {
            let mut m = MapBuilder::default();
            for c in &b.components.exact {
                let name = COMPONENT_NAMES.get(usize::try_from(c.type_id()).ok()?)?;
                m.put(&format!("minecraft:{name}"), self.component_hash(c)?);
            }
            map.put("components", m.build());
        }
        if !b.components.partial.0.is_empty() {
            let mut m = MapBuilder::default();
            for p in &b.components.partial.0 {
                let key = match p.kind {
                    PredicateType::Predicate(id) => self
                        .data
                        .registry("minecraft:data_component_predicate_type")?
                        .name_of(u32::try_from(id).ok()?)?
                        .to_owned(),
                    PredicateType::AnyValue(id) => {
                        format!(
                            "minecraft:{}",
                            COMPONENT_NAMES.get(usize::try_from(id).ok()?)?
                        )
                    }
                };
                m.put(&key, h_nbt(&p.value.0));
            }
            map.put("predicates", m.build());
        }
        Some(map.build())
    }
}

fn use_effects(u: &UseEffects) -> u32 {
    let mut map = MapBuilder::default();
    map.put_if(u.can_sprint, "can_sprint", || h_bool(true));
    map.put_if(!u.interact_vibrations, "interact_vibrations", || {
        h_bool(false)
    });
    map.put_if(u.speed_multiplier != 0.2, "speed_multiplier", || {
        h_float(u.speed_multiplier)
    });
    map.build()
}

fn weapon(w: &Weapon) -> u32 {
    let mut map = MapBuilder::default();
    map.put_if(
        w.item_damage_per_attack != 1,
        "item_damage_per_attack",
        || h_int(w.item_damage_per_attack),
    );
    map.put_if(
        w.disable_blocking_for_seconds != 0.0,
        "disable_blocking_for_seconds",
        || h_float(w.disable_blocking_for_seconds),
    );
    map.build()
}

fn attack_range(a: &AttackRange) -> u32 {
    let mut map = MapBuilder::default();
    for (key, value, default) in [
        ("min_reach", a.min_reach, 0.0),
        ("max_reach", a.max_reach, 3.0),
        ("min_creative_reach", a.min_creative_reach, 0.0),
        ("max_creative_reach", a.max_creative_reach, 5.0),
        ("hitbox_margin", a.hitbox_margin, 0.3),
        ("mob_factor", a.mob_factor, 1.0),
    ] {
        map.put_if(value != default, key, || h_float(value));
    }
    map.build()
}

fn swing_animation(a: &SwingAnimation) -> u32 {
    let mut map = MapBuilder::default();
    map.put_if(a.kind != SwingAnimationType::Whack, "type", || {
        h_str(match a.kind {
            SwingAnimationType::None => "none",
            SwingAnimationType::Whack => "whack",
            SwingAnimationType::Stab => "stab",
        })
    });
    map.put_if(a.duration != 6, "duration", || h_int(a.duration));
    map.build()
}

fn kinetic_condition(c: &KineticCondition) -> u32 {
    let mut map = MapBuilder::default();
    map.put("max_duration_ticks", h_int(c.max_duration_ticks));
    map.put_if(c.min_speed != 0.0, "min_speed", || h_float(c.min_speed));
    map.put_if(c.min_relative_speed != 0.0, "min_relative_speed", || {
        h_float(c.min_relative_speed)
    });
    map.build()
}

fn effect_details(map: &mut MapBuilder, d: &MobEffectDetails) {
    // The amplifier codec is an unsigned byte, which hashes as a byte tag.
    map.put_if(d.amplifier != 0, "amplifier", || h_byte(d.amplifier as i8));
    map.put_if(d.duration != 0, "duration", || h_int(d.duration));
    map.put_if(d.ambient, "ambient", || h_bool(true));
    map.put_if(!d.show_particles, "show_particles", || h_bool(false));
    map.put("show_icon", h_bool(d.show_icon));
    if let Some(hidden) = &d.hidden_effect {
        let mut inner = MapBuilder::default();
        effect_details(&mut inner, hidden);
        map.put("hidden_effect", inner.build());
    }
}

fn use_animation_name(a: ItemUseAnimation) -> &'static str {
    match a {
        ItemUseAnimation::None => "none",
        ItemUseAnimation::Eat => "eat",
        ItemUseAnimation::Drink => "drink",
        ItemUseAnimation::Block => "block",
        ItemUseAnimation::Bow => "bow",
        ItemUseAnimation::Trident => "trident",
        ItemUseAnimation::Crossbow => "crossbow",
        ItemUseAnimation::Spyglass => "spyglass",
        ItemUseAnimation::TootHorn => "toot_horn",
        ItemUseAnimation::Brush => "brush",
        ItemUseAnimation::Bundle => "bundle",
        ItemUseAnimation::Spear => "spear",
    }
}

fn global_pos(p: &GlobalPos) -> u32 {
    let mut map = MapBuilder::default();
    map.put("dimension", h_str(p.dimension.as_str()));
    map.put("pos", int_array(&[p.pos.x, p.pos.y, p.pos.z]));
    map.build()
}

fn uuid_hash(id: &Uuid) -> u32 {
    let v = id.as_u128();
    int_array(&[
        (v >> 96) as i32,
        (v >> 64) as i32,
        (v >> 32) as i32,
        v as i32,
    ])
}

fn resolvable_int(v: &ResolvableInt) -> u32 {
    match v {
        ResolvableInt::Constant(i) => h_int(*i),
        ResolvableInt::Reference(id) => h_str(id.as_str()),
    }
}

fn resolvable_float(v: &ResolvableFloat) -> u32 {
    match v {
        ResolvableFloat::Constant(f) => h_float(*f),
        ResolvableFloat::Reference(id) => h_str(id.as_str()),
    }
}

fn enum_name(id: i32, names: &[&str]) -> Option<u32> {
    names.get(usize::try_from(id).ok()?).map(|n| h_str(n))
}

fn tropical_pattern(packed: i32) -> Option<u32> {
    const SMALL: [&str; 6] = ["kob", "sunstreak", "snooper", "dasher", "brinely", "spotty"];
    const LARGE: [&str; 6] = [
        "flopper",
        "stripey",
        "glitter",
        "blockfish",
        "betty",
        "clayfish",
    ];
    let names = match packed & 0xff {
        0 => &SMALL,
        1 => &LARGE,
        _ => return None,
    };
    names
        .get(usize::try_from(packed >> 8).ok()?)
        .map(|n| h_str(n))
}

fn rabbit_variant(id: i32) -> Option<u32> {
    let name = match id {
        0 => "brown",
        1 => "white",
        2 => "black",
        3 => "white_splotched",
        4 => "gold",
        5 => "salt",
        99 => "evil",
        _ => return None,
    };
    Some(h_str(name))
}

fn sign_text(s: &SignText) -> Option<u32> {
    let lines = |m: &[mcr_protocol::types::Component; 4]| -> Option<u32> {
        let mut out = Vec::new();
        for c in m {
            out.push(component_hash(c)?);
        }
        Some(h_list(out))
    };
    let mut map = MapBuilder::default();
    map.put("messages", lines(&s.messages)?);
    if let Some(f) = &s.filtered_messages
        && f != &s.messages
    {
        map.put("filtered_messages", lines(f)?);
    }
    map.put("color", h_str(dye_name(s.color)));
    map.put("has_glowing_text", h_bool(s.has_glowing_text));
    Some(map.build())
}
