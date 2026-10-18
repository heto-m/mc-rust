//! Item stacks and their data components, in both directions: old layout to canonical going in,
//! canonical to old layout for the stacks the client sends.

use mcr_buf::{
    DecodeError, Identifier, Result, decode_str, encode_str, read_len, read_varint, take,
    write_varint,
};
use mcr_nbt::{Compound, Limits, Value};

use crate::types::SoundEvent;
use crate::types::component::{
    AttributeDisplay, AttributeModifier, COMPONENT_NAMES, DamageReduction, EquipmentSlotGroup,
    HolderSet, ItemDamageFunction, KineticCondition, MobEffectInstance, StatePropertiesPredicate,
    component_type_id, skip_value,
};
use crate::types::item::ItemStackTemplate;
use crate::version::io::{self, Depth};
use crate::version::translator::{Maps, Remap};

fn new_component(name: &str) -> i32 {
    component_type_id(name).expect("canonical component")
}

/// One direction of the item codec: `back` reads canonical bytes and writes the old layout.
#[derive(Clone, Copy)]
pub struct Codec {
    pub m: &'static Maps,
    pub back: bool,
}

impl Codec {
    pub fn fwd(m: &'static Maps) -> Self {
        Self { m, back: false }
    }

    pub fn back(m: &'static Maps) -> Self {
        Self { m, back: true }
    }

    fn map(self, remap: Remap<'_>, id: i32) -> Option<i32> {
        if self.back {
            remap.back(id)
        } else {
            remap.forward(id)
        }
    }

    fn id(
        self,
        r: &mut &[u8],
        w: &mut Vec<u8>,
        remap: Remap<'_>,
        what: &'static str,
    ) -> Result<i32> {
        io::remap(r, w, what, |id| self.map(remap, id))
    }

    fn item(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<i32> {
        self.id(r, w, self.m.item, "item")
    }

    /// Target-side ids for one source component id; old `swing_animation` split in two, `map_color` is gone.
    fn component_ids(self, id: i32) -> Result<([i32; 2], usize)> {
        if self.back {
            let name = COMPONENT_NAMES.get(usize::try_from(id).unwrap_or(usize::MAX));
            if name == Some(&"attack_animation") {
                return Ok(self
                    .m
                    .old_swing_animation
                    .map_or(([0; 2], 0), |o| ([o, 0], 1)));
            }
            if name == Some(&"interact_animation") {
                return Ok(([0; 2], 0));
            }
            return Ok(self
                .m
                .component
                .back(id)
                .map_or(([0; 2], 0), |o| ([o, 0], 1)));
        }
        if Some(id) == self.m.old_swing_animation {
            return Ok((
                [
                    new_component("attack_animation"),
                    new_component("interact_animation"),
                ],
                2,
            ));
        }
        if Some(id) == self.m.old_map_color {
            return Ok(([0; 2], 0));
        }
        let new = self
            .m
            .component
            .forward(id)
            .ok_or(io::unknown("data component type", id))?;
        Ok(([new, 0], 1))
    }

    /// Optional stack: a count of zero or less carries nothing else.
    pub fn stack(self, r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
        let count = io::varint(r, w)?;
        if count <= 0 {
            return Ok(());
        }
        self.item(r, w)?;
        self.patch(r, w, d)
    }

    pub fn template(self, r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
        self.item(r, w)?;
        io::varint(r, w)?;
        self.patch(r, w, d)
    }

    pub fn patch(self, r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
        let d = d.deeper()?;
        let added = read_len(r, r.len())?;
        let removed = read_len(r, r.len())?;
        if added == 0 && removed == 0 {
            w.extend_from_slice(&[0, 0]);
            return Ok(());
        }
        let mut body = Vec::new();
        let mut new_added = 0;
        for _ in 0..added {
            new_added += self.component(r, &mut body, d)?;
        }
        let new_removed = self.removed(r, &mut body, removed)?;
        write_varint(w, new_added);
        write_varint(w, new_removed);
        w.extend_from_slice(&body);
        Ok(())
    }

    fn removed(self, r: &mut &[u8], w: &mut Vec<u8>, n: usize) -> Result<i32> {
        let mut count = 0;
        for _ in 0..n {
            let (ids, k) = self.component_ids(read_varint(r)?)?;
            for &id in &ids[..k] {
                write_varint(w, id);
                count += 1;
            }
        }
        Ok(count)
    }

    /// One type id and value; returns how many target components it became.
    pub fn component(self, r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<i32> {
        let id = read_varint(r)?;
        let (ids, k) = self.component_ids(id)?;
        let canonical = if self.back { id } else { ids[0] };
        if k == 0 {
            if self.back {
                skip_value(canonical, r)?;
            } else {
                take(r, 4)?;
            }
            return Ok(0);
        }
        if k == 2 {
            let start = *r;
            read_varint(r)?;
            read_varint(r)?;
            let value = &start[..start.len() - r.len()];
            for id in ids {
                write_varint(w, id);
                w.extend_from_slice(value);
            }
            return Ok(2);
        }
        write_varint(w, ids[0]);
        self.value(canonical, r, w, d)?;
        Ok(1)
    }

    fn templates(self, r: &mut &[u8], w: &mut Vec<u8>, max: usize, d: Depth) -> Result<()> {
        for _ in 0..io::len(r, w, max.min(r.len()))? {
            self.template(r, w, d)?;
        }
        Ok(())
    }

    fn value(self, canonical: i32, r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
        let m = self.m;
        match COMPONENT_NAMES[canonical as usize] {
            "container" => {
                for _ in 0..io::len(r, w, 256)? {
                    if io::bool(r, w)? {
                        self.template(r, w, d)?;
                    }
                }
            }
            "bundle_contents" => self.templates(r, w, usize::MAX, d)?,
            "charged_projectiles" => self.templates(r, w, 1024, d)?,
            "use_remainder" | "sulfur_cube_content" => self.template(r, w, d)?,
            "pot_decorations" if self.back => self.pot_decorations_back(r, w)?,
            "pot_decorations" => self.pot_decorations(r, w)?,
            "tooltip_display" => {
                io::bool(r, w)?;
                let n = read_len(r, r.len())?;
                let mut ids = Vec::with_capacity(n.min(256));
                for _ in 0..n {
                    let (new, k) = self.component_ids(read_varint(r)?)?;
                    ids.extend_from_slice(&new[..k]);
                }
                write_varint(w, ids.len() as i32);
                for id in ids {
                    write_varint(w, id);
                }
            }
            "repairable" => self.holder_set(r, w, m.item, "item")?,
            "tool" => {
                for _ in 0..io::len(r, w, r.len())? {
                    self.holder_set(r, w, m.block, "block")?;
                    io::copy::<Option<f32>>(r, w)?;
                    io::copy::<Option<bool>>(r, w)?;
                }
                io::copy::<f32>(r, w)?;
                io::varint(r, w)?;
                io::bool(r, w)?;
            }
            "attribute_modifiers" => {
                for _ in 0..io::len(r, w, r.len())? {
                    self.id(r, w, m.attribute, "attribute")?;
                    io::copy::<AttributeModifier>(r, w)?;
                    io::copy::<EquipmentSlotGroup>(r, w)?;
                    io::copy::<AttributeDisplay>(r, w)?;
                }
            }
            "entity_data" => self.typed_entity(r, w)?,
            "bees" => {
                for _ in 0..io::len(r, w, r.len())? {
                    self.typed_entity(r, w)?;
                    io::varint(r, w)?;
                    io::varint(r, w)?;
                }
            }
            "break_sound" => self.sound(r, w)?,
            "equippable" => {
                io::varint(r, w)?;
                self.sound(r, w)?;
                io::copy::<Option<Identifier>>(r, w)?;
                io::copy::<Option<Identifier>>(r, w)?;
                if io::bool(r, w)? {
                    self.holder_set(r, w, m.entity, "entity type")?;
                }
                for _ in 0..5 {
                    io::bool(r, w)?;
                }
                self.sound(r, w)?;
            }
            "consumable" => {
                io::copy::<f32>(r, w)?;
                io::varint(r, w)?;
                self.sound(r, w)?;
                io::bool(r, w)?;
                self.consume_effects(r, w)?;
            }
            "death_protection" => self.consume_effects(r, w)?,
            "blocks_attacks" => {
                io::copy::<f32>(r, w)?;
                io::copy::<f32>(r, w)?;
                io::copy::<Vec<DamageReduction>>(r, w)?;
                io::copy::<ItemDamageFunction>(r, w)?;
                io::copy::<Option<HolderSet>>(r, w)?;
                self.optional_sound(r, w)?;
                self.optional_sound(r, w)?;
            }
            "piercing_weapon" => {
                io::bool(r, w)?;
                io::bool(r, w)?;
                self.optional_sound(r, w)?;
                self.optional_sound(r, w)?;
            }
            "kinetic_weapon" => {
                io::varint(r, w)?;
                io::varint(r, w)?;
                for _ in 0..3 {
                    io::copy::<Option<KineticCondition>>(r, w)?;
                }
                io::copy::<f32>(r, w)?;
                io::copy::<f32>(r, w)?;
                self.optional_sound(r, w)?;
                self.optional_sound(r, w)?;
            }
            "instrument" => {
                if io::varint(r, w)? == 0 {
                    self.sound(r, w)?;
                    io::bytes(r, w, 8)?;
                    // 26.3 added durability damage; older instruments never damage the item.
                    if self.back {
                        read_varint(r)?;
                    } else {
                        write_varint(w, 0);
                    }
                    io::text(r, w)?;
                }
            }
            "jukebox_playable" => {
                if io::varint(r, w)? == 0 {
                    self.sound(r, w)?;
                    io::text(r, w)?;
                    io::copy::<f32>(r, w)?;
                    io::varint(r, w)?;
                }
            }
            "trim" => {
                self.trim_material(r, w)?;
                if io::varint(r, w)? == 0 {
                    io::copy::<Identifier>(r, w)?;
                    io::text(r, w)?;
                    io::bool(r, w)?;
                }
            }
            "provides_trim_material" => self.trim_material(r, w)?,
            "can_place_on" | "can_break" => {
                for _ in 0..io::len(r, w, r.len())? {
                    self.block_predicate(r, w, d)?;
                }
            }
            _ => {
                let start = *r;
                skip_value(canonical, r)?;
                w.extend_from_slice(&start[..start.len() - r.len()]);
            }
        }
        Ok(())
    }

    /// 26.2 lists up to four sherd item ids (brick = blank); 26.3 has four optional stacks.
    fn pot_decorations(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
        let n = read_len(r, 4)?;
        let mut sides = [None; 4];
        for side in sides.iter_mut().take(n) {
            let old = read_varint(r)?;
            *side = Some(self.m.item.forward(old).ok_or(io::unknown("item", old))?);
        }
        let brick = self.m.canonical.brick;
        for side in sides {
            match side.filter(|&id| Some(id) != brick) {
                Some(id) => {
                    w.push(1);
                    write_varint(w, id);
                    w.extend_from_slice(&[1, 0, 0]);
                }
                None => w.push(0),
            }
        }
        Ok(())
    }

    fn pot_decorations_back(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
        let brick = self
            .m
            .canonical
            .brick
            .and_then(|b| self.m.item.back(b))
            .ok_or(io::unknown("item", 0))?;
        write_varint(w, 4);
        for _ in 0..4 {
            let side = if take(r, 1)?[0] != 0 {
                let template = <ItemStackTemplate as mcr_buf::Decode>::decode(r)?;
                self.m.item.back(template.item)
            } else {
                None
            };
            write_varint(w, side.unwrap_or(brick));
        }
        Ok(())
    }

    /// Inline 26.2 materials name an asset suffix and per-armour overrides; 26.3 names a palette.
    fn trim_material(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
        if io::varint(r, w)? != 0 {
            return Ok(());
        }
        if self.back {
            let palette = <Identifier as mcr_buf::Decode>::decode(r)?;
            let path = palette.as_str().split_once(':').map_or("", |(_, p)| p);
            encode_str(path.strip_prefix("trim/").unwrap_or(path), w);
            write_varint(w, 0);
            return io::text(r, w);
        }
        let base = decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        let palette = Identifier::parse(&format!("minecraft:trim/{base}"))
            .map_err(|_| DecodeError::Other("trim material asset name".into()))?;
        for _ in 0..read_len(r, r.len())? {
            <Identifier as mcr_buf::Decode>::decode(r)?;
            decode_str(r, mcr_buf::MAX_STRING_CHARS)?;
        }
        encode_str(palette.as_str(), w);
        io::text(r, w)
    }

    fn block_predicate(self, r: &mut &[u8], w: &mut Vec<u8>, d: Depth) -> Result<()> {
        let d = d.deeper()?;
        if io::bool(r, w)? {
            self.holder_set(r, w, self.m.block, "block")?;
        }
        io::copy::<Option<StatePropertiesPredicate>>(r, w)?;
        if io::bool(r, w)? {
            io::compound(r, w)?;
        }
        let exact = read_len(r, r.len())?;
        let mut body = Vec::new();
        let mut count = 0;
        for _ in 0..exact {
            count += self.component(r, &mut body, d)?;
        }
        write_varint(w, count);
        w.extend_from_slice(&body);
        let potions = self.m.canonical.potion_contents_predicate;
        for _ in 0..io::len(r, w, 64)? {
            if io::bool(r, w)? {
                let kind = io::varint(r, w)?;
                if Some(kind) == potions {
                    self.potion_predicate(r, w)?;
                    continue;
                }
            } else {
                let id = read_varint(r)?;
                let (ids, k) = self.component_ids(id)?;
                if k == 0 {
                    return Err(io::unknown("component predicate type", id));
                }
                write_varint(w, ids[0]);
            }
            io::nbt(r, w, false)?;
        }
        Ok(())
    }

    /// 26.2's potion predicate is a bare holder set; 26.3 wraps it as `{potions: ...}`.
    fn potion_predicate(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
        let nbt_error = |e: mcr_nbt::NbtError| DecodeError::Other(format!("nbt: {e}"));
        let value = mcr_nbt::read_network(r, Limits::NETWORK)
            .map_err(nbt_error)?
            .ok_or_else(|| DecodeError::Other("nbt: unexpected end tag".into()))?;
        let out = if self.back {
            match value {
                Value::Compound(mut c) => c
                    .remove("potions")
                    .unwrap_or(Value::List(mcr_nbt::List::Empty)),
                other => other,
            }
        } else {
            let mut c = Compound::new();
            c.insert("potions", value);
            Value::Compound(c)
        };
        mcr_nbt::write_network(w, Some(&out));
        Ok(())
    }

    pub fn holder_set(
        self,
        r: &mut &[u8],
        w: &mut Vec<u8>,
        map: Remap<'_>,
        what: &'static str,
    ) -> Result<()> {
        let n = read_len(r, r.len() + 1)?;
        write_varint(w, n as i32);
        if n == 0 {
            return io::copy::<Identifier>(r, w);
        }
        for _ in 1..n {
            self.id(r, w, map, what)?;
        }
        Ok(())
    }

    fn typed_entity(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
        self.id(r, w, self.m.entity, "entity type")?;
        io::compound(r, w)
    }

    /// `Holder<SoundEvent>`: registry id + 1, or 0 and an inline event.
    pub fn sound(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
        let v = read_varint(r)?;
        if v == 0 {
            write_varint(w, 0);
            return io::copy::<SoundEvent>(r, w);
        }
        let new = self
            .map(self.m.sound, v - 1)
            .ok_or(io::unknown("sound event", v - 1))?;
        write_varint(w, new + 1);
        Ok(())
    }

    fn optional_sound(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
        if io::bool(r, w)? {
            self.sound(r, w)?;
        }
        Ok(())
    }

    fn consume_effects(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
        for _ in 0..io::len(r, w, r.len())? {
            match io::varint(r, w)? {
                0 => {
                    io::copy::<Vec<MobEffectInstance>>(r, w)?;
                    io::copy::<f32>(r, w)?;
                }
                1 => io::copy::<HolderSet>(r, w)?,
                2 => {}
                3 => {
                    io::copy::<f32>(r, w)?;
                    // 26.3's particle flag; older teleports always show particles.
                    if self.back {
                        take(r, 1)?;
                    } else {
                        w.push(1);
                    }
                }
                4 => self.sound(r, w)?,
                other => return Err(io::unknown("consume effect", other)),
            }
        }
        Ok(())
    }

    /// Client stack with length-prefixed values, canonical to old; `false` when the item has no old id.
    pub fn untrusted_stack_back(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<bool> {
        let count = io::varint(r, w)?;
        if count <= 0 {
            return Ok(true);
        }
        let new = read_varint(r)?;
        let Some(old) = self.m.item.back(new) else {
            return Ok(false);
        };
        write_varint(w, old);
        let added = read_len(r, r.len())?;
        let removed = read_len(r, r.len())?;
        let mut body = Vec::new();
        let mut value = Vec::new();
        let mut kept_added = 0;
        for _ in 0..added {
            let id = read_varint(r)?;
            let len = read_len(r, r.len())?;
            let mut bytes = take(r, len)?;
            let (ids, k) = self.component_ids(id)?;
            if k == 0 {
                continue;
            }
            value.clear();
            self.value(id, &mut bytes, &mut value, Depth::top())?;
            if !bytes.is_empty() {
                return Err(DecodeError::Other(
                    "component value has trailing bytes".into(),
                ));
            }
            write_varint(&mut body, ids[0]);
            write_varint(&mut body, value.len() as i32);
            body.extend_from_slice(&value);
            kept_added += 1;
        }
        let kept_removed = self.removed(r, &mut body, removed)?;
        write_varint(w, kept_added);
        write_varint(w, kept_removed);
        w.extend_from_slice(&body);
        Ok(true)
    }

    /// A hashed stack from a container click, canonical to old. Unknown items and components are
    /// left out; the server then resends the slot, as it does after any hash mismatch.
    pub fn hashed_stack_back(self, r: &mut &[u8], w: &mut Vec<u8>) -> Result<()> {
        if take(r, 1)?[0] == 0 {
            w.push(0);
            return Ok(());
        }
        let new = read_varint(r)?;
        let count = read_varint(r)?;
        let mut body = Vec::new();
        for removed in [false, true] {
            let n = read_len(r, 256)?;
            let mut kept = Vec::new();
            for _ in 0..n {
                let id = read_varint(r)?;
                let hash = if removed { None } else { Some(take(r, 4)?) };
                let (ids, k) = self.component_ids(id)?;
                if k == 1 {
                    kept.push((ids[0], hash));
                }
            }
            write_varint(&mut body, kept.len() as i32);
            for (id, hash) in kept {
                write_varint(&mut body, id);
                if let Some(hash) = hash {
                    body.extend_from_slice(hash);
                }
            }
        }
        match self.m.item.back(new) {
            Some(old) => {
                w.push(1);
                write_varint(w, old);
                write_varint(w, count);
                w.extend_from_slice(&body);
            }
            None => w.push(0),
        }
        Ok(())
    }
}

/// 26.2 decorated pots list their sherd item names (back, left, right, front; brick = blank); 26.3
/// stores each side as an optional stack.
pub fn pot_sherds(tag: &mut Compound) {
    let Some(Value::List(mcr_nbt::List::String(names))) = tag.get("sherds") else {
        return;
    };
    let mut sides = Compound::new();
    for (side, name) in ["back", "left", "right", "front"].into_iter().zip(names) {
        let id = if name.contains(':') {
            name.clone()
        } else {
            format!("minecraft:{name}")
        };
        if id == "minecraft:brick" {
            continue;
        }
        let mut stack = Compound::new();
        stack.insert("id", Value::String(id));
        stack.insert("count", Value::Int(1));
        sides.insert(side, Value::Compound(stack));
    }
    if sides.is_empty() {
        tag.remove("sherds");
    } else {
        tag.insert("sherds", Value::Compound(sides));
    }
}

#[cfg(test)]
pub(super) fn old_component_id(name: &str) -> i32 {
    super::translator()
        .tables()
        .old_names("registry/minecraft:data_component_type")
        .iter()
        .position(|n| n.strip_prefix("minecraft:") == Some(name))
        .unwrap_or_else(|| panic!("no 26.2 component {name}")) as i32
}
