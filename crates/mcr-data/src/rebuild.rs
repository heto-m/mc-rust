//! Tables rebuilt from a modded client's registries: blocks, states, items, entity types and fluids
//! replaced wholesale (mods add properties to vanilla blocks, so vanilla's state ids move), packets and
//! everything else kept from the base version.

use std::collections::HashMap;

use crate::tables::{
    Aabb, BlockTable, EntityTable, ItemTable, PropDef, Registry, StateTable, Tables, fluid_bits,
};

pub const MAX_REGISTRIES: usize = 1024;
pub const MAX_ENTRIES: usize = 1 << 18;
pub const MAX_NAME: usize = 256;
pub const MAX_PROPERTIES: usize = 32;
pub const MAX_VALUES: usize = 1024;
/// Matches `mcr_world::WorldConfig::MAX_BLOCK_STATES`.
pub const MAX_BLOCK_STATES: usize = 1 << 20;
pub const MAX_SHAPES: usize = 1 << 16;
pub const MAX_SHAPE_BOXES: usize = 256;
pub const MAX_TOOL_RULES: usize = 256;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Export {
    /// Static registries by name, entries in id order. A registry the base has must start with its entries.
    pub registries: Vec<(String, Vec<String>)>,
    /// Every block in id order; the same for items, entity types and fluids.
    pub blocks: Vec<ExportBlock>,
    pub items: Vec<ExportItem>,
    pub entity_types: Vec<ExportEntityType>,
    pub fluids: Vec<ExportFluid>,
    /// Distinct voxel shapes the states index.
    pub shapes: Vec<Vec<Aabb>>,
    /// Distinct fluid states the states index.
    pub fluid_states: Vec<ExportFluidState>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportBlock {
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    pub destroy_time: f32,
    pub requires_tool: bool,
    pub explosion_resistance: f32,
    /// The break sound's event name.
    pub sound_type: String,
    /// Sorted by name, values in their definition order; states enumerate them in radix order.
    pub properties: Vec<(String, Vec<String>)>,
    /// Index into `states`.
    pub default_state: u32,
    pub states: Vec<ExportState>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ExportState {
    /// Indices into [`Export::shapes`].
    pub collision: u32,
    pub outline: u32,
    pub occlusion: u32,
    pub light_emission: u8,
    pub light_block: u8,
    /// [`crate::tables::flag`] bits; the rest are ignored.
    pub flags: u16,
    pub map_color: u8,
    pub sturdy_faces: u8,
    pub occlusion_faces: u8,
    /// Index into [`Export::fluid_states`].
    pub fluid: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportFluidState {
    pub fluid: String,
    pub amount: u8,
    pub source: bool,
    pub falling: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportItem {
    pub max_stack_size: u16,
    pub max_damage: u32,
    pub block: Option<String>,
    pub tool: Option<ExportTool>,
    /// Index into head, chest, legs, feet, body, saddle, mainhand, offhand; 255 for none.
    pub equip_slot: u8,
    pub consume_seconds: Option<f32>,
    pub cooldown_seconds: Option<f32>,
    /// `Some(can_always_eat)` with a `food` component.
    pub food: Option<bool>,
    /// Ids below 128 of the data component types among the item's defaults.
    pub default_components: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportTool {
    pub default_mining_speed: f32,
    pub damage_per_block: u8,
    pub can_destroy_blocks_in_creative: bool,
    /// Block name or `#tag`, speed, correct for drops.
    pub rules: Vec<(String, Option<f32>, Option<bool>)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportEntityType {
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
    pub category: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExportFluid {
    pub source: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RebuildError {
    #[error("registry {0} does not start with the base version's entries")]
    Reordered(String),
    #[error("registry {0} lists {1} entries, but {2} were described")]
    Count(&'static str, usize, usize),
    #[error("{0}")]
    Invalid(String),
    #[error("too many {0}")]
    TooMany(&'static str),
}

fn invalid(what: impl Into<String>) -> RebuildError {
    RebuildError::Invalid(what.into())
}

struct Strings<'a> {
    strings: &'a mut Vec<String>,
    index: HashMap<String, u32>,
}

impl<'a> Strings<'a> {
    fn new(strings: &'a mut Vec<String>) -> Self {
        let index = strings
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i as u32))
            .collect();
        Self { strings, index }
    }

    fn intern(&mut self, s: &str) -> Result<u32, RebuildError> {
        if let Some(&i) = self.index.get(s) {
            return Ok(i);
        }
        if s.len() > MAX_NAME * 4 {
            return Err(invalid("string too long"));
        }
        let i = u32::try_from(self.strings.len()).map_err(|_| RebuildError::TooMany("strings"))?;
        self.strings.push(s.to_owned());
        self.index.insert(s.to_owned(), i);
        Ok(i)
    }
}

fn bounded_name(name: &str) -> Result<(), RebuildError> {
    if name.is_empty() || name.len() > MAX_NAME || name.chars().any(char::is_control) {
        return Err(invalid(format!(
            "bad registry entry name {:?}",
            name.chars().take(64).collect::<String>()
        )));
    }
    Ok(())
}

fn finite(values: &[f32]) -> Result<(), RebuildError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(invalid("non-finite value"))
    }
}

fn names_of(t: &Tables, reg: &str) -> Vec<String> {
    t.registries
        .iter()
        .find(|r| t.strings[r.name as usize] == reg)
        .map(|r| {
            r.entries
                .iter()
                .map(|&e| t.strings[e as usize].clone())
                .collect()
        })
        .unwrap_or_default()
}

fn replace_registries(t: &mut Tables, export: &Export) -> Result<(), RebuildError> {
    if export.registries.len() > MAX_REGISTRIES {
        return Err(RebuildError::TooMany("registries"));
    }
    let mut strings = Strings::new(&mut t.strings);
    for (name, entries) in &export.registries {
        bounded_name(name)?;
        if entries.len() > MAX_ENTRIES {
            return Err(RebuildError::TooMany("registry entries"));
        }
        let name_id = strings.intern(name)?;
        let pos = t.registries.iter().position(|r| r.name == name_id);
        let have = pos.map_or(0, |p| t.registries[p].entries.len());
        if entries.len() < have
            || pos.is_some_and(|p| {
                t.registries[p]
                    .entries
                    .iter()
                    .zip(entries)
                    .any(|(&id, e)| strings.strings[id as usize] != *e)
            })
        {
            return Err(RebuildError::Reordered(name.clone()));
        }
        let mut ids = pos.map_or_else(Vec::new, |p| t.registries[p].entries.clone());
        for e in &entries[have..] {
            bounded_name(e)?;
            ids.push(strings.intern(e)?);
        }
        let mut sorted: Vec<u32> = (0..ids.len() as u32).collect();
        sorted.sort_by(|&a, &b| {
            strings.strings[ids[a as usize] as usize]
                .cmp(&strings.strings[ids[b as usize] as usize])
        });
        if sorted
            .windows(2)
            .any(|w| ids[w[0] as usize] == ids[w[1] as usize])
        {
            return Err(invalid(format!("registry {name} repeats an entry")));
        }
        let registry = Registry {
            name: name_id,
            entries: ids,
            sorted,
        };
        match pos {
            Some(p) => t.registries[p] = registry,
            None => t.registries.push(registry),
        }
    }
    Ok(())
}

fn shapes(t: &mut Tables, export: &Export) -> Result<(), RebuildError> {
    if export.shapes.len() > MAX_SHAPES {
        return Err(RebuildError::TooMany("shapes"));
    }
    t.shape_starts = vec![0];
    t.shape_boxes = Vec::new();
    for shape in &export.shapes {
        if shape.len() > MAX_SHAPE_BOXES {
            return Err(RebuildError::TooMany("boxes in a shape"));
        }
        if shape
            .iter()
            .any(|b| b.min.iter().chain(&b.max).any(|v| !v.is_finite()))
        {
            return Err(invalid("non-finite shape"));
        }
        t.shape_boxes.extend_from_slice(shape);
        t.shape_starts.push(t.shape_boxes.len() as u32);
    }
    Ok(())
}

fn fluid_codes(export: &Export, fluid_names: &[String]) -> Result<Vec<u16>, RebuildError> {
    if fluid_names.len() > fluid_bits::ID_MASK as usize + 1 {
        return Err(RebuildError::TooMany("fluids"));
    }
    export
        .fluid_states
        .iter()
        .map(|f| {
            let id = fluid_names
                .iter()
                .position(|n| *n == f.fluid)
                .ok_or_else(|| invalid(format!("unknown fluid {}", f.fluid)))?;
            Ok(fluid_bits::PRESENT
                | id as u16
                | u16::from(f.amount.min(15)) << fluid_bits::AMOUNT_SHIFT
                | if f.source { fluid_bits::SOURCE } else { 0 }
                | if f.falling { fluid_bits::FALLING } else { 0 })
        })
        .collect()
}

fn blocks(
    t: &mut Tables,
    export: &Export,
    fluids: &[u16],
    strings: &mut Strings<'_>,
) -> Result<(), RebuildError> {
    let shape_index = |i: u32| -> Result<u16, RebuildError> {
        if (i as usize) < export.shapes.len() {
            Ok(i as u16)
        } else {
            Err(invalid("shape index out of range"))
        }
    };
    let mut b = BlockTable {
        first_state: vec![0],
        prop_start: vec![0],
        ..BlockTable::default()
    };
    let mut st = StateTable::default();
    for (bid, block) in export.blocks.iter().enumerate() {
        finite(&[
            block.friction,
            block.speed_factor,
            block.jump_factor,
            block.destroy_time,
            block.explosion_resistance,
        ])?;
        if block.properties.len() > MAX_PROPERTIES {
            return Err(RebuildError::TooMany("properties"));
        }
        let mut expected: usize = 1;
        let mut last: Option<&str> = None;
        for (name, values) in &block.properties {
            bounded_name(name)?;
            if last.is_some_and(|l| l >= name.as_str()) {
                return Err(invalid("block properties not sorted by name"));
            }
            last = Some(name);
            if values.is_empty() || values.len() > MAX_VALUES {
                return Err(invalid("property without values"));
            }
            expected = expected.saturating_mul(values.len());
        }
        if expected != block.states.len() || block.states.is_empty() {
            return Err(invalid(
                "state count is not the product of the property sizes",
            ));
        }
        if block.default_state as usize >= block.states.len() {
            return Err(invalid("default state out of range"));
        }
        let first = *b.first_state.last().unwrap_or(&0);
        if first as usize + block.states.len() > MAX_BLOCK_STATES {
            return Err(RebuildError::TooMany("block states"));
        }
        let bid = u16::try_from(bid).map_err(|_| RebuildError::TooMany("blocks"))?;
        b.friction.push(block.friction);
        b.speed_factor.push(block.speed_factor);
        b.jump_factor.push(block.jump_factor);
        b.destroy_time.push(block.destroy_time);
        b.requires_tool.push(u8::from(block.requires_tool));
        b.explosion_resistance.push(block.explosion_resistance);
        b.sound_type.push(strings.intern(&block.sound_type)?);
        for (name, values) in &block.properties {
            let value_start = b.prop_values.len() as u32;
            for v in values {
                bounded_name(v)?;
                b.prop_values.push(strings.intern(v)?);
            }
            b.props.push(PropDef {
                name: strings.intern(name)?,
                value_start,
                value_count: values.len() as u32,
            });
        }
        b.prop_start.push(b.props.len() as u32);
        b.default_state.push(first + block.default_state);
        b.first_state.push(first + block.states.len() as u32);
        for s in &block.states {
            st.block.push(bid);
            st.flags.push(s.flags & 0x3ff);
            st.light_emission.push(s.light_emission.min(15));
            st.light_block.push(s.light_block.min(15));
            st.collision.push(shape_index(s.collision)?);
            st.outline.push(shape_index(s.outline)?);
            st.occlusion.push(shape_index(s.occlusion)?);
            st.map_color.push(s.map_color);
            st.sturdy_faces.push(s.sturdy_faces & 0x3f);
            st.occlusion_faces.push(s.occlusion_faces & 0x3f);
            st.fluid.push(match s.fluid {
                None => 0,
                Some(i) => *fluids
                    .get(i as usize)
                    .ok_or_else(|| invalid("fluid state index out of range"))?,
            });
        }
    }
    t.blocks = b;
    t.states = st;
    Ok(())
}

fn items(
    t: &mut Tables,
    export: &Export,
    block_names: &[String],
    strings: &mut Strings<'_>,
) -> Result<(), RebuildError> {
    let block_ids: HashMap<&str, i32> = block_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i as i32))
        .collect();
    let mut it = ItemTable {
        tool_start: vec![0],
        ..ItemTable::default()
    };
    for item in &export.items {
        it.max_stack_size.push(item.max_stack_size.clamp(1, 99));
        it.max_damage.push(item.max_damage);
        it.block.push(match &item.block {
            None => -1,
            Some(name) => *block_ids
                .get(name.as_str())
                .ok_or_else(|| invalid(format!("item names unknown block {name}")))?,
        });
        let (mut default_speed, mut damage, mut tool_flags) = (-1.0f32, 0u8, 0u8);
        if let Some(tool) = &item.tool {
            finite(&[tool.default_mining_speed])?;
            if tool.rules.len() > MAX_TOOL_RULES {
                return Err(RebuildError::TooMany("tool rules"));
            }
            default_speed = tool.default_mining_speed;
            damage = tool.damage_per_block;
            tool_flags = 1 | if tool.can_destroy_blocks_in_creative {
                2
            } else {
                0
            };
            for (blocks, speed, correct) in &tool.rules {
                it.tool_blocks.push(strings.intern(blocks)?);
                it.tool_speed
                    .push(speed.filter(|s| s.is_finite() && *s >= 0.0).unwrap_or(-1.0));
                it.tool_correct.push(match correct {
                    None => 0,
                    Some(false) => 1,
                    Some(true) => 2,
                });
            }
        }
        it.tool_start.push(it.tool_blocks.len() as u32);
        it.tool_default_speed.push(default_speed);
        it.tool_damage.push(damage);
        it.tool_flags.push(tool_flags);
        it.equip_slot.push(if item.equip_slot < 8 {
            item.equip_slot
        } else {
            255
        });
        let seconds = |v: Option<f32>| v.filter(|s| s.is_finite() && *s >= 0.0).unwrap_or(-1.0);
        it.consume_seconds.push(seconds(item.consume_seconds));
        it.cooldown_seconds.push(seconds(item.cooldown_seconds));
        it.food_flags.push(match item.food {
            None => 0,
            Some(always) => 1 | if always { 2 } else { 0 },
        });
        let mut words = [0u32; 4];
        for &c in item.default_components.iter().filter(|&&c| c < 128) {
            words[c as usize / 32] |= 1 << (c % 32);
        }
        it.default_components.extend_from_slice(&words);
    }
    t.items = it;
    Ok(())
}

/// `base` with its registries, blocks, items, entity types and fluids replaced by `export`'s.
pub fn rebuild(base: &Tables, export: &Export) -> Result<Tables, RebuildError> {
    let mut t = base.clone();
    replace_registries(&mut t, export)?;
    for (reg, label, described) in [
        ("minecraft:block", "blocks", export.blocks.len()),
        ("minecraft:item", "items", export.items.len()),
        (
            "minecraft:entity_type",
            "entity types",
            export.entity_types.len(),
        ),
        ("minecraft:fluid", "fluids", export.fluids.len()),
    ] {
        let listed = names_of(&t, reg).len();
        if listed != described {
            return Err(RebuildError::Count(label, listed, described));
        }
    }
    let fluid_names = names_of(&t, "minecraft:fluid");
    let block_names = names_of(&t, "minecraft:block");
    let fluids = fluid_codes(export, &fluid_names)?;
    shapes(&mut t, export)?;
    let mut strings_vec = std::mem::take(&mut t.strings);
    let mut strings = Strings::new(&mut strings_vec);
    blocks(&mut t, export, &fluids, &mut strings)?;
    items(&mut t, export, &block_names, &mut strings)?;
    let mut entities = EntityTable::default();
    for e in &export.entity_types {
        finite(&[e.width, e.height, e.eye_height])?;
        entities.width.push(e.width.clamp(0.0, 256.0));
        entities.height.push(e.height.clamp(0.0, 256.0));
        entities.eye_height.push(e.eye_height.clamp(0.0, 256.0));
        entities.category.push(strings.intern(&e.category)?);
    }
    t.entities = entities;
    t.fluid_source = export.fluids.iter().map(|f| u8::from(f.source)).collect();
    t.strings = strings_vec;
    Ok(t)
}

/// What `tables` would export: the inverse of [`rebuild`] for the parts it replaces.
pub fn export_of(tables: &Tables) -> Export {
    let t = tables;
    let s = |i: u32| t.strings[i as usize].clone();
    let registries = t
        .registries
        .iter()
        .map(|r| (s(r.name), r.entries.iter().map(|&e| s(e)).collect()))
        .collect();
    let shapes = (0..t.shape_starts.len().saturating_sub(1))
        .map(|i| t.shape_boxes[t.shape_starts[i] as usize..t.shape_starts[i + 1] as usize].to_vec())
        .collect();
    let fluid_names = names_of(t, "minecraft:fluid");
    let block_names = names_of(t, "minecraft:block");
    let mut fluid_states: Vec<ExportFluidState> = Vec::new();
    let mut fluid_index: HashMap<u16, u32> = HashMap::new();
    let b = &t.blocks;
    let blocks = (0..b.default_state.len())
        .map(|i| {
            let (first, end) = (b.first_state[i], b.first_state[i + 1]);
            let properties = (b.prop_start[i]..b.prop_start[i + 1])
                .map(|p| {
                    let def = &b.props[p as usize];
                    let values = (def.value_start..def.value_start + def.value_count)
                        .map(|v| s(b.prop_values[v as usize]))
                        .collect();
                    (s(def.name), values)
                })
                .collect();
            let states = (first..end)
                .map(|id| {
                    let id = id as usize;
                    let st = &t.states;
                    let code = st.fluid[id];
                    let fluid = (code & fluid_bits::PRESENT != 0).then(|| {
                        *fluid_index.entry(code).or_insert_with(|| {
                            fluid_states.push(ExportFluidState {
                                fluid: fluid_names[(code & fluid_bits::ID_MASK) as usize].clone(),
                                amount: (code >> fluid_bits::AMOUNT_SHIFT & fluid_bits::AMOUNT_MASK)
                                    as u8,
                                source: code & fluid_bits::SOURCE != 0,
                                falling: code & fluid_bits::FALLING != 0,
                            });
                            fluid_states.len() as u32 - 1
                        })
                    });
                    ExportState {
                        collision: u32::from(st.collision[id]),
                        outline: u32::from(st.outline[id]),
                        occlusion: u32::from(st.occlusion[id]),
                        light_emission: st.light_emission[id],
                        light_block: st.light_block[id],
                        flags: st.flags[id],
                        map_color: st.map_color[id],
                        sturdy_faces: st.sturdy_faces[id],
                        occlusion_faces: st.occlusion_faces[id],
                        fluid,
                    }
                })
                .collect();
            ExportBlock {
                friction: b.friction[i],
                speed_factor: b.speed_factor[i],
                jump_factor: b.jump_factor[i],
                destroy_time: b.destroy_time[i],
                requires_tool: b.requires_tool[i] != 0,
                explosion_resistance: b.explosion_resistance[i],
                sound_type: s(b.sound_type[i]),
                properties,
                default_state: b.default_state[i] - first,
                states,
            }
        })
        .collect();
    let it = &t.items;
    let items = (0..it.max_stack_size.len())
        .map(|i| {
            let rules: Vec<_> = (it.tool_start[i]..it.tool_start[i + 1])
                .map(|r| {
                    let r = r as usize;
                    let speed = it.tool_speed[r];
                    let correct = match it.tool_correct[r] {
                        1 => Some(false),
                        2 => Some(true),
                        _ => None,
                    };
                    (
                        s(it.tool_blocks[r]),
                        (speed >= 0.0).then_some(speed),
                        correct,
                    )
                })
                .collect();
            let tool = (it.tool_flags[i] & 1 != 0).then(|| ExportTool {
                default_mining_speed: it.tool_default_speed[i],
                damage_per_block: it.tool_damage[i],
                can_destroy_blocks_in_creative: it.tool_flags[i] & 2 != 0,
                rules,
            });
            let seconds = |v: f32| (v >= 0.0).then_some(v);
            let words = &it.default_components[i * 4..i * 4 + 4];
            ExportItem {
                max_stack_size: it.max_stack_size[i],
                max_damage: it.max_damage[i],
                block: usize::try_from(it.block[i])
                    .ok()
                    .map(|b| block_names[b].clone()),
                tool,
                equip_slot: it.equip_slot[i],
                consume_seconds: seconds(it.consume_seconds[i]),
                cooldown_seconds: seconds(it.cooldown_seconds[i]),
                food: (it.food_flags[i] & 1 != 0).then_some(it.food_flags[i] & 2 != 0),
                default_components: (0..128u32)
                    .filter(|&c| words[c as usize / 32] >> (c % 32) & 1 != 0)
                    .collect(),
            }
        })
        .collect();
    let e = &t.entities;
    let entity_types = (0..e.width.len())
        .map(|i| ExportEntityType {
            width: e.width[i],
            height: e.height[i],
            eye_height: e.eye_height[i],
            category: s(e.category[i]),
        })
        .collect();
    let fluids = t
        .fluid_source
        .iter()
        .map(|&f| ExportFluid { source: f != 0 })
        .collect();
    Export {
        registries,
        blocks,
        items,
        entity_types,
        fluids,
        shapes,
        fluid_states,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Version, vanilla_26_3};

    fn sample() -> Export {
        let mut e = vanilla_26_3().export();
        let push = |e: &mut Export, reg: &str, name: &str| {
            e.registries
                .iter_mut()
                .find(|(n, _)| n == reg)
                .unwrap()
                .1
                .push(name.into());
        };
        push(&mut e, "minecraft:block", "test:lamp");
        push(&mut e, "minecraft:item", "test:lamp");
        push(&mut e, "minecraft:entity_type", "test:mob");
        push(&mut e, "minecraft:sound_event", "test:ding");
        e.registries
            .push(("test:custom".into(), vec!["test:a".into()]));
        let state = |light| ExportState {
            light_emission: light,
            flags: crate::flag::CAN_OCCLUDE | crate::flag::SOLID_RENDER,
            sturdy_faces: 0x3f,
            ..ExportState::default()
        };
        e.blocks.push(ExportBlock {
            friction: 0.6,
            speed_factor: 1.0,
            jump_factor: 1.0,
            destroy_time: 0.3,
            sound_type: "minecraft:block.glass.break".into(),
            properties: vec![("lit".into(), vec!["true".into(), "false".into()])],
            default_state: 1,
            states: vec![state(15), state(0)],
            ..ExportBlock::default()
        });
        e.items.push(ExportItem {
            max_stack_size: 64,
            block: Some("test:lamp".into()),
            equip_slot: 255,
            ..ExportItem::default()
        });
        e.entity_types.push(ExportEntityType {
            width: 0.6,
            height: 1.8,
            eye_height: 1.62,
            category: "monster".into(),
        });
        e
    }

    #[test]
    fn vanilla_round_trips_through_its_export() {
        let base = vanilla_26_3();
        let v = base.rebuild(&base.export()).unwrap();
        assert_eq!(v.export(), base.export());
        assert_eq!(v.state_count(), base.state_count());
    }

    #[test]
    fn appends_a_modded_block() {
        let base = vanilla_26_3();
        let v = base.rebuild(&sample()).unwrap();
        assert_eq!(v.state_count(), base.state_count() + 2);
        let lamp = v.block_by_name("test:lamp").unwrap();
        assert_eq!(lamp as usize, base.block_count());
        let info = v.block_info(lamp).unwrap();
        assert_eq!(info.states.0 as usize, base.state_count());
        assert_eq!(info.default_state, info.states.0 + 1);
        assert_eq!(v.light_emission(info.states.0), 15);
        assert_eq!(v.state_block_name(info.states.0), Some("test:lamp"));
        let props: Vec<_> = v.state_properties(info.states.0 + 1).collect();
        assert_eq!(props, [("lit", "false")]);
        let item = v.item_by_name("test:lamp").unwrap();
        assert_eq!(v.item_info(item).unwrap().block, Some(lamp));
        let mob = v.entity_type_by_name("test:mob").unwrap();
        assert_eq!(v.entity_type_info(mob).unwrap().category, "monster");
        assert!(v.registry("test:custom").is_some());
        let stone = base.block_by_name("minecraft:stone").unwrap();
        assert_eq!(v.block_info(stone), base.block_info(stone));
        let sounds = v.registry("minecraft:sound_event").unwrap();
        assert_eq!(sounds.id_of("test:ding"), Some(sounds.len() as u32 - 1));
    }

    #[test]
    fn a_property_added_to_a_vanilla_block_moves_later_states() {
        let base = vanilla_26_3();
        let mut e = base.export();
        let stone = base.block_by_name("minecraft:stone").unwrap() as usize;
        let s = e.blocks[stone].states[0];
        e.blocks[stone].properties = vec![("wet".into(), vec!["false".into(), "true".into()])];
        e.blocks[stone].states = vec![s, s];
        let v = base.rebuild(&e).unwrap();
        assert_eq!(v.state_count(), base.state_count() + 1);
        let dirt = v.block_by_name("minecraft:dirt").unwrap();
        assert_eq!(
            v.block_info(dirt).unwrap().default_state,
            base.block_info(dirt).unwrap().default_state + 1
        );
        let wet: Vec<_> = v
            .state_properties(v.block_info(stone as u32).unwrap().states.0 + 1)
            .collect();
        assert_eq!(wet, [("wet", "true")]);
    }

    #[test]
    fn refuses_reordered_or_inconsistent_exports() {
        let base = vanilla_26_3();
        let mut e = sample();
        e.registries
            .iter_mut()
            .find(|(n, _)| n == "minecraft:block")
            .unwrap()
            .1
            .swap(0, 1);
        assert!(matches!(base.rebuild(&e), Err(RebuildError::Reordered(_))));
        let mut e = sample();
        e.blocks.pop();
        assert!(matches!(
            base.rebuild(&e),
            Err(RebuildError::Count("blocks", _, _))
        ));
        let mut e = sample();
        e.blocks.last_mut().unwrap().states.pop();
        assert!(matches!(base.rebuild(&e), Err(RebuildError::Invalid(_))));
        let mut e = sample();
        e.blocks.last_mut().unwrap().default_state = 9;
        assert!(base.rebuild(&e).is_err());
        let mut e = sample();
        e.shapes[1][0].max[1] = f32::NAN;
        assert!(base.rebuild(&e).is_err());
        let mut e = sample();
        e.blocks.last_mut().unwrap().states[0].collision = e.shapes.len() as u32;
        assert!(base.rebuild(&e).is_err());
        let mut e = sample();
        e.blocks.last_mut().unwrap().states[0].fluid = Some(e.fluid_states.len() as u32);
        assert!(base.rebuild(&e).is_err());
        let mut e = sample();
        e.items.last_mut().unwrap().block = Some("test:nothing".into());
        assert!(base.rebuild(&e).is_err());
    }

    #[test]
    fn hostile_sizes_are_bounded() {
        let base = vanilla_26_3();
        let mut e = sample();
        e.blocks.last_mut().unwrap().properties = (0..40)
            .map(|i| (format!("p{i:02}"), vec!["a".into(), "b".into()]))
            .collect();
        assert_eq!(
            base.rebuild(&e).unwrap_err(),
            RebuildError::TooMany("properties")
        );
        let mut e = sample();
        e.shapes.push(vec![Aabb::default(); MAX_SHAPE_BOXES + 1]);
        assert_eq!(
            base.rebuild(&e).unwrap_err(),
            RebuildError::TooMany("boxes in a shape")
        );
        let mut e = sample();
        e.registries[0].1.push("x".repeat(MAX_NAME + 1));
        assert!(base.rebuild(&e).is_err());
        let mut e = sample();
        let lamp = e.blocks.last_mut().unwrap();
        lamp.properties = vec![
            ("a".into(), (0..1024).map(|i| i.to_string()).collect()),
            ("b".into(), (0..1024).map(|i| i.to_string()).collect()),
        ];
        lamp.states = vec![ExportState::default(); 1 << 20];
        assert_eq!(
            base.rebuild(&e).unwrap_err(),
            RebuildError::TooMany("block states")
        );
    }

    #[test]
    fn install_and_activate() {
        let base = vanilla_26_3();
        let v: &'static Version = crate::install(base.rebuild(&sample()).unwrap()).unwrap();
        assert!(std::ptr::eq(crate::v26_3(), base));
        crate::activate(v);
        assert!(crate::v26_3().block_by_name("test:lamp").is_some());
        crate::activate(base);
        assert!(std::ptr::eq(crate::v26_3(), base));
    }
}
