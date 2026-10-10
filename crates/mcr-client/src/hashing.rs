//! Vanilla `HashOps` (CRC32C) over item component values, as sent in `container_click` hashed stacks.

use std::cell::Cell;

use mcr_buf::{LengthPrefixed, VarInt};
use mcr_data::Version;
use mcr_nbt::{Compound, List, Value};
use mcr_protocol::types::component::{
    AttributeDisplay, AttributeOperation, COMPONENT_NAMES, DataComponent, DataComponentPatch,
    DyeColor, EquipmentSlot, EquipmentSlotGroup, FireworkShape, Rarity,
};
use mcr_protocol::types::item::{HashedComponent, HashedItem, HashedPatchMap, HashedStack};
use mcr_protocol::types::text::{ClickEvent, Color, Content, HoverEvent, NbtSource, ObjectContent};
use mcr_protocol::types::{Component, ItemStack};

use crate::registry::Registries;

mod more;
mod snbt;

const MAX_NBT_DEPTH: usize = 512;
const NBT_DEPTH_LIMIT_HASH: u32 = 0;

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                (c >> 1) ^ 0x82F6_3B78
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
}

static TABLE: [u32; 256] = crc_table();

pub fn crc32c(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c = TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8);
    }
    !c
}

fn tagged(tag: u8, payload: &[u8]) -> u32 {
    let mut buf = Vec::with_capacity(payload.len() + 1);
    buf.push(tag);
    buf.extend_from_slice(payload);
    crc32c(&buf)
}

pub fn h_byte(v: i8) -> u32 {
    tagged(6, &v.to_le_bytes())
}
pub fn h_short(v: i16) -> u32 {
    tagged(7, &v.to_le_bytes())
}
pub fn h_int(v: i32) -> u32 {
    tagged(8, &v.to_le_bytes())
}
pub fn h_long(v: i64) -> u32 {
    tagged(9, &v.to_le_bytes())
}
pub fn h_float(v: f32) -> u32 {
    tagged(10, &v.to_bits().to_le_bytes())
}
pub fn h_double(v: f64) -> u32 {
    tagged(11, &v.to_bits().to_le_bytes())
}
pub fn h_bool(v: bool) -> u32 {
    crc32c(&[13, u8::from(v)])
}
pub fn h_empty() -> u32 {
    crc32c(&[1])
}
pub fn h_empty_map() -> u32 {
    crc32c(&[2, 3])
}

pub fn h_str(s: &str) -> u32 {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut buf = Vec::with_capacity(5 + units.len() * 2);
    buf.push(12);
    buf.extend_from_slice(&(units.len() as i32).to_le_bytes());
    for u in units {
        buf.extend_from_slice(&u.to_le_bytes());
    }
    crc32c(&buf)
}

pub fn h_list(items: impl IntoIterator<Item = u32>) -> u32 {
    let mut buf = vec![4u8];
    for h in items {
        buf.extend_from_slice(&h.to_le_bytes());
    }
    buf.push(5);
    crc32c(&buf)
}

fn int_array(items: &[i32]) -> u32 {
    h_array(16, 17, items.iter().map(|v| v.to_le_bytes()))
}

fn h_array<const N: usize>(start: u8, end: u8, items: impl IntoIterator<Item = [u8; N]>) -> u32 {
    let mut buf = vec![start];
    for i in items {
        buf.extend_from_slice(&i);
    }
    buf.push(end);
    crc32c(&buf)
}

#[derive(Debug, Default)]
pub struct MapBuilder(Vec<(u32, u32)>);

impl MapBuilder {
    pub fn put(&mut self, key: &str, value: u32) -> &mut Self {
        self.0.push((h_str(key), value));
        self
    }

    pub fn put_if(&mut self, cond: bool, key: &str, value: impl FnOnce() -> u32) -> &mut Self {
        if cond {
            self.put(key, value());
        }
        self
    }

    pub fn build(mut self) -> u32 {
        self.0.sort_unstable();
        let mut buf = vec![2u8];
        for (k, v) in &self.0 {
            buf.extend_from_slice(&k.to_le_bytes());
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.push(3);
        crc32c(&buf)
    }
}

pub fn h_nbt(value: &Value) -> u32 {
    nbt(value, 0)
}

fn nbt(value: &Value, depth: usize) -> u32 {
    if depth > MAX_NBT_DEPTH {
        return NBT_DEPTH_LIMIT_HASH;
    }
    match value {
        Value::Byte(v) => h_byte(*v),
        Value::Short(v) => h_short(*v),
        Value::Int(v) => h_int(*v),
        Value::Long(v) => h_long(*v),
        Value::Float(v) => h_float(*v),
        Value::Double(v) => h_double(*v),
        Value::String(s) => h_str(s),
        Value::ByteArray(v) => h_array(14, 15, v.iter().map(|b| b.to_le_bytes())),
        Value::IntArray(v) => h_array(16, 17, v.iter().map(|b| b.to_le_bytes())),
        Value::LongArray(v) => h_array(18, 19, v.iter().map(|b| b.to_le_bytes())),
        Value::List(l) => h_list(nbt_list(l, depth)),
        Value::Compound(c) => nbt_compound(c, depth),
    }
}

fn nbt_compound(c: &Compound, depth: usize) -> u32 {
    let mut map = MapBuilder::default();
    for (k, v) in c.iter() {
        map.put(k, nbt(v, depth + 1));
    }
    map.build()
}

fn nbt_list(l: &List, depth: usize) -> Vec<u32> {
    let d = depth + 1;
    match l {
        List::Empty => Vec::new(),
        List::Byte(v) => v.iter().map(|x| h_byte(*x)).collect(),
        List::Short(v) => v.iter().map(|x| h_short(*x)).collect(),
        List::Int(v) => v.iter().map(|x| h_int(*x)).collect(),
        List::Long(v) => v.iter().map(|x| h_long(*x)).collect(),
        List::Float(v) => v.iter().map(|x| h_float(*x)).collect(),
        List::Double(v) => v.iter().map(|x| h_double(*x)).collect(),
        List::String(v) => v.iter().map(|x| h_str(x)).collect(),
        List::ByteArray(v) => v
            .iter()
            .map(|a| h_array(14, 15, a.iter().map(|b| b.to_le_bytes())))
            .collect(),
        List::IntArray(v) => v
            .iter()
            .map(|a| h_array(16, 17, a.iter().map(|b| b.to_le_bytes())))
            .collect(),
        List::LongArray(v) => v
            .iter()
            .map(|a| h_array(18, 19, a.iter().map(|b| b.to_le_bytes())))
            .collect(),
        List::List(v) => v
            .iter()
            .map(|x| {
                if d > MAX_NBT_DEPTH {
                    NBT_DEPTH_LIMIT_HASH
                } else {
                    h_list(nbt_list(x, d))
                }
            })
            .collect(),
        List::Compound(v) => v.iter().map(|x| nbt_compound(x, d)).collect(),
    }
}

/// Resolves registry ids and counts what could not be hashed.
#[derive(Debug)]
pub struct HashCx<'a> {
    pub data: &'a Version,
    pub registries: &'a Registries,
    unsupported: Cell<u32>,
}

impl<'a> HashCx<'a> {
    pub fn new(data: &'a Version, registries: &'a Registries) -> Self {
        Self {
            data,
            registries,
            unsupported: Cell::new(0),
        }
    }

    pub fn unsupported(&self) -> u32 {
        self.unsupported.get()
    }

    fn registry_name(&self, registry: &str, id: i32) -> Option<String> {
        if let Some(r) = self.registries.get(registry) {
            return r.get(id).map(|i| i.id.as_str().to_owned());
        }
        let id = u32::try_from(id).ok()?;
        self.data.registry(registry)?.name_of(id).map(str::to_owned)
    }

    fn registry_ref(&self, registry: &str, id: i32) -> Option<u32> {
        self.registry_name(registry, id).map(|n| h_str(&n))
    }

    pub fn hash_stack(&self, stack: &ItemStack) -> HashedStack {
        if stack.is_empty() {
            return HashedStack::Empty;
        }
        HashedStack::Item(HashedItem {
            item: stack.item,
            count: stack.count,
            components: self.hash_patch(&stack.components),
        })
    }

    pub fn hash_patch(&self, patch: &DataComponentPatch) -> HashedPatchMap {
        HashedPatchMap {
            added: LengthPrefixed(
                patch
                    .added
                    .iter()
                    .take(256)
                    .map(|c| HashedComponent {
                        type_id: c.type_id(),
                        hash: self.component_hash(c).unwrap_or_else(|| {
                            self.unsupported.set(self.unsupported.get() + 1);
                            0
                        }) as i32,
                    })
                    .collect(),
            ),
            removed: LengthPrefixed(patch.removed.iter().take(256).map(|&i| VarInt(i)).collect()),
        }
    }

    fn patch_map(&self, patch: &DataComponentPatch) -> Option<u32> {
        let mut map = MapBuilder::default();
        for c in &patch.added {
            let name = COMPONENT_NAMES.get(usize::try_from(c.type_id()).ok()?)?;
            map.put(&format!("minecraft:{name}"), self.component_hash(c)?);
        }
        for &id in &patch.removed {
            let name = COMPONENT_NAMES.get(usize::try_from(id).ok()?)?;
            map.put(&format!("!minecraft:{name}"), h_empty_map());
        }
        Some(map.build())
    }

    fn template(&self, item: i32, count: i32, patch: &DataComponentPatch) -> Option<u32> {
        let mut map = MapBuilder::default();
        map.put("id", self.registry_ref("minecraft:item", item)?);
        if count != 1 {
            map.put("count", h_int(count));
        }
        if !patch.is_empty() {
            map.put("components", self.patch_map(patch)?);
        }
        Some(map.build())
    }

    fn enchantments(&self, e: &[mcr_protocol::types::component::EnchantmentLevel]) -> Option<u32> {
        let mut map = MapBuilder::default();
        for l in e {
            map.put(
                &self.registry_name("minecraft:enchantment", l.enchantment)?,
                h_int(l.level),
            );
        }
        Some(map.build())
    }

    pub fn component(&self, c: &Component) -> Option<u32> {
        component_hash(c)
    }

    /// `None` when the component type has no encoder here.
    pub fn component_hash(&self, c: &DataComponent) -> Option<u32> {
        use DataComponent as D;
        Some(match c {
            D::CustomData(v) => h_nbt(&v.0),
            D::IntangibleProjectile(_) => h_empty_map(),
            D::MaxStackSize(v) | D::MaxDamage(v) | D::Damage(v) | D::RepairCost(v) => h_int(v.0),
            D::Unbreakable(_) | D::Glider(_) | D::Waxed(_) => h_empty_map(),
            D::MinimumAttackCharge(v) | D::PotionDurationScale(v) => h_float(*v),
            D::CustomName(v) | D::ItemName(v) => component_hash(v)?,
            D::ItemModel(v) | D::TooltipStyle(v) | D::NoteBlockSound(v) => h_str(v.as_str()),
            D::Lore(v) => {
                let mut out = Vec::new();
                for line in &v.0.0 {
                    out.push(component_hash(line)?);
                }
                h_list(out)
            }
            D::Rarity(r) => h_str(match r {
                Rarity::Common => "common",
                Rarity::Uncommon => "uncommon",
                Rarity::Rare => "rare",
                Rarity::Epic => "epic",
            }),
            D::Enchantments(e) | D::StoredEnchantments(e) => self.enchantments(&e.0)?,
            D::AttributeModifiers(m) => {
                let mut out = Vec::new();
                for e in &m.0 {
                    let mut map = MapBuilder::default();
                    map.put(
                        "type",
                        self.registry_ref("minecraft:attribute", e.attribute)?,
                    );
                    map.put("id", h_str(e.modifier.id.as_str()));
                    map.put("amount", h_double(e.modifier.amount));
                    map.put(
                        "operation",
                        h_str(match e.modifier.operation {
                            AttributeOperation::AddValue => "add_value",
                            AttributeOperation::AddMultipliedBase => "add_multiplied_base",
                            AttributeOperation::AddMultipliedTotal => "add_multiplied_total",
                        }),
                    );
                    if e.slot != EquipmentSlotGroup::Any {
                        map.put("slot", h_str(slot_group_name(e.slot)));
                    }
                    match &e.display {
                        AttributeDisplay::Default => {}
                        AttributeDisplay::Hidden => {
                            let mut d = MapBuilder::default();
                            d.put("type", h_str("hidden"));
                            map.put("display", d.build());
                        }
                        AttributeDisplay::Override(c) => {
                            let mut d = MapBuilder::default();
                            d.put("type", h_str("override"));
                            d.put("value", component_hash(c)?);
                            map.put("display", d.build());
                        }
                    }
                    out.push(map.build());
                }
                h_list(out)
            }
            D::CustomModelData(m) => {
                let mut map = MapBuilder::default();
                map.put_if(!m.floats.is_empty(), "floats", || {
                    h_list(m.floats.iter().map(|f| h_float(*f)))
                });
                map.put_if(!m.flags.is_empty(), "flags", || {
                    h_list(m.flags.iter().map(|f| h_bool(*f)))
                });
                map.put_if(!m.strings.is_empty(), "strings", || {
                    h_list(m.strings.iter().map(|s| h_str(s)))
                });
                map.put_if(!m.colors.is_empty(), "colors", || {
                    h_list(m.colors.iter().map(|c| h_int(*c)))
                });
                map.build()
            }
            D::TooltipDisplay(t) => {
                let mut map = MapBuilder::default();
                map.put_if(t.hide_tooltip, "hide_tooltip", || h_bool(true));
                if !t.hidden_components.is_empty() {
                    let mut out = Vec::new();
                    for id in &t.hidden_components {
                        let name = COMPONENT_NAMES.get(usize::try_from(id.0).ok()?)?;
                        out.push(h_str(&format!("minecraft:{name}")));
                    }
                    map.put("hidden_components", h_list(out));
                }
                map.build()
            }
            D::EnchantmentGlintOverride(b) => h_bool(*b),
            D::Food(f) => {
                let mut map = MapBuilder::default();
                map.put("nutrition", h_int(f.nutrition));
                map.put("saturation", h_float(f.saturation));
                map.put_if(f.can_always_eat, "can_always_eat", || h_bool(true));
                map.build()
            }
            D::UseCooldown(u) => {
                let mut map = MapBuilder::default();
                map.put("seconds", h_float(u.seconds));
                if let Some(g) = &u.cooldown_group {
                    map.put("cooldown_group", h_str(g.as_str()));
                }
                map.build()
            }
            D::UseRemainder(t) => self.template(t.item, t.count, &t.components)?,
            D::Enchantable(v) => {
                let mut map = MapBuilder::default();
                map.put("value", h_int(v.0));
                map.build()
            }
            D::Dye(c) | D::BaseColor(c) | D::WolfCollar(c) | D::CatCollar(c) | D::SheepColor(c) => {
                h_str(dye_name(*c))
            }
            D::TropicalFishBaseColor(c)
            | D::TropicalFishPatternColor(c)
            | D::ShulkerColor(c)
            | D::CushionColor(c) => h_str(dye_name(*c)),
            D::DyedColor(c) => h_int(*c),
            D::MapId(v) | D::OminousBottleAmplifier(v) => h_int(v.0),
            D::ChargedProjectiles(items) => {
                let mut out = Vec::new();
                for t in &items.0 {
                    out.push(self.template(t.item, t.count, &t.components)?);
                }
                h_list(out)
            }
            D::BundleContents(items) => {
                let mut out = Vec::new();
                for t in items {
                    out.push(self.template(t.item, t.count, &t.components)?);
                }
                h_list(out)
            }
            D::Container(slots) => {
                let mut out = Vec::new();
                for (i, t) in slots.0.iter().enumerate() {
                    if let Some(t) = t {
                        let mut map = MapBuilder::default();
                        map.put("slot", h_int(i as i32));
                        map.put("item", self.template(t.item, t.count, &t.components)?);
                        out.push(map.build());
                    }
                }
                h_list(out)
            }
            D::PotionContents(p) => {
                let mut map = MapBuilder::default();
                if let Some(potion) = p.potion {
                    map.put("potion", self.registry_ref("minecraft:potion", potion.0)?);
                }
                if let Some(color) = p.custom_color {
                    map.put("custom_color", h_int(color));
                }
                if !p.custom_effects.is_empty() {
                    let mut out = Vec::new();
                    for e in &p.custom_effects {
                        out.push(self.mob_effect(e)?);
                    }
                    map.put("custom_effects", h_list(out));
                }
                if let Some(name) = &p.custom_name {
                    map.put("custom_name", h_str(name));
                }
                map.build()
            }
            D::SuspiciousStewEffects(effects) => {
                let mut out = Vec::new();
                for e in effects {
                    let mut map = MapBuilder::default();
                    map.put("id", self.registry_ref("minecraft:mob_effect", e.effect)?);
                    map.put_if(e.duration != 160, "duration", || h_int(e.duration));
                    out.push(map.build());
                }
                h_list(out)
            }
            D::WritableBookContent(pages) => {
                let mut map = MapBuilder::default();
                if !pages.0.is_empty() {
                    map.put(
                        "pages",
                        h_list(pages.0.iter().map(|p| {
                            filterable(h_str(&p.raw.0), p.filtered.as_ref().map(|s| h_str(&s.0)))
                        })),
                    );
                }
                map.build()
            }
            D::WrittenBookContent(b) => {
                let mut map = MapBuilder::default();
                map.put(
                    "title",
                    filterable(
                        h_str(&b.title.raw.0),
                        b.title.filtered.as_ref().map(|s| h_str(&s.0)),
                    ),
                );
                map.put("author", h_str(&b.author));
                map.put_if(b.generation != 0, "generation", || h_int(b.generation));
                if !b.pages.is_empty() {
                    let mut pages = Vec::new();
                    for p in &b.pages {
                        let filtered = match &p.filtered {
                            Some(f) => Some(component_hash(f)?),
                            None => None,
                        };
                        pages.push(filterable(component_hash(&p.raw)?, filtered));
                    }
                    map.put("pages", h_list(pages));
                }
                map.put_if(b.resolved, "resolved", || h_bool(true));
                map.build()
            }
            D::BucketEntityData(c) => nbt_compound(&c.0, 0),
            D::FireworkExplosion(e) => firework_explosion(e),
            D::Fireworks(f) => {
                let mut map = MapBuilder::default();
                map.put_if(f.flight_duration != 0, "flight_duration", || {
                    h_byte(f.flight_duration as i8)
                });
                if !f.explosions.0.is_empty() {
                    map.put(
                        "explosions",
                        h_list(f.explosions.0.iter().map(firework_explosion)),
                    );
                }
                map.build()
            }
            D::BlockState(props) => {
                let mut map = MapBuilder::default();
                for p in props {
                    map.put(&p.name, h_str(&p.value));
                }
                map.build()
            }
            D::BannerPatterns(layers) => {
                let mut out = Vec::new();
                for l in layers {
                    let mut map = MapBuilder::default();
                    match &l.pattern {
                        mcr_buf::IdOr::Id(id) => map.put(
                            "pattern",
                            self.registry_ref("minecraft:banner_pattern", *id)?,
                        ),
                        mcr_buf::IdOr::Inline(_) => return None,
                    };
                    map.put("color", h_str(dye_name(l.color)));
                    out.push(map.build());
                }
                h_list(out)
            }
            D::VillagerVariant(v) => self.registry_ref("minecraft:villager_type", v.0)?,
            D::WolfVariant(v) => self.registry_ref("minecraft:wolf_variant", v.0)?,
            D::WolfSoundVariant(v) => self.registry_ref("minecraft:wolf_sound_variant", v.0)?,
            D::PigVariant(v) => self.registry_ref("minecraft:pig_variant", v.0)?,
            D::PigSoundVariant(v) => self.registry_ref("minecraft:pig_sound_variant", v.0)?,
            D::CowVariant(v) => self.registry_ref("minecraft:cow_variant", v.0)?,
            D::CowSoundVariant(v) => self.registry_ref("minecraft:cow_sound_variant", v.0)?,
            D::ChickenVariant(v) => self.registry_ref("minecraft:chicken_variant", v.0)?,
            D::ChickenSoundVariant(v) => {
                self.registry_ref("minecraft:chicken_sound_variant", v.0)?
            }
            D::ZombieNautilusVariant(v) => {
                self.registry_ref("minecraft:zombie_nautilus_variant", v.0)?
            }
            D::FrogVariant(v) => self.registry_ref("minecraft:frog_variant", v.0)?,
            D::CatVariant(v) => self.registry_ref("minecraft:cat_variant", v.0)?,
            D::CatSoundVariant(v) => self.registry_ref("minecraft:cat_sound_variant", v.0)?,
            D::DamageType(v) => self.registry_ref("minecraft:damage_type", v.0)?,
            _ => return self.component_hash_more(c),
        })
    }
}

fn filterable(raw: u32, filtered: Option<u32>) -> u32 {
    let mut map = MapBuilder::default();
    map.put("raw", raw);
    if let Some(f) = filtered {
        map.put("filtered", f);
    }
    map.build()
}

fn firework_explosion(e: &mcr_protocol::types::component::FireworkExplosion) -> u32 {
    let mut map = MapBuilder::default();
    map.put(
        "shape",
        h_str(match e.shape {
            FireworkShape::SmallBall => "small_ball",
            FireworkShape::LargeBall => "large_ball",
            FireworkShape::Star => "star",
            FireworkShape::Creeper => "creeper",
            FireworkShape::Burst => "burst",
        }),
    );
    map.put_if(!e.colors.is_empty(), "colors", || {
        h_list(e.colors.iter().map(|c| h_int(*c)))
    });
    map.put_if(!e.fade_colors.is_empty(), "fade_colors", || {
        h_list(e.fade_colors.iter().map(|c| h_int(*c)))
    });
    map.put_if(e.has_trail, "has_trail", || h_bool(true));
    map.put_if(e.has_twinkle, "has_twinkle", || h_bool(true));
    map.build()
}

fn equipment_slot_name(s: EquipmentSlot) -> &'static str {
    match s {
        EquipmentSlot::MainHand => "mainhand",
        EquipmentSlot::OffHand => "offhand",
        EquipmentSlot::Feet => "feet",
        EquipmentSlot::Legs => "legs",
        EquipmentSlot::Chest => "chest",
        EquipmentSlot::Head => "head",
        EquipmentSlot::Body => "body",
        EquipmentSlot::Saddle => "saddle",
    }
}

fn slot_group_name(g: EquipmentSlotGroup) -> &'static str {
    match g {
        EquipmentSlotGroup::Any => "any",
        EquipmentSlotGroup::MainHand => "mainhand",
        EquipmentSlotGroup::OffHand => "offhand",
        EquipmentSlotGroup::Hand => "hand",
        EquipmentSlotGroup::Feet => "feet",
        EquipmentSlotGroup::Legs => "legs",
        EquipmentSlotGroup::Chest => "chest",
        EquipmentSlotGroup::Head => "head",
        EquipmentSlotGroup::Armor => "armor",
        EquipmentSlotGroup::Body => "body",
        EquipmentSlotGroup::Saddle => "saddle",
    }
}

fn dye_name(c: DyeColor) -> &'static str {
    const NAMES: [&str; 16] = [
        "white",
        "orange",
        "magenta",
        "light_blue",
        "yellow",
        "lime",
        "pink",
        "gray",
        "light_gray",
        "cyan",
        "purple",
        "blue",
        "brown",
        "green",
        "red",
        "black",
    ];
    NAMES[c as usize]
}

/// Text component through the persistent codec (plain text collapses to a string).
pub fn component_hash(c: &Component) -> Option<u32> {
    component_depth(c, 0)
}

fn component_depth(c: &Component, depth: usize) -> Option<u32> {
    if depth > 64 {
        return None;
    }
    if let Content::Text(t) = &c.content
        && c.extra.is_empty()
        && c.style.is_empty()
    {
        return Some(h_str(t));
    }
    let mut map = MapBuilder::default();
    match &c.content {
        Content::Text(t) => {
            map.put("text", h_str(t));
        }
        Content::Translatable {
            key,
            fallback,
            args,
        } => {
            map.put("translate", h_str(key));
            if let Some(f) = fallback {
                map.put("fallback", h_str(f));
            }
            if !args.is_empty() {
                let mut out = Vec::new();
                for a in args {
                    out.push(match a {
                        mcr_protocol::types::text::Arg::Str(s) => h_str(s),
                        mcr_protocol::types::text::Arg::Component(c) => {
                            component_depth(c, depth + 1)?
                        }
                        mcr_protocol::types::text::Arg::Number(n) => nbt(n, 0),
                    });
                }
                map.put("with", h_list(out));
            }
        }
        Content::Keybind(k) => {
            map.put("keybind", h_str(k));
        }
        Content::Score { name, objective } => {
            let mut s = MapBuilder::default();
            s.put("name", h_str(name));
            s.put("objective", h_str(objective));
            map.put("score", s.build());
        }
        Content::Selector {
            selector,
            separator,
        } => {
            map.put("selector", h_str(selector));
            if let Some(sep) = separator {
                map.put("separator", component_depth(sep, depth + 1)?);
            }
        }
        Content::Nbt(n) => {
            map.put("nbt", h_str(&n.path));
            map.put_if(n.interpret, "interpret", || h_bool(true));
            map.put_if(n.plain, "plain", || h_bool(true));
            if let Some(sep) = &n.separator {
                map.put("separator", component_depth(sep, depth + 1)?);
            }
            match &n.source {
                NbtSource::Entity(s) => map.put("entity", h_str(s)),
                NbtSource::Block(s) => map.put("block", h_str(s)),
                NbtSource::Storage(id) => map.put("storage", h_str(id.as_str())),
            };
        }
        Content::Object { object, fallback } => {
            match object {
                ObjectContent::Atlas { atlas, sprite } => {
                    map.put_if(atlas.as_str() != "minecraft:blocks", "atlas", || {
                        h_str(atlas.as_str())
                    });
                    map.put("sprite", h_str(sprite.as_str()));
                }
                ObjectContent::Player { player, hat } => {
                    map.put("player", nbt(player, 0));
                    map.put_if(!hat, "hat", || h_bool(false));
                }
            }
            if let Some(f) = fallback {
                map.put("fallback", component_depth(f, depth + 1)?);
            }
        }
    }
    if !c.extra.is_empty() {
        let mut out = Vec::new();
        for e in &c.extra {
            out.push(component_depth(e, depth + 1)?);
        }
        map.put("extra", h_list(out));
    }
    let s = &c.style;
    if let Some(color) = s.color {
        map.put("color", h_str(&Color::serialize(color)));
    }
    if let Some(v) = s.shadow_color {
        map.put("shadow_color", h_int(v));
    }
    for (name, v) in [
        ("bold", s.bold),
        ("italic", s.italic),
        ("underlined", s.underlined),
        ("strikethrough", s.strikethrough),
        ("obfuscated", s.obfuscated),
    ] {
        if let Some(v) = v {
            map.put(name, h_bool(v));
        }
    }
    if let Some(e) = &s.click_event {
        map.put("click_event", click_event(e)?);
    }
    if let Some(h) = &s.hover_event {
        map.put("hover_event", hover_event(h, depth)?);
    }
    if let Some(i) = &s.insertion {
        map.put("insertion", h_str(i));
    }
    if let Some(f) = &s.font {
        map.put("font", h_str(f.as_str()));
    }
    Some(map.build())
}

fn click_event(e: &ClickEvent) -> Option<u32> {
    let mut m = MapBuilder::default();
    let (action, key, value) = match e {
        ClickEvent::OpenUrl(v) => ("open_url", "url", h_str(v)),
        ClickEvent::OpenFile(v) => ("open_file", "path", h_str(v)),
        ClickEvent::RunCommand(v) => ("run_command", "command", h_str(v)),
        ClickEvent::SuggestCommand(v) => ("suggest_command", "command", h_str(v)),
        ClickEvent::ShowDialog(v) => ("show_dialog", "dialog", nbt(v, 0)),
        ClickEvent::ChangePage(v) => ("change_page", "page", h_int(*v)),
        ClickEvent::CopyToClipboard(v) => ("copy_to_clipboard", "value", h_str(v)),
        ClickEvent::Custom { id, payload } => {
            m.put("id", h_str(id.as_str()));
            if let Some(p) = payload {
                m.put("payload", nbt(p, 0));
            }
            m.put("action", h_str("custom"));
            return Some(m.build());
        }
    };
    m.put("action", h_str(action));
    m.put(key, value);
    Some(m.build())
}

fn hover_event(h: &HoverEvent, depth: usize) -> Option<u32> {
    let mut m = MapBuilder::default();
    match h {
        HoverEvent::ShowText(t) => {
            m.put("action", h_str("show_text"));
            m.put("value", component_depth(t, depth + 1)?);
        }
        HoverEvent::ShowItem(item) => {
            m.put("action", h_str("show_item"));
            for (k, v) in item.iter() {
                m.put(k, nbt(v, 1));
            }
        }
        HoverEvent::ShowEntity { entity, uuid, name } => {
            m.put("action", h_str("show_entity"));
            m.put("id", h_str(entity.as_str()));
            let v = uuid.as_u128();
            m.put(
                "uuid",
                int_array(&[
                    (v >> 96) as i32,
                    (v >> 64) as i32,
                    (v >> 32) as i32,
                    v as i32,
                ]),
            );
            if let Some(n) = name {
                m.put("name", component_depth(n, depth + 1)?);
            }
        }
    }
    Some(m.build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32c_known_vector() {
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn primitive_payload_layout() {
        assert_eq!(h_int(1), crc32c(&[8, 1, 0, 0, 0]));
        assert_eq!(h_str("a"), crc32c(&[12, 1, 0, 0, 0, b'a', 0]));
        assert_eq!(h_bool(true), crc32c(&[13, 1]));
        assert_eq!(h_empty_map(), crc32c(&[2, 3]));
        assert_eq!(h_list([]), crc32c(&[4, 5]));
    }

    #[test]
    fn map_entries_sort_by_key_hash() {
        let mut a = MapBuilder::default();
        a.put("x", h_int(1)).put("y", h_int(2));
        let mut b = MapBuilder::default();
        b.put("y", h_int(2)).put("x", h_int(1));
        assert_eq!(a.build(), b.build());
    }

    #[test]
    fn plain_text_collapses_to_string() {
        assert_eq!(component_hash(&Component::text("hi")), Some(h_str("hi")));
    }
}
