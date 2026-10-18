//! Per block state lookup built once from the static block data.

use mcr_data::Version;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Kind {
    None,
    Torch,
    WallTorch,
    RedstoneTorch,
    RedstoneWallTorch,
    Candle,
    CandleCake,
    Campfire,
    Fire,
    SoulFire,
    Furnace,
    BlastFurnace,
    Smoker,
    BrewingStand,
    Beehive,
    Brushable,
    FallingBlock,
    Sand,
    BubbleColumn,
    CreakingHeart,
    CryingObsidian,
    DriedGhast,
    DeadBush,
    DryGrass,
    EnchantingTable,
    EnderChest,
    EndGateway,
    EndPortal,
    EndRod,
    Eyeblossom,
    FireflyBush,
    HangingMoss,
    Leaves,
    Lever,
    LightningRod,
    Mycelium,
    NetherPortal,
    PointedDripstone,
    PotentSulfur,
    RedstoneOre,
    RedstoneWire,
    Repeater,
    RespawnAnchor,
    SculkSensor,
    SporeBlossom,
    WetSponge,
    WitherRose,
    Vault,
    TrialSpawner,
}

/// `TrialSpawnerState`, which decides the client particles and whether the mob spins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrialState {
    Inactive,
    WaitingForPlayers,
    Active,
    WaitingForRewardEjection,
    EjectingReward,
    Cooldown,
}

impl TrialState {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "inactive" => Self::Inactive,
            "waiting_for_players" => Self::WaitingForPlayers,
            "active" => Self::Active,
            "waiting_for_reward_ejection" => Self::WaitingForRewardEjection,
            "ejecting_reward" => Self::EjectingReward,
            "cooldown" => Self::Cooldown,
            _ => return None,
        })
    }

    fn from_index(i: u16) -> Self {
        match i {
            1 => Self::WaitingForPlayers,
            2 => Self::Active,
            3 => Self::WaitingForRewardEjection,
            4 => Self::EjectingReward,
            5 => Self::Cooldown,
            _ => Self::Inactive,
        }
    }

    /// `isCapableOfSpawning`.
    pub fn can_spawn(self) -> bool {
        matches!(self, Self::WaitingForPlayers | Self::Active)
    }
}

pub mod flag {
    pub const AIR: u16 = 1;
    pub const PLAIN_AIR: u16 = 1 << 1;
    pub const SOLID_RENDER: u16 = 1 << 2;
    pub const FULL_COLLISION: u16 = 1 << 3;
    pub const CAN_OCCLUDE: u16 = 1 << 4;
    pub const FLAMMABLE: u16 = 1 << 5;
    pub const TOP_FULL: u16 = 1 << 6;
    pub const FREE_TO_FALL: u16 = 1 << 7;
    pub const DRIP_DOWN: u16 = 1 << 8;
}

pub mod tag {
    pub const IMPERMEABLE: u16 = 1;
    pub const TERRACOTTA: u16 = 1 << 1;
    pub const DESERT_SAND: u16 = 1 << 2;
    pub const DESERT_VEGETATION: u16 = 1 << 3;
    pub const GHAST_TRIGGER: u16 = 1 << 4;
    pub const PALE_OAK_LOGS: u16 = 1 << 5;
    pub const POPLAR_LEAF_SOUND: u16 = 1 << 6;
    pub const POWER_PROVIDER: u16 = 1 << 7;
    pub const POWER_TRANSMITTER: u16 = 1 << 8;

    pub const ALL: [(u16, &str); 9] = [
        (IMPERMEABLE, "minecraft:impermeable"),
        (TERRACOTTA, "minecraft:terracotta"),
        (
            DESERT_SAND,
            "minecraft:triggers_ambient_desert_sand_block_sounds",
        ),
        (
            DESERT_VEGETATION,
            "minecraft:triggers_ambient_desert_dry_vegetation_block_sounds",
        ),
        (
            GHAST_TRIGGER,
            "minecraft:triggers_ambient_dried_ghast_block_sounds",
        ),
        (PALE_OAK_LOGS, "minecraft:pale_oak_logs"),
        (
            POPLAR_LEAF_SOUND,
            "minecraft:required_for_poplar_leaf_ambience",
        ),
        (POWER_PROVIDER, "minecraft:enchantment_power_provider"),
        (POWER_TRANSMITTER, "minecraft:enchantment_power_transmitter"),
    ];
}

pub mod fluid {
    pub const NONE: u8 = 0;
    pub const WATER_SOURCE: u8 = 1;
    pub const WATER_FALLING: u8 = 2;
    pub const WATER_FLOWING: u8 = 3;
    pub const LAVA: u8 = 4;
}

/// `Direction.ordinal()` order: down, up, north, south, west, east.
pub const DIRS: [[i32; 3]; 6] = [
    [0, -1, 0],
    [0, 1, 0],
    [0, 0, -1],
    [0, 0, 1],
    [-1, 0, 0],
    [1, 0, 0],
];

pub const HORIZONTAL: [usize; 4] = [2, 5, 3, 4];

pub const fn opposite(dir: usize) -> usize {
    dir ^ 1
}

#[derive(Debug, Clone, Copy)]
pub struct Info {
    pub kind: Kind,
    pub arg: u8,
    pub aux: u16,
    pub flags: u16,
    pub sturdy: u8,
    pub fluid: u8,
    pub block: u16,
}

impl Info {
    pub const AIR: Info = Info {
        kind: Kind::None,
        arg: 0,
        aux: 0,
        flags: flag::AIR | flag::PLAIN_AIR | flag::FREE_TO_FALL,
        sturdy: 0,
        fluid: fluid::NONE,
        block: u16::MAX,
    };

    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    pub fn sturdy(&self, face: usize) -> bool {
        self.sturdy >> face & 1 != 0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Leaf {
    pub chance: f32,
    pub particle: LeafParticle,
    pub poplar_sound: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafParticle {
    None,
    Tinted,
    Birch,
    Azalea,
    Cherry,
    PaleOak,
    RedPoplar,
    OrangePoplar,
    YellowPoplar,
}

pub const LEAVES: [(&str, Leaf); 14] = {
    const fn leaf(chance: f32, particle: LeafParticle, poplar_sound: bool) -> Leaf {
        Leaf {
            chance,
            particle,
            poplar_sound,
        }
    }
    use LeafParticle::*;
    [
        ("oak_leaves", leaf(0.01, Tinted, false)),
        ("spruce_leaves", leaf(0.0, None, false)),
        ("birch_leaves", leaf(0.01, Birch, false)),
        ("jungle_leaves", leaf(0.01, Tinted, false)),
        ("acacia_leaves", leaf(0.01, Tinted, false)),
        ("cherry_leaves", leaf(0.1, Cherry, false)),
        ("dark_oak_leaves", leaf(0.01, Tinted, false)),
        ("pale_oak_leaves", leaf(0.02, PaleOak, false)),
        ("mangrove_leaves", leaf(0.01, Tinted, false)),
        ("red_poplar_leaves", leaf(0.01, RedPoplar, true)),
        ("orange_poplar_leaves", leaf(0.01, OrangePoplar, true)),
        ("yellow_poplar_leaves", leaf(0.01, YellowPoplar, true)),
        ("azalea_leaves", leaf(0.01, Azalea, false)),
        ("flowering_azalea_leaves", leaf(0.01, Azalea, false)),
    ]
};

const FLAMMABLE: &[&str] = &[
    "oak_planks",
    "spruce_planks",
    "birch_planks",
    "jungle_planks",
    "acacia_planks",
    "cherry_planks",
    "dark_oak_planks",
    "pale_oak_planks",
    "poplar_planks",
    "mangrove_planks",
    "bamboo_planks",
    "bamboo_mosaic",
    "oak_slab",
    "spruce_slab",
    "birch_slab",
    "jungle_slab",
    "acacia_slab",
    "cherry_slab",
    "dark_oak_slab",
    "pale_oak_slab",
    "poplar_slab",
    "mangrove_slab",
    "bamboo_slab",
    "bamboo_mosaic_slab",
    "oak_fence_gate",
    "spruce_fence_gate",
    "birch_fence_gate",
    "jungle_fence_gate",
    "acacia_fence_gate",
    "cherry_fence_gate",
    "dark_oak_fence_gate",
    "pale_oak_fence_gate",
    "poplar_fence_gate",
    "mangrove_fence_gate",
    "bamboo_fence_gate",
    "oak_fence",
    "spruce_fence",
    "birch_fence",
    "jungle_fence",
    "acacia_fence",
    "cherry_fence",
    "dark_oak_fence",
    "pale_oak_fence",
    "poplar_fence",
    "mangrove_fence",
    "bamboo_fence",
    "oak_stairs",
    "birch_stairs",
    "spruce_stairs",
    "jungle_stairs",
    "acacia_stairs",
    "cherry_stairs",
    "dark_oak_stairs",
    "pale_oak_stairs",
    "poplar_stairs",
    "mangrove_stairs",
    "bamboo_stairs",
    "bamboo_mosaic_stairs",
    "oak_log",
    "spruce_log",
    "birch_log",
    "jungle_log",
    "acacia_log",
    "cherry_log",
    "pale_oak_log",
    "poplar_log",
    "dark_oak_log",
    "mangrove_log",
    "bamboo_block",
    "stripped_oak_log",
    "stripped_spruce_log",
    "stripped_birch_log",
    "stripped_jungle_log",
    "stripped_acacia_log",
    "stripped_cherry_log",
    "stripped_dark_oak_log",
    "stripped_pale_oak_log",
    "stripped_poplar_log",
    "stripped_mangrove_log",
    "stripped_bamboo_block",
    "stripped_oak_wood",
    "stripped_spruce_wood",
    "stripped_birch_wood",
    "stripped_jungle_wood",
    "stripped_acacia_wood",
    "stripped_cherry_wood",
    "stripped_dark_oak_wood",
    "stripped_pale_oak_wood",
    "stripped_poplar_wood",
    "stripped_mangrove_wood",
    "oak_wood",
    "spruce_wood",
    "birch_wood",
    "jungle_wood",
    "acacia_wood",
    "cherry_wood",
    "pale_oak_wood",
    "poplar_wood",
    "dark_oak_wood",
    "mangrove_wood",
    "mangrove_roots",
    "oak_leaves",
    "spruce_leaves",
    "birch_leaves",
    "jungle_leaves",
    "acacia_leaves",
    "cherry_leaves",
    "dark_oak_leaves",
    "pale_oak_leaves",
    "mangrove_leaves",
    "red_poplar_leaves",
    "orange_poplar_leaves",
    "yellow_poplar_leaves",
    "bookshelf",
    "tnt",
    "short_grass",
    "fern",
    "dead_bush",
    "short_dry_grass",
    "tall_dry_grass",
    "sunflower",
    "lilac",
    "rose_bush",
    "peony",
    "red_shrub",
    "tall_grass",
    "large_fern",
    "dandelion",
    "golden_dandelion",
    "poppy",
    "open_eyeblossom",
    "closed_eyeblossom",
    "blue_orchid",
    "allium",
    "azure_bluet",
    "red_tulip",
    "orange_tulip",
    "white_tulip",
    "pink_tulip",
    "oxeye_daisy",
    "cornflower",
    "lily_of_the_valley",
    "torchflower",
    "pitcher_plant",
    "wither_rose",
    "pink_petals",
    "wildflowers",
    "leaf_litter",
    "cactus_flower",
    "vine",
    "coal_block",
    "hay_block",
    "target",
    "pale_moss_block",
    "pale_moss_carpet",
    "pale_hanging_moss",
    "dried_kelp_block",
    "bamboo",
    "scaffolding",
    "lectern",
    "composter",
    "sweet_berry_bush",
    "beehive",
    "bee_nest",
    "azalea_leaves",
    "flowering_azalea_leaves",
    "cave_vines",
    "cave_vines_plant",
    "spore_blossom",
    "azalea",
    "flowering_azalea",
    "big_dripleaf",
    "big_dripleaf_stem",
    "small_dripleaf",
    "hanging_roots",
    "glow_lichen",
    "firefly_bush",
    "bush",
    "acacia_shelf",
    "bamboo_shelf",
    "birch_shelf",
    "cherry_shelf",
    "dark_oak_shelf",
    "jungle_shelf",
    "mangrove_shelf",
    "poplar_shelf",
    "oak_shelf",
    "pale_oak_shelf",
    "spruce_shelf",
    "straw_bed",
];

fn flammable(name: &str) -> bool {
    FLAMMABLE.contains(&name)
        || name.ends_with("_wool")
        || name.ends_with("_wool_stairs")
        || name.ends_with("_wool_slab")
        || (name.ends_with("_carpet") && !name.ends_with("moss_carpet"))
}

pub fn dir_index(name: &str) -> Option<usize> {
    Some(match name {
        "down" => 0,
        "up" => 1,
        "north" => 2,
        "south" => 3,
        "west" => 4,
        "east" => 5,
        _ => return None,
    })
}

struct Props {
    pairs: [(&'static str, &'static str); 12],
    len: usize,
}

impl Props {
    fn new(data: &'static Version, state: u32) -> Self {
        let mut pairs = [("", ""); 12];
        let mut len = 0;
        for pair in data.state_properties(state).take(12) {
            pairs[len] = pair;
            len += 1;
        }
        Self { pairs, len }
    }

    fn get(&self, key: &str) -> Option<&'static str> {
        self.pairs[..self.len]
            .iter()
            .find_map(|(k, v)| (*k == key).then_some(*v))
    }

    fn is(&self, key: &str, value: &str) -> bool {
        self.get(key) == Some(value)
    }

    fn dir(&self, key: &str) -> usize {
        self.get(key).and_then(dir_index).unwrap_or(2)
    }
}

fn trial_state(name: Option<&str>) -> TrialState {
    TrialState::from_name(name.unwrap_or("inactive")).unwrap_or(TrialState::Inactive)
}

/// Bit 0 is the signal fire, bits 1 and 2 `Direction.get2DDataValue` of the facing.
fn campfire_aux(p: &Props) -> u16 {
    let facing = match p.get("facing") {
        Some("west") => 1,
        Some("north") => 2,
        Some("east") => 3,
        _ => 0,
    };
    u16::from(p.is("signal_fire", "true")) | facing << 1
}

fn classify(name: &str, p: &Props) -> (Kind, u8, u16) {
    let lit = p.is("lit", "true");
    match name {
        "torch" => (Kind::Torch, 0, 0),
        "soul_torch" => (Kind::Torch, 1, 0),
        "copper_torch" => (Kind::Torch, 2, 0),
        "wall_torch" => (Kind::WallTorch, 0, p.dir("facing") as u16),
        "soul_wall_torch" => (Kind::WallTorch, 1, p.dir("facing") as u16),
        "copper_wall_torch" => (Kind::WallTorch, 2, p.dir("facing") as u16),
        "redstone_torch" if lit => (Kind::RedstoneTorch, 0, 0),
        "redstone_wall_torch" if lit => (Kind::RedstoneWallTorch, 0, p.dir("facing") as u16),
        n if n.ends_with("candle_cake") && lit => (Kind::CandleCake, 0, 0),
        n if n.ends_with("candle") && lit => {
            let count = p.get("candles").and_then(|c| c.parse().ok()).unwrap_or(1);
            (Kind::Candle, count, 0)
        }
        "vault" => (
            Kind::Vault,
            u8::from(p.is("ominous", "true")),
            p.dir("facing") as u16,
        ),
        "trial_spawner" => (
            Kind::TrialSpawner,
            u8::from(p.is("ominous", "true")),
            trial_state(p.get("trial_spawner_state")) as u16,
        ),
        "campfire" if lit => (Kind::Campfire, 1, campfire_aux(p)),
        "soul_campfire" if lit => (Kind::Campfire, 0, campfire_aux(p)),
        "fire" => (Kind::Fire, 0, 0),
        "soul_fire" => (Kind::SoulFire, 0, 0),
        "furnace" if lit => (Kind::Furnace, 0, p.dir("facing") as u16),
        "blast_furnace" if lit => (Kind::BlastFurnace, 0, p.dir("facing") as u16),
        "smoker" if lit => (Kind::Smoker, 0, 0),
        "brewing_stand" => (Kind::BrewingStand, 0, 0),
        "beehive" | "bee_nest"
            if p.get("honey_level")
                .and_then(|v| v.parse::<u8>().ok())
                .is_some_and(|v| v >= 5) =>
        {
            (Kind::Beehive, 0, 0)
        }
        "suspicious_sand" | "suspicious_gravel" => (Kind::Brushable, 0, 0),
        "sand" | "red_sand" => (Kind::Sand, 0, 0),
        "gravel" | "anvil" | "chipped_anvil" | "damaged_anvil" | "dragon_egg" => {
            (Kind::FallingBlock, 0, 0)
        }
        n if n.ends_with("concrete_powder") => (Kind::FallingBlock, 0, 0),
        "bubble_column" => (Kind::BubbleColumn, u8::from(p.is("drag", "true")), 0),
        "creaking_heart" if !p.is("creaking_heart_state", "uprooted") => {
            (Kind::CreakingHeart, 0, 0)
        }
        "crying_obsidian" => (Kind::CryingObsidian, 0, 0),
        "dried_ghast" => (Kind::DriedGhast, u8::from(p.is("waterlogged", "true")), 0),
        "dead_bush" => (Kind::DeadBush, 0, 0),
        "short_dry_grass" | "tall_dry_grass" => (Kind::DryGrass, 0, 0),
        "enchanting_table" => (Kind::EnchantingTable, 0, 0),
        "ender_chest" => (Kind::EnderChest, 0, 0),
        "end_gateway" => (Kind::EndGateway, 0, 0),
        "end_portal" => (Kind::EndPortal, 0, 0),
        "end_rod" => (Kind::EndRod, 0, p.dir("facing") as u16),
        "open_eyeblossom" => (Kind::Eyeblossom, 0, 0),
        "firefly_bush" => (Kind::FireflyBush, 0, 0),
        "pale_hanging_moss" => (Kind::HangingMoss, 0, 0),
        n if n.ends_with("_leaves") => match LEAVES.iter().position(|(l, _)| *l == n) {
            Some(index) => (Kind::Leaves, index as u8, 0),
            None => (Kind::None, 0, 0),
        },
        "lever" if p.is("powered", "true") => {
            let facing = p.dir("facing");
            let connected = match p.get("face") {
                Some("floor") => 1,
                Some("ceiling") => 0,
                _ => facing,
            };
            (
                Kind::Lever,
                opposite(facing) as u8,
                opposite(connected) as u16,
            )
        }
        n if n.contains("lightning_rod") => {
            let axis = match p.dir("facing") {
                0 | 1 => 1,
                2 | 3 => 2,
                _ => 0,
            };
            (Kind::LightningRod, axis, 0)
        }
        "mycelium" => (Kind::Mycelium, 0, 0),
        "nether_portal" => (Kind::NetherPortal, 0, 0),
        "pointed_dripstone"
            if p.is("thickness", "tip")
                && p.is("vertical_direction", "down")
                && !p.is("waterlogged", "true") =>
        {
            (Kind::PointedDripstone, 0, 0)
        }
        "potent_sulfur" if p.get("potent_sulfur_state").is_some_and(|s| s != "dry") => {
            (Kind::PotentSulfur, 0, 0)
        }
        "redstone_ore" | "deepslate_redstone_ore" if lit => (Kind::RedstoneOre, 0, 0),
        "redstone_wire" => {
            let power = p
                .get("power")
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(0);
            if power == 0 {
                return (Kind::None, 0, 0);
            }
            let mut sides = 0u16;
            for (i, key) in ["north", "east", "south", "west"].iter().enumerate() {
                let v = match p.get(key) {
                    Some("side") => 1,
                    Some("up") => 2,
                    _ => 0,
                };
                sides |= v << (2 * i);
            }
            (Kind::RedstoneWire, power, sides)
        }
        "repeater" if p.is("powered", "true") => {
            let delay = p
                .get("delay")
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(1);
            (Kind::Repeater, p.dir("facing") as u8, delay)
        }
        "respawn_anchor" if p.get("charges").is_some_and(|c| c != "0") => {
            (Kind::RespawnAnchor, 0, 0)
        }
        "sculk_sensor" | "calibrated_sculk_sensor" if p.is("sculk_sensor_phase", "active") => {
            (Kind::SculkSensor, 0, 0)
        }
        "spore_blossom" => (Kind::SporeBlossom, 0, 0),
        "wet_sponge" => (Kind::WetSponge, 0, 0),
        "wither_rose" => (Kind::WitherRose, 0, 0),
        _ => (Kind::None, 0, 0),
    }
}

fn fluid_class(data: &Version, fluid: mcr_data::FluidStateInfo) -> u8 {
    match data.fluid_name(fluid.fluid) {
        Some(n) if n.ends_with("lava") => fluid::LAVA,
        Some(n) if n.ends_with("water") => {
            if fluid.source {
                fluid::WATER_SOURCE
            } else if fluid.falling {
                fluid::WATER_FALLING
            } else {
                fluid::WATER_FLOWING
            }
        }
        _ => fluid::NONE,
    }
}

fn top_face_full(data: &Version, state: u32) -> bool {
    data.collision_shape(state).iter().any(|b| {
        b.min[0] <= 0.0 && b.min[2] <= 0.0 && b.max[0] >= 1.0 && b.max[2] >= 1.0 && b.max[1] >= 1.0
    })
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Known {
    pub pale_moss_block: u16,
    pub pale_oak_leaves: u16,
    pub red_sand: u16,
    pub mud: u16,
    pub pointed_dripstone: u16,
}

#[derive(Debug)]
pub struct Table {
    info: Vec<Info>,
    block_tags: Vec<u16>,
    pub known: Known,
}

impl Table {
    pub fn build(data: &'static Version) -> Self {
        let blocks = data.block_count();
        let flammable_blocks: Vec<bool> = (0..blocks as u32)
            .map(|b| {
                data.block_name(b)
                    .is_some_and(|n| flammable(n.strip_prefix("minecraft:").unwrap_or(n)))
            })
            .collect();
        let id_of = |name: &str| {
            data.block_by_name(&format!("minecraft:{name}"))
                .map_or(u16::MAX, |b| b as u16)
        };
        let known = Known {
            pale_moss_block: id_of("pale_moss_block"),
            pale_oak_leaves: id_of("pale_oak_leaves"),
            red_sand: id_of("red_sand"),
            mud: id_of("mud"),
            pointed_dripstone: id_of("pointed_dripstone"),
        };
        let fire_blocks = [id_of("fire"), id_of("soul_fire")];
        let mut info = Vec::with_capacity(data.state_count());
        for state in 0..data.state_count() as u32 {
            let Some(s) = data.block_state(state) else {
                info.push(Info::AIR);
                continue;
            };
            let name = data.state_block_name(state).unwrap_or("");
            let name = name.strip_prefix("minecraft:").unwrap_or(name);
            let props = Props::new(data, state);
            let (kind, arg, aux) = classify(name, &props);
            let block = s.block as u16;
            let mut flags = 0;
            let mut set = |on: bool, bit: u16| {
                if on {
                    flags |= bit;
                }
            };
            set(s.is_air(), flag::AIR);
            set(name == "air", flag::PLAIN_AIR);
            set(s.is_solid_render(), flag::SOLID_RENDER);
            set(s.is_full_collision(), flag::FULL_COLLISION);
            set(s.can_occlude(), flag::CAN_OCCLUDE);
            set(
                flammable_blocks[s.block as usize] && !props.is("waterlogged", "true"),
                flag::FLAMMABLE,
            );
            set(top_face_full(data, state), flag::TOP_FULL);
            set(
                s.is_air() || s.is_liquid() || s.is_replaceable() || fire_blocks.contains(&block),
                flag::FREE_TO_FALL,
            );
            set(
                block == known.pointed_dripstone && props.is("vertical_direction", "down"),
                flag::DRIP_DOWN,
            );
            let fluid = s.fluid.map_or(fluid::NONE, |f| fluid_class(data, f));
            info.push(Info {
                kind,
                arg,
                aux,
                flags,
                sturdy: s.sturdy_faces,
                fluid,
                block,
            });
        }
        Self {
            info,
            block_tags: vec![0; blocks],
            known,
        }
    }

    pub fn info(&self, state: u32) -> &Info {
        self.info.get(state as usize).unwrap_or(&Info::AIR)
    }

    /// Whether the state is a lit campfire, and whether its signal fire is on.
    pub fn campfire(&self, state: u32) -> Option<bool> {
        let info = self.info(state);
        (info.kind == Kind::Campfire).then_some(info.aux & 1 != 0)
    }

    /// `Direction.get2DDataValue` (south 0, west 1, north 2, east 3) of a lit campfire's facing.
    pub fn campfire_rotation(&self, state: u32) -> Option<u8> {
        let info = self.info(state);
        (info.kind == Kind::Campfire).then_some((info.aux >> 1 & 3) as u8)
    }

    /// A vault state's keyhole direction (an index into [`DIRS`]) and whether it is ominous.
    pub fn vault(&self, state: u32) -> Option<(usize, bool)> {
        let info = self.info(state);
        (info.kind == Kind::Vault).then_some((usize::from(info.aux), info.arg != 0))
    }

    /// A trial spawner state's `trial_spawner_state` and whether it is ominous.
    pub fn trial_spawner(&self, state: u32) -> Option<(TrialState, bool)> {
        let info = self.info(state);
        (info.kind == Kind::TrialSpawner).then(|| (TrialState::from_index(info.aux), info.arg != 0))
    }

    pub fn states(&self) -> usize {
        self.info.len()
    }

    pub fn set_tags(&mut self, in_tag: &dyn Fn(&str, u32) -> bool) {
        for (block, bits) in self.block_tags.iter_mut().enumerate() {
            *bits = tag::ALL
                .iter()
                .filter(|(_, name)| in_tag(name, block as u32))
                .fold(0, |acc, (bit, _)| acc | bit);
        }
    }

    pub fn tagged(&self, info: &Info, bit: u16) -> bool {
        self.block_tags
            .get(usize::from(info.block))
            .is_some_and(|t| t & bit != 0)
    }
}
