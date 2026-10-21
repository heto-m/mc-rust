//! `Block#getStateForPlacement` and `canBeReplaced` for the block families the client predicts.

use mcr_buf::BlockPos;
use mcr_protocol::play::serverbound::Facing;

use crate::state::ClientState;

const ALL: [Facing; 6] = [
    Facing::Down,
    Facing::Up,
    Facing::North,
    Facing::South,
    Facing::West,
    Facing::East,
];

/// `Direction.BY_2D_DATA`.
const HORIZONTAL_2D: [Facing; 4] = [Facing::South, Facing::West, Facing::North, Facing::East];

pub(crate) fn step(f: Facing) -> [i32; 3] {
    match f {
        Facing::Down => [0, -1, 0],
        Facing::Up => [0, 1, 0],
        Facing::North => [0, 0, -1],
        Facing::South => [0, 0, 1],
        Facing::West => [-1, 0, 0],
        Facing::East => [1, 0, 0],
    }
}

pub(crate) fn relative(pos: BlockPos, f: Facing) -> BlockPos {
    let s = step(f);
    BlockPos::new(
        pos.x.saturating_add(s[0]),
        pos.y.saturating_add(s[1]),
        pos.z.saturating_add(s[2]),
    )
}

fn opposite(f: Facing) -> Facing {
    match f {
        Facing::Down => Facing::Up,
        Facing::Up => Facing::Down,
        Facing::North => Facing::South,
        Facing::South => Facing::North,
        Facing::West => Facing::East,
        Facing::East => Facing::West,
    }
}

fn clockwise(f: Facing) -> Facing {
    match f {
        Facing::North => Facing::East,
        Facing::East => Facing::South,
        Facing::South => Facing::West,
        Facing::West => Facing::North,
        v => v,
    }
}

fn counter_clockwise(f: Facing) -> Facing {
    match f {
        Facing::North => Facing::West,
        Facing::West => Facing::South,
        Facing::South => Facing::East,
        Facing::East => Facing::North,
        v => v,
    }
}

fn axis(f: Facing) -> &'static str {
    match f {
        Facing::Down | Facing::Up => "y",
        Facing::North | Facing::South => "z",
        Facing::West | Facing::East => "x",
    }
}

fn is_horizontal(f: Facing) -> bool {
    !matches!(f, Facing::Down | Facing::Up)
}

fn facing_name(f: Facing) -> &'static str {
    match f {
        Facing::Down => "down",
        Facing::Up => "up",
        Facing::North => "north",
        Facing::South => "south",
        Facing::West => "west",
        Facing::East => "east",
    }
}

fn facing_from_name(s: &str) -> Option<Facing> {
    ALL.into_iter().find(|f| facing_name(*f) == s)
}

/// `Entity#getDirection`: `Direction#fromYRot`.
pub(crate) fn horizontal_direction(yaw: f32) -> Facing {
    let q = (f64::from(yaw) / 90.0 + 0.5).floor();
    let q = if q.is_finite() {
        q.rem_euclid(4.0)
    } else {
        0.0
    };
    HORIZONTAL_2D[q as usize & 3]
}

/// `Direction#orderedByNearest(entity)[0]`.
pub(crate) fn nearest_looking(yaw: f32, pitch: f32) -> Facing {
    let pitch = pitch.to_radians();
    let yaw = -yaw.to_radians();
    let (ps, pc) = (pitch.sin(), pitch.cos());
    let (ys, yc) = (yaw.sin(), yaw.cos());
    let x_pos = ys > 0.0;
    let y_pos = ps < 0.0;
    let z_pos = yc > 0.0;
    let x_yaw = if x_pos { ys } else { -ys };
    let y_mag = if y_pos { -ps } else { ps };
    let z_yaw = if z_pos { yc } else { -yc };
    let x_mag = x_yaw * pc;
    let z_mag = z_yaw * pc;
    let ax = if x_pos { Facing::East } else { Facing::West };
    let ay = if y_pos { Facing::Up } else { Facing::Down };
    let az = if z_pos { Facing::South } else { Facing::North };
    if x_yaw > z_yaw {
        if y_mag > x_mag { ay } else { ax }
    } else if y_mag > z_mag {
        ay
    } else {
        az
    }
}

/// The parts of `BlockPlaceContext` that placement states read.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlaceContext {
    pub clicked: BlockPos,
    pub face: Facing,
    /// The absolute hit location.
    pub hit: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// The block of the held item.
    pub item_block: u32,
    pub replace_clicked: bool,
}

impl PlaceContext {
    pub(crate) fn pos(&self) -> BlockPos {
        if self.replace_clicked {
            self.clicked
        } else {
            relative(self.clicked, self.face)
        }
    }

    /// The bottom half for stairs, slabs and trapdoors: `clickedFace != DOWN && (UP || y <= 0.5)`.
    fn bottom_half(&self) -> bool {
        let y = self.hit[1] - f64::from(self.pos().y);
        self.face != Facing::Down && (self.face == Facing::Up || y <= 0.5)
    }
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

/// Blocks whose own `canBeReplaced` the client does not model.
fn custom_replace(name: &str) -> bool {
    name == "candle"
        || name.ends_with("_candle")
        || matches!(
            name,
            "snow"
                | "sea_pickle"
                | "turtle_egg"
                | "vine"
                | "glow_lichen"
                | "sculk_vein"
                | "resin_clump"
                | "scaffolding"
                | "pink_petals"
                | "wildflowers"
                | "leaf_litter"
                | "pitcher_crop"
                | "kelp_plant"
                | "twisting_vines_plant"
                | "weeping_vines_plant"
                | "cave_vines_plant"
        )
}

/// Whether placing at a block can go ahead, or `None` when the block's rule is not modelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Replace {
    Yes,
    No,
    Unknown,
}

impl ClientState {
    fn state_name(&self, state: u32) -> &'static str {
        self.data.state_block_name(state).map_or("", short)
    }

    fn prop(&self, state: u32, key: &str) -> Option<&'static str> {
        self.data
            .state_properties(state)
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }

    fn state_at(&self, pos: BlockPos) -> Option<u32> {
        self.chunks.block(pos.x, pos.y, pos.z).map(|s| s.0)
    }

    /// `FluidState#is(Fluids.WATER)`: a water source, waterlogged blocks included.
    fn water_source_at(&self, pos: BlockPos) -> bool {
        self.state_at(pos)
            .and_then(|s| self.data.block_state(s))
            .and_then(|i| i.fluid)
            .is_some_and(|f| f.source && self.data.fluid_name(f.fluid).map(short) == Some("water"))
    }

    fn any_water_at(&self, pos: BlockPos) -> bool {
        self.state_at(pos)
            .and_then(|s| self.data.block_state(s))
            .and_then(|i| i.fluid)
            .is_some_and(|f| {
                self.data
                    .fluid_name(f.fluid)
                    .map(short)
                    .is_some_and(|n| n.ends_with("water"))
            })
    }

    /// `BlockState#canBeReplaced(BlockPlaceContext)`.
    pub(crate) fn can_be_replaced(&self, state: u32, ctx: &PlaceContext) -> Replace {
        let Some(info) = self.data.block_state(state) else {
            return Replace::No;
        };
        let name = self.state_name(state);
        if name.ends_with("_slab") {
            let Some(kind) = self.prop(state, "type") else {
                return Replace::Unknown;
            };
            if kind == "double" || info.block != ctx.item_block {
                return Replace::No;
            }
            if !ctx.replace_clicked {
                return Replace::Yes;
            }
            let upper = ctx.hit[1] - f64::from(ctx.clicked.y) > 0.5;
            let ok = if kind == "bottom" {
                ctx.face == Facing::Up || upper && is_horizontal(ctx.face)
            } else {
                ctx.face == Facing::Down || !upper && is_horizontal(ctx.face)
            };
            return if ok { Replace::Yes } else { Replace::No };
        }
        if custom_replace(name) {
            return Replace::Unknown;
        }
        if (info.is_air() || info.is_replaceable()) && info.block != ctx.item_block {
            Replace::Yes
        } else {
            Replace::No
        }
    }

    /// `Block#getStateForPlacement` for the families the client models, `None` for the rest.
    pub(crate) fn placement_state(&self, block: u32, ctx: &PlaceContext) -> Option<u32> {
        let data = self.data;
        let default = data.block_info(block)?.default_state;
        let name = short(data.block_name(block)?);
        let pos = ctx.pos();
        let mut keys = [""; 4];
        let mut n = 0;
        for (k, _) in data.state_properties(default) {
            if n == keys.len() {
                return None;
            }
            keys[n] = k;
            n += 1;
        }
        let keys = &mut keys[..n];
        keys.sort_unstable();
        let keys = &*keys;
        let set = |s: u32, k: &str, v: &str| data.with_property(s, k, v);
        let water = || {
            if self.water_source_at(pos) {
                "true"
            } else {
                "false"
            }
        };
        let horizontal = horizontal_direction(ctx.yaw);
        match keys {
            [] => self.plain_placement(name, default, pos),
            _ if name.ends_with("_stairs") => {
                let s = set(default, "facing", facing_name(horizontal))?;
                let half = if ctx.bottom_half() { "bottom" } else { "top" };
                let s = set(s, "half", half)?;
                let s = set(s, "waterlogged", water())?;
                set(s, "shape", self.stairs_shape(horizontal, half, pos))
            }
            _ if name.ends_with("_slab") => {
                let replaced = self.state_at(pos)?;
                if data.block_state(replaced)?.block == block {
                    let s = set(replaced, "type", "double")?;
                    return set(s, "waterlogged", "false");
                }
                let kind = if ctx.bottom_half() { "bottom" } else { "top" };
                set(set(default, "type", kind)?, "waterlogged", water())
            }
            ["axis"] => set(default, "axis", axis(ctx.face)),
            ["axis", "waterlogged"] if name.ends_with("chain") => set(
                set(default, "axis", axis(ctx.face))?,
                "waterlogged",
                water(),
            ),
            ["waterlogged"]
                if name.ends_with("copper_grate")
                    || matches!(name, "heavy_core" | "mangrove_roots" | "barrier") =>
            {
                set(default, "waterlogged", water())
            }
            ["distance", "persistent", "waterlogged"] if name.ends_with("_leaves") => {
                let s = set(default, "persistent", "true")?;
                let s = set(s, "waterlogged", water())?;
                set(s, "distance", &self.leaf_distance(pos).to_string())
            }
            _ => {
                let facing = match name {
                    "furnace" | "smoker" | "blast_furnace" | "carved_pumpkin"
                    | "jack_o_lantern" | "loom" | "stonecutter" | "beehive" | "bee_nest"
                    | "chiseled_bookshelf" => opposite(horizontal),
                    _ if name.ends_with("_glazed_terracotta") => opposite(horizontal),
                    "anvil" | "chipped_anvil" | "damaged_anvil" => clockwise(horizontal),
                    "dispenser" | "dropper" | "piston" | "sticky_piston" | "barrel" => {
                        opposite(nearest_looking(ctx.yaw, ctx.pitch))
                    }
                    "observer" => nearest_looking(ctx.yaw, ctx.pitch),
                    _ if name == "shulker_box" || name.ends_with("_shulker_box") => ctx.face,
                    "ender_chest" => {
                        let s = set(default, "facing", facing_name(opposite(horizontal)))?;
                        return set(s, "waterlogged", water());
                    }
                    _ => return None,
                };
                set(default, "facing", facing_name(facing))
            }
        }
    }

    /// Property-less blocks: only those that need no support and never turn into another block.
    fn plain_placement(&self, name: &str, default: u32, pos: BlockPos) -> Option<u32> {
        if self.data.collision_shape(default).is_empty()
            || matches!(
                name,
                "lily_pad" | "frogspawn" | "dirt_path" | "azalea" | "flowering_azalea"
            )
        {
            return None;
        }
        if name.ends_with("carpet") {
            let below = self.state_at(relative(pos, Facing::Down))?;
            if self.data.block_state(below)?.is_air() {
                return None;
            }
        }
        if name.ends_with("_concrete_powder")
            && (self.any_water_at(pos) || ALL.iter().any(|f| self.any_water_at(relative(pos, *f))))
        {
            return None;
        }
        Some(default)
    }

    /// `StairBlock#getStairsShape`.
    fn stairs_shape(&self, facing: Facing, half: &str, pos: BlockPos) -> &'static str {
        let stairs = |p: BlockPos| {
            let s = self.state_at(p)?;
            if !self.state_name(s).ends_with("_stairs") {
                return None;
            }
            Some((
                facing_from_name(self.prop(s, "facing")?)?,
                self.prop(s, "half")?,
            ))
        };
        let can_take =
            |dir: Facing| stairs(relative(pos, dir)).is_none_or(|(f, h)| f != facing || h != half);
        if let Some((behind, h)) = stairs(relative(pos, facing))
            && h == half
            && axis(behind) != axis(facing)
            && can_take(opposite(behind))
        {
            return if behind == counter_clockwise(facing) {
                "outer_left"
            } else {
                "outer_right"
            };
        }
        if let Some((front, h)) = stairs(relative(pos, opposite(facing)))
            && h == half
            && axis(front) != axis(facing)
            && can_take(front)
        {
            return if front == counter_clockwise(facing) {
                "inner_left"
            } else {
                "inner_right"
            };
        }
        "straight"
    }

    /// `LeavesBlock#updateDistance`.
    fn leaf_distance(&self, pos: BlockPos) -> u8 {
        let mut best = 7u8;
        for f in ALL {
            let Some(s) = self.state_at(relative(pos, f)) else {
                continue;
            };
            let block = self.data.block_state(s).map_or(-1, |i| i.block as i32);
            let at = if self.registries.is_in_tag(
                "minecraft:block",
                "minecraft:prevents_nearby_leaf_decay",
                block,
            ) {
                0
            } else if self.state_name(s).ends_with("_leaves") {
                self.prop(s, "distance")
                    .and_then(|d| d.parse::<u8>().ok())
                    .unwrap_or(7)
            } else {
                7
            };
            best = best.min(at.saturating_add(1));
            if best == 1 {
                break;
            }
        }
        best.clamp(1, 7)
    }
}
