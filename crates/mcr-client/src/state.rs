use std::collections::VecDeque;

use mcr_buf::{BlockPos, Identifier};
use mcr_data::Version;
use mcr_nbt::Compound;
use mcr_physics::{Attributes, Effects};
use mcr_protocol::common::{ResourcePackAction, ResourcePackResponse, ServerLink};
use mcr_protocol::dispatch::PlayServerbound;
use mcr_protocol::types::Component;
use mcr_world::{BlockState, ChunkStore, WorldConfig};
use rustc_hash::FxHashMap;
use uuid::Uuid;

use crate::batch::ChunkBatchSizeCalculator;
use crate::config::{ClientConfig, ResourcePackPolicy};
use crate::entity::Entities;
use crate::error::{ClientError, Result};
use crate::events::ClientEvent;
use crate::inventory::Inventory;
use crate::player::{LocalPlayer, SPRINT_MODIFIER_ID};
use crate::registry::{BIOME, DIMENSION_TYPE, Dimension, Registries, dimension_shape};
use crate::social::{BossBar, ChatState, PlayerList, Scoreboard, TabList, Titles};

pub const MAX_COOKIES: usize = 64;
pub const MAX_COOKIE_BYTES: usize = 5120;
pub const MAX_SERVER_LINKS: usize = 64;
pub const MAX_BLOCK_ENTITIES: usize = 1 << 18;

/// What applying a packet produced: events for the UI and packets to send back.
#[derive(Debug, Default)]
pub struct Outbox {
    pub events: VecDeque<ClientEvent>,
    pub replies: Vec<PlayServerbound<'static>>,
    pub cookie_responses: Vec<(Identifier, Option<Vec<u8>>)>,
}

impl Outbox {
    pub fn event(&mut self, event: ClientEvent) {
        self.events.push_back(event);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cooldown {
    pub ends_at: u64,
    pub total: i32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ClockState {
    pub ticks: f64,
    pub rate: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldBorder {
    pub center_x: f64,
    pub center_z: f64,
    pub size: f64,
    pub target_size: f64,
    pub lerp_ticks_left: i64,
    pub lerp_total_ticks: i64,
    pub absolute_max_size: i32,
    pub warning_blocks: i32,
    pub warning_time: i32,
}

impl Default for WorldBorder {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_z: 0.0,
            size: 5.9999968e7,
            target_size: 5.9999968e7,
            lerp_ticks_left: 0,
            lerp_total_ticks: 0,
            absolute_max_size: 29_999_984,
            warning_blocks: 5,
            warning_time: 15,
        }
    }
}

impl WorldBorder {
    pub fn set_lerp(&mut self, from: f64, to: f64, millis: i64) {
        self.size = from;
        self.target_size = to;
        let ticks = (millis / 50).clamp(0, i64::from(i32::MAX));
        self.lerp_total_ticks = ticks;
        self.lerp_ticks_left = ticks;
        if ticks == 0 {
            self.size = to;
        }
    }

    pub fn tick(&mut self) {
        if self.lerp_ticks_left > 0 {
            let step = (self.target_size - self.size) / self.lerp_ticks_left as f64;
            self.size += step;
            self.lerp_ticks_left -= 1;
            if self.lerp_ticks_left == 0 {
                self.size = self.target_size;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpawnPoint {
    pub dimension: Identifier,
    pub pos: BlockPos,
    pub yaw: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone)]
pub struct WorldInfo {
    pub game_time: i64,
    pub clocks: FxHashMap<i32, ClockState>,
    pub raining: bool,
    pub rain_level: f32,
    pub thunder_level: f32,
    pub border: WorldBorder,
    pub spawn: Option<SpawnPoint>,
    pub tick_rate: f32,
    pub frozen: bool,
    pub frozen_ticks_to_run: i32,
    pub difficulty: u8,
    pub difficulty_locked: bool,
    pub hardcore: bool,
    pub view_distance: i32,
    pub simulation_distance: i32,
    pub chunk_center: (i32, i32),
}

impl Default for WorldInfo {
    fn default() -> Self {
        Self {
            game_time: 0,
            clocks: FxHashMap::default(),
            raining: false,
            rain_level: 0.0,
            thunder_level: 0.0,
            border: WorldBorder::default(),
            spawn: None,
            tick_rate: 20.0,
            frozen: false,
            frozen_ticks_to_run: 0,
            difficulty: 2,
            difficulty_locked: false,
            hardcore: false,
            view_distance: 2,
            simulation_distance: 2,
            chunk_center: (0, 0),
        }
    }
}

impl WorldInfo {
    /// Day time of the first clock (the overworld clock in vanilla), in ticks.
    pub fn day_time(&self) -> Option<i64> {
        self.clocks
            .iter()
            .min_by_key(|(id, _)| **id)
            .map(|(_, c)| c.ticks as i64)
    }

    pub fn tick(&mut self) {
        self.border.tick();
        let running = !self.frozen || self.frozen_ticks_to_run > 0;
        if self.frozen && self.frozen_ticks_to_run > 0 {
            self.frozen_ticks_to_run -= 1;
        }
        if running {
            self.game_time += 1;
            for clock in self.clocks.values_mut() {
                clock.ticks += f64::from(clock.rate);
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ServerInfo {
    pub brand: Option<String>,
    pub motd: Option<Component>,
    pub links: Vec<ServerLink>,
    pub cookies: FxHashMap<String, Vec<u8>>,
    pub game_rules: FxHashMap<String, String>,
    pub online_mode: bool,
    pub enforces_secure_chat: bool,
    pub max_players: i32,
    pub reduced_debug_info: bool,
    pub do_limited_crafting: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PhysicsIds {
    pub movement_speed: Option<i32>,
    pub jump_strength: Option<i32>,
    pub step_height: Option<i32>,
    pub gravity: Option<i32>,
    pub sneaking_speed: Option<i32>,
    pub water_movement_efficiency: Option<i32>,
    pub movement_efficiency: Option<i32>,
    pub jump_boost: Option<i32>,
    pub levitation: Option<i32>,
    pub slow_falling: Option<i32>,
}

impl PhysicsIds {
    fn resolve(data: &Version) -> Self {
        let attr = |name: &str| {
            data.registry("minecraft:attribute")
                .and_then(|r| r.id_of(&format!("minecraft:{name}")))
                .map(|id| id as i32)
        };
        let effect = |name: &str| {
            data.registry("minecraft:mob_effect")
                .and_then(|r| r.id_of(&format!("minecraft:{name}")))
                .map(|id| id as i32)
        };
        Self {
            movement_speed: attr("movement_speed"),
            jump_strength: attr("jump_strength"),
            step_height: attr("step_height"),
            gravity: attr("gravity"),
            sneaking_speed: attr("sneaking_speed"),
            water_movement_efficiency: attr("water_movement_efficiency"),
            movement_efficiency: attr("movement_efficiency"),
            jump_boost: effect("jump_boost"),
            levitation: effect("levitation"),
            slow_falling: effect("slow_falling"),
        }
    }
}

/// A `command_suggestions` reply; `start` and `length` are character offsets into the request.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandReply {
    pub id: i32,
    pub start: usize,
    pub length: usize,
    pub items: Vec<(String, Option<String>)>,
}

#[derive(Debug)]
pub struct ClientState {
    pub data: &'static Version,
    pub name: String,
    pub uuid: Uuid,
    pub registries: Registries,
    pub dimension: Dimension,
    pub chunks: ChunkStore,
    pub block_entities: FxHashMap<BlockPos, (i32, Compound)>,
    pub entities: Entities,
    pub player: LocalPlayer,
    pub inventory: Inventory,
    pub world: WorldInfo,
    pub chat: ChatState,
    pub players: PlayerList,
    pub tab_list: TabList,
    pub titles: Titles,
    pub boss_bars: FxHashMap<Uuid, BossBar>,
    pub scoreboard: Scoreboard,
    pub server: ServerInfo,
    pub last_command_suggestions: Option<(i32, Vec<String>)>,
    pub commands: crate::commands::CommandTree,
    pub command_reply: Option<CommandReply>,
    pub block_ack_sequence: i32,
    pub predictions: crate::digging::BlockPredictions,
    pub cooldowns: FxHashMap<String, Cooldown>,
    pub gameplay: crate::gamestate::GameplayState,
    pub game_ticks: u64,
    pub controlled_vehicle: Option<i32>,
    pub vehicle_corrections: u32,
    pub chunk_batches: ChunkBatchSizeCalculator,
    pub chunks_received: u64,
    pub unhandled: FxHashMap<&'static str, u64>,
    pub(crate) ids: PhysicsIds,
    pub(crate) resource_packs: ResourcePackPolicy,
    pub(crate) in_batch: bool,
}

pub fn default_world_config(
    data: &Version,
    biomes: u32,
    min_y: i32,
    height: u32,
) -> Result<WorldConfig> {
    let air = ["air", "cave_air", "void_air"].map(|name| {
        data.block_by_name(&format!("minecraft:{name}"))
            .and_then(|b| data.block_info(b))
            .map_or(BlockState::AIR, |info| {
                BlockState(info.default_state as u16)
            })
    });
    Ok(WorldConfig::new(data.state_count() as u32, biomes.max(1), min_y, height)?.with_air(air))
}

impl ClientState {
    /// Remaining fraction of the cooldown covering the stack, if any.
    pub fn cooldown_progress(&self, stack: &mcr_protocol::types::ItemStack) -> Option<f32> {
        let key = self.cooldown_group(stack)?;
        let c = self.cooldowns.get(&key)?;
        let left = c.ends_at.checked_sub(self.game_ticks)?;
        (left > 0).then(|| left as f32 / c.total.max(1) as f32)
    }

    pub fn is_on_cooldown(&self, stack: &mcr_protocol::types::ItemStack) -> bool {
        self.cooldown_progress(stack).is_some()
    }

    fn cooldown_group(&self, stack: &mcr_protocol::types::ItemStack) -> Option<String> {
        use mcr_protocol::types::component::DataComponent;
        if stack.is_empty() {
            return None;
        }
        for c in &stack.components.added {
            if let DataComponent::UseCooldown(u) = c
                && let Some(g) = &u.cooldown_group
            {
                return Some(g.as_str().to_owned());
            }
        }
        let name = self.data.item_name(u32::try_from(stack.item).ok()?)?;
        Some(name.to_owned())
    }

    pub fn new(config: &ClientConfig, uuid: Uuid) -> Self {
        let data = mcr_data::v26_3();
        let chunks = ChunkStore::new(
            default_world_config(data, 1, -64, 384).expect("default world config is valid"),
        );
        Self {
            data,
            name: config.name.clone(),
            uuid,
            registries: Registries::default(),
            dimension: Dimension::default(),
            chunks,
            block_entities: FxHashMap::default(),
            entities: Entities::default(),
            player: LocalPlayer::default(),
            inventory: Inventory::default(),
            world: WorldInfo::default(),
            chat: ChatState::with_capacity(config.chat_history),
            players: PlayerList::default(),
            tab_list: TabList::default(),
            titles: Titles::default(),
            boss_bars: FxHashMap::default(),
            scoreboard: Scoreboard::default(),
            server: ServerInfo::default(),
            last_command_suggestions: None,
            commands: crate::commands::CommandTree::default(),
            command_reply: None,
            block_ack_sequence: 0,
            predictions: crate::digging::BlockPredictions::default(),
            cooldowns: FxHashMap::default(),
            gameplay: crate::gamestate::GameplayState::default(),
            game_ticks: 0,
            controlled_vehicle: None,
            vehicle_corrections: 0,
            chunk_batches: ChunkBatchSizeCalculator::default(),
            chunks_received: 0,
            unhandled: FxHashMap::default(),
            ids: PhysicsIds::resolve(data),
            resource_packs: config.resource_packs,
            in_batch: false,
        }
    }

    pub(crate) fn build_world(&mut self, dimension: Dimension) -> Result<()> {
        let (min_y, height, logical) = dimension_shape(&self.registries, dimension.type_id)
            .ok_or_else(|| {
                ClientError::Invalid(format!("unknown dimension type {}", dimension.type_id))
            })?;
        let biomes = self.registries.get(BIOME).map_or(1, |r| r.len() as u32);
        let height = u32::try_from(height)
            .map_err(|_| ClientError::Invalid(format!("dimension height {height}")))?;
        let cfg = default_world_config(self.data, biomes, min_y, height)?;
        self.chunks = ChunkStore::new(cfg);
        self.block_entities.clear();
        self.entities.clear();
        self.dimension = Dimension {
            min_y,
            height: height as i32,
            logical_height: logical,
            ..dimension
        };
        Ok(())
    }

    pub fn dimension_type_name(&self) -> Option<&str> {
        self.registries
            .get(DIMENSION_TYPE)?
            .get(self.dimension.type_id)
            .map(|i| i.id.as_str())
    }

    /// Chunk radius the client keeps loaded: vanilla's `max(2, radius) + 3` storage range.
    pub fn chunk_storage_radius(&self) -> i32 {
        self.world.view_distance.max(2) + 3
    }

    pub fn chunk_in_range(&self, x: i32, z: i32) -> bool {
        let r = self.chunk_storage_radius();
        let (cx, cz) = self.world.chunk_center;
        (x - cx).abs() <= r && (z - cz).abs() <= r
    }

    pub fn tick(&mut self) {
        self.game_ticks += 1;
        let now = self.game_ticks;
        self.cooldowns.retain(|_, c| c.ends_at > now);
        self.entities.tick();
        self.world.tick();
    }

    pub(crate) fn refresh_physics_inputs(&mut self) {
        let ids = self.ids;
        let value = |id: Option<i32>, skip: Option<&str>| {
            let a = self.player.attributes.get(&id?)?;
            Some(a.value(skip))
        };
        let defaults = Attributes::default();
        let attributes = Attributes {
            movement_speed: value(ids.movement_speed, Some(SPRINT_MODIFIER_ID))
                .unwrap_or(f64::from(self.player.walking_speed)),
            jump_strength: value(ids.jump_strength, None).unwrap_or(defaults.jump_strength),
            step_height: value(ids.step_height, None).unwrap_or(defaults.step_height),
            gravity: value(ids.gravity, None).unwrap_or(defaults.gravity),
            sneaking_speed: value(ids.sneaking_speed, None).unwrap_or(defaults.sneaking_speed),
            water_movement_efficiency: value(ids.water_movement_efficiency, None)
                .unwrap_or(defaults.water_movement_efficiency),
            movement_efficiency: value(ids.movement_efficiency, None)
                .unwrap_or(defaults.movement_efficiency),
            ..defaults
        };
        let level = |id: Option<i32>| {
            let e = self.player.effects.get(&id?)?;
            u8::try_from(e.amplifier).ok()
        };
        let effects = Effects {
            jump_boost: level(ids.jump_boost),
            levitation: level(ids.levitation),
            slow_falling: ids
                .slow_falling
                .is_some_and(|id| self.player.effects.contains_key(&id)),
        };
        self.player.apply_attributes(attributes);
        self.player.apply_effects(effects);
    }
}

pub fn resource_pack_responses(policy: ResourcePackPolicy, id: Uuid) -> Vec<ResourcePackResponse> {
    let actions: &[ResourcePackAction] = match policy {
        ResourcePackPolicy::Decline => &[ResourcePackAction::Declined],
        ResourcePackPolicy::Accept => &[
            ResourcePackAction::Accepted,
            ResourcePackAction::SuccessfullyLoaded,
        ],
    };
    actions
        .iter()
        .map(|&action| ResourcePackResponse { id, action })
        .collect()
}

impl ClientState {
    /// Drops world and entity state ahead of a re-configuration; registries are replaced by the server.
    pub fn reset_for_reconfiguration(&mut self) {
        let cfg = *self.chunks.config();
        self.chunks = ChunkStore::new(cfg);
        self.block_entities.clear();
        self.entities.clear();
        self.boss_bars.clear();
        self.inventory.open = None;
        self.player.loaded = false;
        self.player.placed = false;
        self.in_batch = false;
    }
}
