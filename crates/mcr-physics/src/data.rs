use std::collections::HashMap;

use mcr_data::Version;

pub mod flag {
    pub const AIR: u32 = 1 << 0;
    pub const FULL_CUBE: u32 = 1 << 1;
    pub const LARGE: u32 = 1 << 2;
    pub const CLIMBABLE: u32 = 1 << 3;
    pub const SCAFFOLDING: u32 = 1 << 4;
    pub const WEB: u32 = 1 << 5;
    pub const POWDER_SNOW: u32 = 1 << 6;
    pub const SWEET_BERRY: u32 = 1 << 7;
    pub const HONEY: u32 = 1 << 8;
    pub const SLIME: u32 = 1 << 9;
    pub const BED: u32 = 1 << 10;
    pub const OPEN_TRAPDOOR: u32 = 1 << 11;
    pub const LADDER: u32 = 1 << 12;
    pub const WALL: u32 = 1 << 13;
    pub const FENCE_GATE: u32 = 1 << 14;
    pub const BUBBLE_COLUMN: u32 = 1 << 15;
    pub const ICE: u32 = 1 << 16;
    pub const SIGN: u32 = 1 << 17;
    pub const FLUID_FALLING: u32 = 1 << 18;
    pub const FLUID_SOURCE: u32 = 1 << 19;
    pub const WATER_BLOCK: u32 = 1 << 20;
    pub const FENCE: u32 = 1 << 21;
    pub const DRAG_DOWN: u32 = 1 << 22;
    pub const LILY_PAD: u32 = 1 << 23;
    /// Vanilla `isSuffocating`: what the local player is pushed out of.
    pub const SUFFOCATING: u32 = 1 << 24;
    /// A floating scaffolding bottom piece, which keeps a thin floor.
    pub const SCAFFOLDING_UNSTABLE: u32 = 1 << 25;
    /// `BlockTags.CAN_GLIDE_THROUGH`: a gliding player is not climbing it.
    pub const GLIDE_THROUGH: u32 = 1 << 26;
}

pub const FLUID_NONE: u8 = 0;
pub const FLUID_WATER: u8 = 1;
pub const FLUID_LAVA: u8 = 2;

/// The `causes_suffocation` tag is every full collision cube, minus vanilla's `isSuffocating` overrides.
fn suffocates(short: &str, full_cube: bool, extended: impl FnOnce() -> bool) -> bool {
    match short {
        "farmland" | "soul_sand" | "dirt_path" | "mud" => true,
        "glass" | "tinted_glass" | "mangrove_roots" | "moving_piston" => false,
        "piston" | "sticky_piston" => !extended(),
        _ if short.ends_with("_leaves")
            || short.ends_with("_stained_glass")
            || short.ends_with("copper_grate") =>
        {
            false
        }
        _ => full_cube,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StateInfo {
    pub box_start: u32,
    pub box_len: u8,
    pub flags: u32,
    pub friction: f32,
    pub speed_factor: f32,
    pub jump_factor: f32,
    pub bounce: f32,
    pub fluid: u8,
    pub fluid_amount: u8,
    pub facing: u8,
    pub sturdy: u8,
}

impl StateInfo {
    pub fn has(&self, f: u32) -> bool {
        self.flags & f != 0
    }

    pub fn is_air(&self) -> bool {
        self.has(flag::AIR)
    }

    pub fn has_collision(&self) -> bool {
        self.box_len != 0
    }

    pub fn own_fluid_height(&self) -> f32 {
        f32::from(self.fluid_amount) / 9.0
    }
}

#[derive(Debug)]
pub struct PhysicsData {
    states: Vec<StateInfo>,
    boxes: Vec<[f64; 6]>,
    overhang_hi: [bool; 3],
    overhang_lo: [bool; 3],
    empty: StateInfo,
}

impl PhysicsData {
    /// For the active tables ([`mcr_data::v26_3`]).
    pub fn v26_3() -> &'static PhysicsData {
        static DATA: mcr_data::PerVersion<PhysicsData> = mcr_data::PerVersion::new();
        DATA.get(PhysicsData::build)
    }

    pub fn build(v: &Version) -> Self {
        let fluid_group = |name: &str| v.fluid_by_name(name);
        let water = [
            fluid_group("minecraft:water"),
            fluid_group("minecraft:flowing_water"),
        ];
        let lava = [
            fluid_group("minecraft:lava"),
            fluid_group("minecraft:flowing_lava"),
        ];
        let mut boxes: Vec<[f64; 6]> = Vec::new();
        let mut shape_slots: HashMap<u16, (u32, u8)> = HashMap::new();
        let mut states = Vec::with_capacity(v.state_count());
        let mut overhang_hi = [false; 3];
        let mut overhang_lo = [false; 3];
        for id in 0..v.state_count() as u32 {
            let info = v.block_state(id).expect("state in range");
            let block = v.block_info(info.block).expect("block in range");
            let name = v.state_block_name(id).unwrap_or("");
            let short = name.strip_prefix("minecraft:").unwrap_or(name);
            let shape_index = v.collision_shape_index(id).unwrap_or(0);
            let shape = v.collision_shape(id);
            let (box_start, box_len) = *shape_slots.entry(shape_index).or_insert_with(|| {
                let start = boxes.len() as u32;
                for b in shape {
                    boxes.push([
                        f64::from(b.min[0]),
                        f64::from(b.min[1]),
                        f64::from(b.min[2]),
                        f64::from(b.max[0]),
                        f64::from(b.max[1]),
                        f64::from(b.max[2]),
                    ]);
                }
                (start, shape.len() as u8)
            });
            let mut flags = 0u32;
            let mut large = false;
            for b in shape {
                for a in 0..3 {
                    if b.max[a] > 1.0 {
                        overhang_hi[a] = true;
                        large = true;
                    }
                    if b.min[a] < 0.0 {
                        overhang_lo[a] = true;
                        large = true;
                    }
                }
            }
            if large {
                flags |= flag::LARGE;
            }
            if shape.len() == 1 && shape[0].min == [0.0; 3] && shape[0].max == [1.0; 3] {
                flags |= flag::FULL_CUBE;
            }
            if info.is_air() {
                flags |= flag::AIR;
            }
            let mut bounce = 0.0f32;
            match short {
                "ladder" => flags |= flag::CLIMBABLE | flag::LADDER,
                "vine"
                | "weeping_vines"
                | "weeping_vines_plant"
                | "twisting_vines"
                | "twisting_vines_plant"
                | "cave_vines"
                | "cave_vines_plant" => flags |= flag::CLIMBABLE | flag::GLIDE_THROUGH,
                "scaffolding" => flags |= flag::CLIMBABLE | flag::SCAFFOLDING,
                "cobweb" => flags |= flag::WEB,
                "lily_pad" => flags |= flag::LILY_PAD,
                "powder_snow" => flags |= flag::POWDER_SNOW,
                "sweet_berry_bush" => flags |= flag::SWEET_BERRY,
                "honey_block" => flags |= flag::HONEY,
                "slime_block" => {
                    flags |= flag::SLIME;
                    bounce = 1.0;
                }
                "bubble_column" => flags |= flag::BUBBLE_COLUMN | flag::WATER_BLOCK,
                "water" => flags |= flag::WATER_BLOCK,
                "ice" | "frosted_ice" => flags |= flag::ICE,
                _ => {}
            }
            if short.ends_with("_bed") {
                flags |= flag::BED;
                bounce = 0.75;
            } else if short == "shelf_mushroom" {
                bounce = 0.75;
            }
            if short.ends_with("_wall") {
                flags |= flag::WALL;
            }
            if short.ends_with("_fence") {
                flags |= flag::FENCE;
            }
            if short == "bubble_column" && v.state_properties(id).any(|kv| kv == ("drag", "true")) {
                flags |= flag::DRAG_DOWN;
            }
            if short.ends_with("_fence_gate") {
                flags |= flag::FENCE_GATE;
            }
            if suffocates(short, flags & flag::FULL_CUBE != 0, || {
                v.state_properties(id).any(|kv| kv == ("extended", "true"))
            }) {
                flags |= flag::SUFFOCATING;
            }
            if short == "scaffolding" {
                let floating = v.state_properties(id).any(|kv| kv == ("bottom", "true"))
                    && v.state_properties(id)
                        .any(|(k, val)| k == "distance" && val != "0");
                if floating {
                    flags |= flag::SCAFFOLDING_UNSTABLE;
                }
            }
            if short.ends_with("_sign") || short.ends_with("_wall_sign") {
                flags |= flag::SIGN;
            }
            let mut facing = 0u8;
            if short.ends_with("trapdoor") || short == "ladder" {
                for (k, val) in v.state_properties(id) {
                    match (k, val) {
                        ("open", "true") if short.ends_with("trapdoor") => {
                            flags |= flag::OPEN_TRAPDOOR
                        }
                        ("facing", f) => {
                            facing = match f {
                                "north" => 1,
                                "south" => 2,
                                "west" => 3,
                                "east" => 4,
                                _ => 0,
                            }
                        }
                        _ => {}
                    }
                }
            }
            let (fluid, fluid_amount) = match info.fluid {
                Some(f) => {
                    let group = if water.contains(&Some(f.fluid)) {
                        FLUID_WATER
                    } else if lava.contains(&Some(f.fluid)) {
                        FLUID_LAVA
                    } else {
                        FLUID_NONE
                    };
                    if f.falling {
                        flags |= flag::FLUID_FALLING;
                    }
                    if f.source {
                        flags |= flag::FLUID_SOURCE;
                    }
                    (group, f.amount)
                }
                None => (FLUID_NONE, 0),
            };
            states.push(StateInfo {
                box_start,
                box_len,
                flags,
                friction: block.friction,
                speed_factor: block.speed_factor,
                jump_factor: block.jump_factor,
                bounce,
                fluid,
                fluid_amount,
                facing,
                sturdy: info.sturdy_faces,
            });
        }
        let empty = states.first().copied().expect("air state");
        Self {
            states,
            boxes,
            overhang_hi,
            overhang_lo,
            empty,
        }
    }

    #[inline]
    pub fn state(&self, id: u32) -> &StateInfo {
        self.states.get(id as usize).unwrap_or(&self.empty)
    }

    #[inline]
    pub fn boxes(&self, s: &StateInfo) -> &[[f64; 6]] {
        &self.boxes[s.box_start as usize..s.box_start as usize + s.box_len as usize]
    }

    #[inline]
    pub fn overhang_hi(&self, axis: usize) -> bool {
        self.overhang_hi[axis]
    }

    #[inline]
    pub fn overhang_lo(&self, axis: usize) -> bool {
        self.overhang_lo[axis]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn by_name(name: &str) -> (&'static PhysicsData, u32) {
        let v = mcr_data::v26_3();
        let b = v.block_by_name(name).unwrap();
        (PhysicsData::v26_3(), v.block_info(b).unwrap().default_state)
    }

    #[test]
    fn classifies_blocks() {
        let (d, s) = by_name("minecraft:ice");
        assert!((d.state(s).friction - 0.98).abs() < 1e-6);
        let (d, s) = by_name("minecraft:cobweb");
        assert!(d.state(s).has(flag::WEB));
        let (d, s) = by_name("minecraft:stone");
        assert!(d.state(s).has(flag::FULL_CUBE));
        let (d, s) = by_name("minecraft:oak_fence");
        assert!(d.state(s).has(flag::LARGE));
        let (d, s) = by_name("minecraft:water");
        assert_eq!(d.state(s).fluid, FLUID_WATER);
        assert_eq!(d.state(s).fluid_amount, 8);
        assert!(d.overhang_hi(1));
    }
}
