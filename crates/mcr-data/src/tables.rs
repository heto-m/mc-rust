const MAGIC: &[u8; 4] = b"MCRD";
const FORMAT: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Registry {
    pub name: u32,
    pub entries: Vec<u32>,
    pub sorted: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PropDef {
    pub name: u32,
    pub value_start: u32,
    pub value_count: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockTable {
    pub first_state: Vec<u32>,
    pub default_state: Vec<u32>,
    pub friction: Vec<f32>,
    pub speed_factor: Vec<f32>,
    pub jump_factor: Vec<f32>,
    pub destroy_time: Vec<f32>,
    pub requires_tool: Vec<u8>,
    pub explosion_resistance: Vec<f32>,
    pub sound_type: Vec<u32>,
    pub prop_start: Vec<u32>,
    pub props: Vec<PropDef>,
    pub prop_values: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct StateTable {
    pub block: Vec<u16>,
    pub flags: Vec<u16>,
    pub light_emission: Vec<u8>,
    pub light_block: Vec<u8>,
    pub collision: Vec<u16>,
    pub outline: Vec<u16>,
    pub occlusion: Vec<u16>,
    pub map_color: Vec<u8>,
    pub sturdy_faces: Vec<u8>,
    pub occlusion_faces: Vec<u8>,
    pub fluid: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemTable {
    pub max_stack_size: Vec<u16>,
    pub max_damage: Vec<u32>,
    pub block: Vec<i32>,
    pub tool_start: Vec<u32>,
    pub tool_blocks: Vec<u32>,
    pub tool_speed: Vec<f32>,
    pub tool_correct: Vec<u8>,
    pub tool_default_speed: Vec<f32>,
    pub tool_damage: Vec<u8>,
    pub tool_flags: Vec<u8>,
    pub equip_slot: Vec<u8>,
    pub consume_seconds: Vec<f32>,
    pub cooldown_seconds: Vec<f32>,
    pub food_flags: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityTable {
    pub width: Vec<f32>,
    pub height: Vec<f32>,
    pub eye_height: Vec<f32>,
    pub category: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PacketEntry {
    pub state: u8,
    pub direction: u8,
    pub id: u32,
    pub name: u32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tables {
    pub version_name: String,
    pub protocol: u32,
    pub strings: Vec<String>,
    pub registries: Vec<Registry>,
    pub blocks: BlockTable,
    pub states: StateTable,
    pub shape_starts: Vec<u32>,
    pub shape_boxes: Vec<Aabb>,
    pub items: ItemTable,
    pub entities: EntityTable,
    pub fluid_source: Vec<u8>,
    pub packets: Vec<PacketEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("bad magic or format version")]
    Header,
    #[error("blob truncated")]
    Truncated,
    #[error("string table is not utf-8")]
    Utf8,
}

trait Prim: Copy {
    const SIZE: usize;
    fn put(self, out: &mut Vec<u8>);
    fn get(bytes: &[u8]) -> Self;
}

macro_rules! prim {
    ($($t:ty),*) => {$(
        impl Prim for $t {
            const SIZE: usize = size_of::<$t>();
            fn put(self, out: &mut Vec<u8>) {
                out.extend_from_slice(&self.to_le_bytes());
            }
            fn get(bytes: &[u8]) -> Self {
                Self::from_le_bytes(bytes.try_into().expect("chunk size"))
            }
        }
    )*};
}
prim!(u8, u16, u32, i32, f32);

struct Writer(Vec<u8>);

impl Writer {
    fn u32(&mut self, v: u32) {
        v.put(&mut self.0);
    }

    fn vec<T: Prim>(&mut self, items: &[T]) {
        self.u32(u32::try_from(items.len()).expect("table length"));
        for &item in items {
            item.put(&mut self.0);
        }
    }
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], DecodeError> {
        let (head, rest) = self.0.split_at_checked(n).ok_or(DecodeError::Truncated)?;
        self.0 = rest;
        Ok(head)
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::get(self.take(4)?))
    }

    fn vec<T: Prim>(&mut self) -> Result<Vec<T>, DecodeError> {
        let n = self.u32()? as usize;
        let bytes = self.take(n.checked_mul(T::SIZE).ok_or(DecodeError::Truncated)?)?;
        Ok(bytes.chunks_exact(T::SIZE).map(T::get).collect())
    }

    fn rows<T: Prim, const N: usize>(&mut self) -> Result<Vec<[T; N]>, DecodeError> {
        Ok(self.vec::<T>()?.as_chunks::<N>().0.to_vec())
    }
}

impl Tables {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer(Vec::new());
        w.0.extend_from_slice(MAGIC);
        w.u32(FORMAT);
        w.u32(self.protocol);
        let mut pool = Vec::new();
        let mut ends = Vec::with_capacity(self.strings.len());
        for s in &self.strings {
            pool.extend_from_slice(s.as_bytes());
            ends.push(u32::try_from(pool.len()).expect("string pool"));
        }
        w.vec(self.version_name.as_bytes());
        w.vec(&pool);
        w.vec(&ends);
        w.u32(u32::try_from(self.registries.len()).expect("registry count"));
        for r in &self.registries {
            w.u32(r.name);
            w.vec(&r.entries);
            w.vec(&r.sorted);
        }
        let b = &self.blocks;
        w.vec(&b.first_state);
        w.vec(&b.default_state);
        w.vec(&b.friction);
        w.vec(&b.speed_factor);
        w.vec(&b.jump_factor);
        w.vec(&b.destroy_time);
        w.vec(&b.requires_tool);
        w.vec(&b.explosion_resistance);
        w.vec(&b.sound_type);
        w.vec(&b.prop_start);
        let props: Vec<u32> = b
            .props
            .iter()
            .flat_map(|p| [p.name, p.value_start, p.value_count])
            .collect();
        w.vec(&props);
        w.vec(&b.prop_values);
        let s = &self.states;
        w.vec(&s.block);
        w.vec(&s.flags);
        w.vec(&s.light_emission);
        w.vec(&s.light_block);
        w.vec(&s.collision);
        w.vec(&s.outline);
        w.vec(&s.occlusion);
        w.vec(&s.map_color);
        w.vec(&s.sturdy_faces);
        w.vec(&s.occlusion_faces);
        w.vec(&s.fluid);
        w.vec(&self.shape_starts);
        let boxes: Vec<f32> = self
            .shape_boxes
            .iter()
            .flat_map(|a| a.min.into_iter().chain(a.max))
            .collect();
        w.vec(&boxes);
        w.vec(&self.items.max_stack_size);
        w.vec(&self.items.max_damage);
        w.vec(&self.items.block);
        let i = &self.items;
        w.vec(&i.tool_start);
        w.vec(&i.tool_blocks);
        w.vec(&i.tool_speed);
        w.vec(&i.tool_correct);
        w.vec(&i.tool_default_speed);
        w.vec(&i.tool_damage);
        w.vec(&i.tool_flags);
        w.vec(&i.equip_slot);
        w.vec(&i.consume_seconds);
        w.vec(&i.cooldown_seconds);
        w.vec(&i.food_flags);
        w.vec(&self.entities.width);
        w.vec(&self.entities.height);
        w.vec(&self.entities.eye_height);
        w.vec(&self.entities.category);
        w.vec(&self.fluid_source);
        let packets: Vec<u32> = self
            .packets
            .iter()
            .flat_map(|p| [u32::from(p.state), u32::from(p.direction), p.id, p.name])
            .collect();
        w.vec(&packets);
        w.0
    }

    pub fn decode(blob: &[u8]) -> Result<Self, DecodeError> {
        let mut r = Reader(blob);
        if r.take(4)? != MAGIC || r.u32()? != FORMAT {
            return Err(DecodeError::Header);
        }
        let protocol = r.u32()?;
        let version_name = String::from_utf8(r.vec::<u8>()?).map_err(|_| DecodeError::Utf8)?;
        let pool = r.vec::<u8>()?;
        let ends = r.vec::<u32>()?;
        let mut strings = Vec::with_capacity(ends.len());
        let mut start = 0usize;
        for end in ends {
            let end = end as usize;
            let s = pool.get(start..end).ok_or(DecodeError::Truncated)?;
            strings.push(
                std::str::from_utf8(s)
                    .map_err(|_| DecodeError::Utf8)?
                    .to_owned(),
            );
            start = end;
        }
        let registry_count = r.u32()?;
        let mut registries = Vec::with_capacity(registry_count as usize);
        for _ in 0..registry_count {
            registries.push(Registry {
                name: r.u32()?,
                entries: r.vec()?,
                sorted: r.vec()?,
            });
        }
        let mut blocks = BlockTable {
            first_state: r.vec()?,
            default_state: r.vec()?,
            friction: r.vec()?,
            speed_factor: r.vec()?,
            jump_factor: r.vec()?,
            destroy_time: r.vec()?,
            requires_tool: r.vec()?,
            explosion_resistance: r.vec()?,
            sound_type: r.vec()?,
            prop_start: r.vec()?,
            ..BlockTable::default()
        };
        blocks.props = r
            .rows::<u32, 3>()?
            .into_iter()
            .map(|[name, value_start, value_count]| PropDef {
                name,
                value_start,
                value_count,
            })
            .collect();
        blocks.prop_values = r.vec()?;
        let states = StateTable {
            block: r.vec()?,
            flags: r.vec()?,
            light_emission: r.vec()?,
            light_block: r.vec()?,
            collision: r.vec()?,
            outline: r.vec()?,
            occlusion: r.vec()?,
            map_color: r.vec()?,
            sturdy_faces: r.vec()?,
            occlusion_faces: r.vec()?,
            fluid: r.vec()?,
        };
        let shape_starts = r.vec()?;
        let shape_boxes = r
            .rows::<f32, 6>()?
            .into_iter()
            .map(|[x0, y0, z0, x1, y1, z1]| Aabb {
                min: [x0, y0, z0],
                max: [x1, y1, z1],
            })
            .collect();
        let items = ItemTable {
            max_stack_size: r.vec()?,
            max_damage: r.vec()?,
            block: r.vec()?,
            tool_start: r.vec()?,
            tool_blocks: r.vec()?,
            tool_speed: r.vec()?,
            tool_correct: r.vec()?,
            tool_default_speed: r.vec()?,
            tool_damage: r.vec()?,
            tool_flags: r.vec()?,
            equip_slot: r.vec()?,
            consume_seconds: r.vec()?,
            cooldown_seconds: r.vec()?,
            food_flags: r.vec()?,
        };
        let entities = EntityTable {
            width: r.vec()?,
            height: r.vec()?,
            eye_height: r.vec()?,
            category: r.vec()?,
        };
        let fluid_source = r.vec()?;
        let packets = r
            .rows::<u32, 4>()?
            .into_iter()
            .map(|[state, direction, id, name]| PacketEntry {
                state: state as u8,
                direction: direction as u8,
                id,
                name,
            })
            .collect();
        Ok(Self {
            version_name,
            protocol,
            strings,
            registries,
            blocks,
            states,
            shape_starts,
            shape_boxes,
            items,
            entities,
            fluid_source,
            packets,
        })
    }
}

pub mod flag {
    pub const AIR: u16 = 1 << 0;
    pub const LIQUID: u16 = 1 << 1;
    pub const REPLACEABLE: u16 = 1 << 2;
    pub const HAS_BLOCK_ENTITY: u16 = 1 << 3;
    pub const CAN_OCCLUDE: u16 = 1 << 4;
    pub const USE_SHAPE_FOR_LIGHT_OCCLUSION: u16 = 1 << 5;
    pub const SOLID_RENDER: u16 = 1 << 6;
    pub const FULL_COLLISION: u16 = 1 << 7;
    pub const PROPAGATES_SKYLIGHT_DOWN: u16 = 1 << 8;
    pub const REDSTONE_CONDUCTOR: u16 = 1 << 9;
}

pub mod fluid_bits {
    pub const ID_MASK: u16 = 0xff;
    pub const AMOUNT_SHIFT: u16 = 8;
    pub const AMOUNT_MASK: u16 = 0xf;
    pub const SOURCE: u16 = 1 << 12;
    pub const FALLING: u16 = 1 << 13;
    pub const PRESENT: u16 = 1 << 14;
}

pub const STATE_NAMES: [&str; 5] = ["handshake", "status", "login", "configuration", "play"];
pub const DIRECTION_NAMES: [&str; 2] = ["clientbound", "serverbound"];
