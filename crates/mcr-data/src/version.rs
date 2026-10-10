use std::ops::Range;

use crate::tables::{Aabb, DIRECTION_NAMES, DecodeError, STATE_NAMES, Tables, flag, fluid_bits};

pub type StateId = u32;
pub type BlockId = u32;
pub type ItemId = u32;
pub type EntityTypeId = u32;
pub type FluidId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProtocolState {
    Handshake,
    Status,
    Login,
    Configuration,
    Play,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Clientbound,
    Serverbound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FluidStateInfo {
    pub fluid: FluidId,
    pub amount: u8,
    pub source: bool,
    pub falling: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockStateInfo {
    pub block: BlockId,
    pub light_emission: u8,
    pub light_block: u8,
    pub map_color: u8,
    pub sturdy_faces: u8,
    pub occlusion_faces: u8,
    pub fluid: Option<FluidStateInfo>,
    flags: u16,
}

impl BlockStateInfo {
    pub fn is_air(&self) -> bool {
        self.flags & flag::AIR != 0
    }

    pub fn is_liquid(&self) -> bool {
        self.flags & flag::LIQUID != 0
    }

    pub fn is_replaceable(&self) -> bool {
        self.flags & flag::REPLACEABLE != 0
    }

    pub fn has_block_entity(&self) -> bool {
        self.flags & flag::HAS_BLOCK_ENTITY != 0
    }

    pub fn can_occlude(&self) -> bool {
        self.flags & flag::CAN_OCCLUDE != 0
    }

    pub fn use_shape_for_light_occlusion(&self) -> bool {
        self.flags & flag::USE_SHAPE_FOR_LIGHT_OCCLUSION != 0
    }

    pub fn is_solid_render(&self) -> bool {
        self.flags & flag::SOLID_RENDER != 0
    }

    pub fn is_full_collision(&self) -> bool {
        self.flags & flag::FULL_COLLISION != 0
    }

    pub fn propagates_skylight_down(&self) -> bool {
        self.flags & flag::PROPAGATES_SKYLIGHT_DOWN != 0
    }

    pub fn is_redstone_conductor(&self) -> bool {
        self.flags & flag::REDSTONE_CONDUCTOR != 0
    }

    /// Face order follows `Direction.ordinal()`: down, up, north, south, west, east.
    pub fn is_face_sturdy(&self, face: usize) -> bool {
        self.sturdy_faces >> face & 1 != 0
    }

    pub fn is_face_fully_occluding(&self, face: usize) -> bool {
        self.occlusion_faces >> face & 1 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockInfo<'a> {
    pub default_state: StateId,
    pub states: (StateId, StateId),
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    pub destroy_time: f32,
    pub requires_correct_tool: bool,
    pub explosion_resistance: f32,
    pub sound_type: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemInfo {
    pub max_stack_size: u16,
    pub max_damage: u32,
    pub block: Option<BlockId>,
}

/// Default `tool` component of an item; `blocks` is a block name or `#tag`.
#[derive(Debug, Clone, Copy)]
pub struct ToolInfo<'a> {
    pub default_mining_speed: f32,
    pub damage_per_block: u8,
    pub can_destroy_blocks_in_creative: bool,
    owner: &'a Version,
    rules: (usize, usize),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolRuleInfo<'a> {
    pub blocks: &'a str,
    pub speed: Option<f32>,
    pub correct_for_drops: Option<bool>,
}

impl<'a> ToolInfo<'a> {
    pub fn rules(&self) -> impl Iterator<Item = ToolRuleInfo<'a>> + 'a {
        let owner = self.owner;
        let (start, end) = self.rules;
        (start..end).map(move |i| {
            let t = &owner.tables.items;
            ToolRuleInfo {
                blocks: owner.string(t.tool_blocks[i]),
                speed: Some(t.tool_speed[i]).filter(|v| *v >= 0.0),
                correct_for_drops: match t.tool_correct[i] {
                    0 => None,
                    n => Some(n == 2),
                },
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntityTypeInfo<'a> {
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
    pub category: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct RegistryView<'a> {
    version: &'a Version,
    index: usize,
}

impl<'a> RegistryView<'a> {
    pub fn len(&self) -> usize {
        self.version.tables.registries[self.index].entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn name_of(&self, id: u32) -> Option<&'a str> {
        let entry = *self.version.tables.registries[self.index]
            .entries
            .get(id as usize)?;
        Some(self.version.string(entry))
    }

    pub fn id_of(&self, name: &str) -> Option<u32> {
        let reg = &self.version.tables.registries[self.index];
        let pos = reg
            .sorted
            .binary_search_by(|&id| self.version.string(reg.entries[id as usize]).cmp(name))
            .ok()?;
        Some(reg.sorted[pos])
    }

    pub fn iter(&self) -> impl Iterator<Item = (u32, &'a str)> + '_ {
        let version = self.version;
        version.tables.registries[self.index]
            .entries
            .iter()
            .enumerate()
            .map(move |(id, &s)| (id as u32, version.string(s)))
    }
}

#[derive(Debug)]
pub struct StateProperties<'a> {
    version: &'a Version,
    props: Range<usize>,
    ordinal: u32,
    stride: u32,
}

impl<'a> Iterator for StateProperties<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        let index = self.props.next()?;
        let t = &self.version.tables;
        let def = &t.blocks.props[index];
        self.stride /= def.value_count;
        let value = self.ordinal / self.stride % def.value_count;
        let value_str = t.blocks.prop_values[(def.value_start + value) as usize];
        Some((
            self.version.string(def.name),
            self.version.string(value_str),
        ))
    }
}

#[derive(Debug)]
pub struct Version {
    tables: Tables,
    block: usize,
    item: usize,
    entity_type: usize,
    fluid: usize,
}

impl Version {
    pub fn from_blob(blob: &[u8]) -> Result<Self, DecodeError> {
        let tables = Tables::decode(blob)?;
        let find = |name: &str| {
            tables
                .registries
                .iter()
                .position(|r| tables.strings[r.name as usize] == name)
                .ok_or(DecodeError::Truncated)
        };
        Ok(Self {
            block: find("minecraft:block")?,
            item: find("minecraft:item")?,
            entity_type: find("minecraft:entity_type")?,
            fluid: find("minecraft:fluid")?,
            tables,
        })
    }

    fn string(&self, index: u32) -> &str {
        &self.tables.strings[index as usize]
    }

    fn registry_at(&self, index: usize) -> RegistryView<'_> {
        RegistryView {
            version: self,
            index,
        }
    }

    pub fn name(&self) -> &str {
        &self.tables.version_name
    }

    pub fn protocol(&self) -> u32 {
        self.tables.protocol
    }

    pub fn state_count(&self) -> usize {
        self.tables.states.block.len()
    }

    pub fn block_count(&self) -> usize {
        self.tables.blocks.default_state.len()
    }

    pub fn registry(&self, name: &str) -> Option<RegistryView<'_>> {
        let index = self
            .tables
            .registries
            .iter()
            .position(|r| self.string(r.name) == name)?;
        Some(self.registry_at(index))
    }

    pub fn blocks(&self) -> RegistryView<'_> {
        self.registry_at(self.block)
    }

    pub fn items(&self) -> RegistryView<'_> {
        self.registry_at(self.item)
    }

    pub fn entity_types(&self) -> RegistryView<'_> {
        self.registry_at(self.entity_type)
    }

    pub fn fluids(&self) -> RegistryView<'_> {
        self.registry_at(self.fluid)
    }

    pub fn block_state(&self, id: StateId) -> Option<BlockStateInfo> {
        let s = &self.tables.states;
        let i = id as usize;
        let fluid = s.fluid.get(i).copied()?;
        let fluid = (fluid & fluid_bits::PRESENT != 0).then(|| FluidStateInfo {
            fluid: u32::from(fluid & fluid_bits::ID_MASK),
            amount: ((fluid >> fluid_bits::AMOUNT_SHIFT) & fluid_bits::AMOUNT_MASK) as u8,
            source: fluid & fluid_bits::SOURCE != 0,
            falling: fluid & fluid_bits::FALLING != 0,
        });
        Some(BlockStateInfo {
            block: u32::from(s.block[i]),
            light_emission: s.light_emission[i],
            light_block: s.light_block[i],
            map_color: s.map_color[i],
            sturdy_faces: s.sturdy_faces[i],
            occlusion_faces: s.occlusion_faces[i],
            fluid,
            flags: s.flags[i],
        })
    }

    pub fn block_by_name(&self, name: &str) -> Option<BlockId> {
        self.blocks().id_of(name)
    }

    pub fn block_name(&self, block: BlockId) -> Option<&str> {
        self.blocks().name_of(block)
    }

    pub fn block_info(&self, block: BlockId) -> Option<BlockInfo<'_>> {
        let b = &self.tables.blocks;
        let i = block as usize;
        Some(BlockInfo {
            default_state: *b.default_state.get(i)?,
            states: (b.first_state[i], b.first_state[i + 1]),
            friction: b.friction[i],
            speed_factor: b.speed_factor[i],
            jump_factor: b.jump_factor[i],
            destroy_time: b.destroy_time[i],
            requires_correct_tool: b.requires_tool[i] != 0,
            explosion_resistance: b.explosion_resistance[i],
            sound_type: self.string(b.sound_type[i]),
        })
    }

    pub fn state_block_name(&self, id: StateId) -> Option<&str> {
        let block = *self.tables.states.block.get(id as usize)?;
        self.block_name(u32::from(block))
    }

    pub fn state_properties(&self, id: StateId) -> StateProperties<'_> {
        let b = &self.tables.blocks;
        let Some(&block) = self.tables.states.block.get(id as usize) else {
            return StateProperties {
                version: self,
                props: 0..0,
                ordinal: 0,
                stride: 1,
            };
        };
        let block = block as usize;
        let props = b.prop_start[block] as usize..b.prop_start[block + 1] as usize;
        let stride = b.props[props.clone()]
            .iter()
            .map(|p| p.value_count)
            .product();
        StateProperties {
            version: self,
            props,
            ordinal: id - b.first_state[block],
            stride,
        }
    }

    fn shape(&self, index: u16) -> &[Aabb] {
        let starts = &self.tables.shape_starts;
        let i = index as usize;
        &self.tables.shape_boxes[starts[i] as usize..starts[i + 1] as usize]
    }

    pub fn collision_shape(&self, id: StateId) -> &[Aabb] {
        self.tables
            .states
            .collision
            .get(id as usize)
            .map_or(&[], |&s| self.shape(s))
    }

    pub fn outline_shape(&self, id: StateId) -> &[Aabb] {
        self.tables
            .states
            .outline
            .get(id as usize)
            .map_or(&[], |&s| self.shape(s))
    }

    pub fn occlusion_shape(&self, id: StateId) -> &[Aabb] {
        self.tables
            .states
            .occlusion
            .get(id as usize)
            .map_or(&[], |&s| self.shape(s))
    }

    pub fn collision_shape_index(&self, id: StateId) -> Option<u16> {
        self.tables.states.collision.get(id as usize).copied()
    }

    pub fn light_emission(&self, id: StateId) -> u8 {
        self.tables
            .states
            .light_emission
            .get(id as usize)
            .copied()
            .unwrap_or(0)
    }

    pub fn light_block(&self, id: StateId) -> u8 {
        self.tables
            .states
            .light_block
            .get(id as usize)
            .copied()
            .unwrap_or(0)
    }

    pub fn item_by_name(&self, name: &str) -> Option<ItemId> {
        self.items().id_of(name)
    }

    pub fn item_name(&self, item: ItemId) -> Option<&str> {
        self.items().name_of(item)
    }

    pub fn item_info(&self, item: ItemId) -> Option<ItemInfo> {
        let t = &self.tables.items;
        let i = item as usize;
        Some(ItemInfo {
            max_stack_size: *t.max_stack_size.get(i)?,
            max_damage: t.max_damage[i],
            block: u32::try_from(t.block[i]).ok(),
        })
    }

    pub fn item_tool(&self, item: ItemId) -> Option<ToolInfo<'_>> {
        let t = &self.tables.items;
        let i = item as usize;
        let flags = *t.tool_flags.get(i)?;
        (flags & 1 != 0).then(|| ToolInfo {
            default_mining_speed: t.tool_default_speed[i],
            damage_per_block: t.tool_damage[i],
            can_destroy_blocks_in_creative: flags & 2 != 0,
            owner: self,
            rules: (t.tool_start[i] as usize, t.tool_start[i + 1] as usize),
        })
    }

    /// Slot the item equips into by default: head, chest, legs, feet, body, saddle, mainhand, offhand.
    pub fn item_equip_slot(&self, item: ItemId) -> Option<&'static str> {
        const NAMES: [&str; 8] = [
            "head", "chest", "legs", "feet", "body", "saddle", "mainhand", "offhand",
        ];
        let slot = *self.tables.items.equip_slot.get(item as usize)?;
        NAMES.get(usize::from(slot)).copied()
    }

    /// `Some(can_always_eat)` for items with the `food` component.
    pub fn item_food(&self, item: ItemId) -> Option<bool> {
        let f = *self.tables.items.food_flags.get(item as usize)?;
        (f & 1 != 0).then_some(f & 2 != 0)
    }

    pub fn item_consume_seconds(&self, item: ItemId) -> Option<f32> {
        Some(*self.tables.items.consume_seconds.get(item as usize)?).filter(|v| *v >= 0.0)
    }

    pub fn item_use_cooldown_seconds(&self, item: ItemId) -> Option<f32> {
        Some(*self.tables.items.cooldown_seconds.get(item as usize)?).filter(|v| *v >= 0.0)
    }

    pub fn entity_type_by_name(&self, name: &str) -> Option<EntityTypeId> {
        self.entity_types().id_of(name)
    }

    pub fn entity_type_name(&self, id: EntityTypeId) -> Option<&str> {
        self.entity_types().name_of(id)
    }

    pub fn entity_type_info(&self, id: EntityTypeId) -> Option<EntityTypeInfo<'_>> {
        let t = &self.tables.entities;
        let i = id as usize;
        Some(EntityTypeInfo {
            width: *t.width.get(i)?,
            height: t.height[i],
            eye_height: t.eye_height[i],
            category: self.string(t.category[i]),
        })
    }

    pub fn fluid_by_name(&self, name: &str) -> Option<FluidId> {
        self.fluids().id_of(name)
    }

    pub fn fluid_name(&self, id: FluidId) -> Option<&str> {
        self.fluids().name_of(id)
    }

    pub fn fluid_is_source(&self, id: FluidId) -> bool {
        self.tables
            .fluid_source
            .get(id as usize)
            .is_some_and(|&b| b != 0)
    }

    fn packet_key(&self, state: ProtocolState, direction: Direction) -> (u8, u8) {
        (state as u8, direction as u8)
    }

    pub fn packet_id(&self, state: ProtocolState, direction: Direction, name: &str) -> Option<u32> {
        let key = self.packet_key(state, direction);
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        self.tables
            .packets
            .iter()
            .find(|p| {
                (p.state, p.direction) == key && {
                    let n = self.string(p.name);
                    n.strip_prefix("minecraft:").unwrap_or(n) == name
                }
            })
            .map(|p| p.id)
    }

    pub fn packet_name(&self, state: ProtocolState, direction: Direction, id: u32) -> Option<&str> {
        let key = self.packet_key(state, direction);
        self.tables
            .packets
            .iter()
            .find(|p| (p.state, p.direction) == key && p.id == id)
            .map(|p| self.string(p.name))
    }

    pub fn protocol_state_name(state: ProtocolState) -> &'static str {
        STATE_NAMES[state as usize]
    }

    pub fn direction_name(direction: Direction) -> &'static str {
        DIRECTION_NAMES[direction as usize]
    }
}
