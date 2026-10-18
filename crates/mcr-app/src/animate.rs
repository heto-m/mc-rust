//! Vanilla's `ClientLevel.animateTick` for the client state: ambient block, fluid and biome particles and
//! the sounds that go with them, from `mcr-animate`.

use std::sync::Arc;

use mcr_animate::{
    AnimateWorld, Animator, ConduitTarget, Cue, Height, MAX_PARTICLES_PER_TICK,
    MAX_SOUNDS_PER_TICK, Sink, VaultPlayer, Visual, tag,
};
use mcr_client::{ClientState, RegistryItem};
use mcr_mesh::BiomeColors;
use mcr_protocol::types::Particle;
use mcr_protocol::types::game::GameType;
use mcr_render::{Precipitation, WeatherSource};
use mcr_world::{ChunkPos, HeightmapKind};

use uuid::Uuid;

use crate::ambience::biome_at;
use crate::assets::Assets;
use crate::audio::Audio;
use crate::block_entities::Ticker;
use crate::effects::{ClientWeather, WorldEffects, light_at, precipitation_table};

const BIOME: &str = "minecraft:worldgen/biome";
const DIMENSION_TYPE: &str = "minecraft:dimension_type";
const BLOCK_REGISTRY: &str = "minecraft:block";
const MAX_TICKERS: usize = 1024;
const DAY_PERIOD: i64 = 24_000;
const CREAKING_ACTIVE_FROM: i64 = 12_600;
const CREAKING_ACTIVE_UNTIL: i64 = 23_401;
const CREAKING_DIMENSIONS: [&str; 2] = ["minecraft:overworld", "minecraft:overworld_caves"];
const RAIN_THRESHOLD: f32 = 0.2;
const THUNDER_THRESHOLD: f32 = 0.9;
const MISSING_HEIGHT: i32 = i32::MIN / 2;

/// The `gameplay/creaking_active` keyframes of the overworld day timeline.
fn creaking_active_at(day_time: i64) -> bool {
    (CREAKING_ACTIVE_FROM..CREAKING_ACTIVE_UNTIL).contains(&day_time.rem_euclid(DAY_PERIOD))
}

const MARKER_ITEMS: [&str; 2] = ["minecraft:barrier", "minecraft:light"];

/// `ClientLevel.getMarkerParticleTarget`: creative mode holding a barrier or light shows that block.
fn marker_target(creative: bool, held: Option<&str>) -> Option<&'static str> {
    let held = held?;
    MARKER_ITEMS
        .iter()
        .copied()
        .find(|m| creative && *m == held)
}

pub struct BlockAnimation {
    animator: Animator,
    assets: Arc<Assets>,
    dimension_type: Option<i32>,
    dimension_name: String,
    biome_count: usize,
    dimension: Visual,
    biomes: Vec<Visual>,
    precipitation: Vec<(bool, f32)>,
    colors: Option<Arc<BiomeColors>>,
    creaking_dimension: bool,
    any_ambient: bool,
    tag_signature: usize,
}

impl std::fmt::Debug for BlockAnimation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockAnimation").finish_non_exhaustive()
    }
}

struct Out<'a> {
    effects: &'a mut WorldEffects,
    audio: Option<&'a mut Audio>,
}

impl Sink for Out<'_> {
    fn particle(&mut self, particle: &Particle, pos: [f64; 3], vel: [f64; 3], always: bool) {
        self.effects
            .add_particle_vanilla(particle, pos, vel, always);
    }

    fn sound(&mut self, cue: &Cue) {
        if let Some(audio) = self.audio.as_deref_mut() {
            audio.play(cue.event, cue.pos, cue.volume, cue.pitch, cue.source);
        }
    }
}

/// One particle and sound budget for everything a tick emits, whichever scan produced it.
struct Capped<'a> {
    inner: &'a mut dyn Sink,
    particles: u32,
    sounds: u32,
}

impl<'a> Capped<'a> {
    fn new(inner: &'a mut dyn Sink) -> Self {
        Self {
            inner,
            particles: 0,
            sounds: 0,
        }
    }
}

impl Sink for Capped<'_> {
    fn particle(&mut self, particle: &Particle, pos: [f64; 3], vel: [f64; 3], always: bool) {
        if self.particles < MAX_PARTICLES_PER_TICK {
            self.particles += 1;
            self.inner.particle(particle, pos, vel, always);
        }
    }

    fn sound(&mut self, cue: &Cue) {
        if self.sounds < MAX_SOUNDS_PER_TICK {
            self.sounds += 1;
            self.inner.sound(cue);
        }
    }
}

struct View<'a> {
    state: &'a ClientState,
    weather: ClientWeather<'a>,
    dimension: &'a Visual,
    biomes: &'a [Visual],
    colors: Option<&'a BiomeColors>,
    creaking: bool,
    any_ambient: bool,
    sky_darken: u8,
}

impl View<'_> {
    fn visual(&self, x: i32, y: i32, z: i32) -> &Visual {
        biome_at(self.state, [x, y, z])
            .and_then(|b| self.biomes.get(b))
            .unwrap_or(self.dimension)
    }
}

impl AnimateWorld for View<'_> {
    fn state(&self, x: i32, y: i32, z: i32) -> Option<u16> {
        self.state.chunks.block(x, y, z).map(|s| s.0)
    }

    fn light(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        light_at(&self.state.chunks, x, y, z).unwrap_or((0, 15))
    }

    fn raining_at(&self, x: i32, y: i32, z: i32) -> bool {
        self.state.world.rain_level > RAIN_THRESHOLD
            && self.weather.height(x, z) <= y
            && self.weather.precipitation(x, y, z) == Precipitation::Rain
    }

    fn thundering(&self) -> bool {
        self.state.world.thunder_level > THUNDER_THRESHOLD
    }

    fn game_time(&self) -> i64 {
        self.state.world.game_time
    }

    fn height(&self, kind: Height, x: i32, z: i32) -> i32 {
        let chunks = &self.state.chunks;
        let kind = match kind {
            Height::WorldSurface => HeightmapKind::WorldSurface,
            Height::MotionBlockingNoLeaves => HeightmapKind::MotionBlockingNoLeaves,
        };
        chunks
            .column(ChunkPos::new(x >> 4, z >> 4))
            .and_then(|c| c.heightmaps().get(kind))
            .map_or(MISSING_HEIGHT, |h| {
                h.first_available(
                    (x & 15) as usize,
                    (z & 15) as usize,
                    chunks.config().min_y(),
                )
            })
    }

    fn sky_darken(&self) -> u8 {
        self.sky_darken
    }

    fn foliage_color(&self, x: i32, y: i32, z: i32) -> u32 {
        let biome = biome_at(self.state, [x, y, z]).unwrap_or(0);
        self.colors.map_or(0x48B518, |c| c.foliage_rgb(biome))
    }

    fn creaking_active(&self) -> bool {
        self.creaking && self.state.world.day_time().is_some_and(creaking_active_at)
    }

    fn has_ambient_particles(&self) -> bool {
        self.any_ambient
    }

    fn ambient_particles(&self, x: i32, y: i32, z: i32) -> &[(Particle, f32)] {
        &self.visual(x, y, z).ambient
    }

    fn dripstone_particle(&self, x: i32, y: i32, z: i32) -> Option<&Particle> {
        self.visual(x, y, z).dripstone.as_ref()
    }

    fn water_evaporates(&self, x: i32, y: i32, z: i32) -> bool {
        self.visual(x, y, z).water_evaporates.unwrap_or(false)
    }

    fn firefly_sounds(&self, x: i32, y: i32, z: i32) -> bool {
        self.visual(x, y, z).firefly_sounds.unwrap_or(false)
    }
}

impl BlockAnimation {
    pub fn new(data: &'static mcr_data::Version, assets: Arc<Assets>) -> Self {
        Self {
            animator: Animator::new(data, 0xA11_CE5ED),
            assets,
            dimension_type: None,
            dimension_name: String::new(),
            biome_count: usize::MAX,
            dimension: Visual::default(),
            biomes: Vec::new(),
            precipitation: Vec::new(),
            colors: None,
            creaking_dimension: false,
            any_ambient: false,
            tag_signature: usize::MAX,
        }
    }

    fn entry_visual(entry: &RegistryItem) -> Visual {
        entry
            .data
            .as_ref()
            .map(Visual::from_registry_entry)
            .unwrap_or_default()
    }

    fn refresh(&mut self, state: &ClientState) {
        self.refresh_tags(state);
        let type_id = state.dimension.type_id;
        let name = state.dimension.name.as_str();
        let biomes = state.registries.get(BIOME);
        let count = biomes.map_or(0, |r| r.len());
        let dimension_changed = self.dimension_type != Some(type_id) || self.dimension_name != name;
        if !dimension_changed && count == self.biome_count {
            return;
        }
        let dimension_entry = state
            .registries
            .get(DIMENSION_TYPE)
            .and_then(|r| r.get(type_id));
        self.dimension = dimension_entry.map(Self::entry_visual).unwrap_or_default();
        self.creaking_dimension =
            dimension_entry.is_some_and(|e| CREAKING_DIMENSIONS.contains(&e.id.as_str()));
        if count != self.biome_count {
            self.precipitation = biomes.map(precipitation_table).unwrap_or_default();
            let names: Vec<String> = biomes
                .map(|r| r.entries.iter().map(|e| e.id.to_string()).collect())
                .unwrap_or_default();
            self.colors = Some(self.assets.biome_colors(&names));
        }
        self.biomes = biomes
            .map(|r| {
                r.entries
                    .iter()
                    .map(|e| Self::entry_visual(e).over(&self.dimension))
                    .collect()
            })
            .unwrap_or_default();
        self.any_ambient =
            !self.dimension.ambient.is_empty() || self.biomes.iter().any(|b| !b.ambient.is_empty());
        self.biome_count = count;
        self.dimension_type = Some(type_id);
        self.dimension_name = name.to_owned();
    }

    fn refresh_tags(&mut self, state: &ClientState) {
        let signature = tag::ALL
            .iter()
            .map(|(_, name)| {
                state
                    .registries
                    .tag(BLOCK_REGISTRY, name)
                    .map_or(0, |ids| ids.len() + 1)
            })
            .sum();
        if signature != self.tag_signature {
            self.tag_signature = signature;
            self.animator.table_mut().set_tags(&|name, block| {
                state
                    .registries
                    .is_in_tag(BLOCK_REGISTRY, name, block as i32)
            });
        }
    }

    /// One client tick. Sounds are skipped without `audio` (Java owns them in mod mode).
    pub fn tick(
        &mut self,
        state: &ClientState,
        effects: &mut WorldEffects,
        audio: Option<&mut Audio>,
        tickers: &mut dyn Iterator<Item = Ticker<'_>>,
        sky_darken: u8,
    ) {
        if !state.player.loaded {
            return;
        }
        self.refresh(state);
        let view = View {
            state,
            weather: ClientWeather::new(state, &self.precipitation),
            dimension: &self.dimension,
            biomes: &self.biomes,
            colors: self.colors.as_deref(),
            creaking: self.creaking_dimension,
            any_ambient: self.any_ambient,
            sky_darken,
        };
        let held = state.held_stack();
        let held_name = (held.count > 0)
            .then(|| u32::try_from(held.item).ok())
            .flatten()
            .and_then(|id| self.animator.data().item_name(id));
        let creative = state.player.game_mode == GameType::Creative;
        let marker = marker_target(creative, held_name).and_then(|name| {
            let data = self.animator.data();
            data.block_by_name(name).and_then(|b| u16::try_from(b).ok())
        });
        self.animator.set_marker(marker);
        let sounds = audio.is_some();
        let mut sink = Out { effects, audio };
        let mut out = Capped::new(&mut sink);
        let center = state.player.pos().map(|v| v.floor() as i32);
        self.animator.tick(center, &view, &mut out, sounds);
        for ticker in tickers.take(MAX_TICKERS) {
            tick_block_entity(&mut self.animator, state, &ticker, &view, &mut out, sounds);
        }
    }
}

fn tick_block_entity(
    animator: &mut Animator,
    state: &ClientState,
    ticker: &Ticker<'_>,
    view: &View<'_>,
    out: &mut dyn Sink,
    sounds: bool,
) {
    let block_at = |pos: [i32; 3]| state.chunks.block(pos[0], pos[1], pos[2]).map(|b| b.0);
    match *ticker {
        Ticker::Campfire { pos, slots } => {
            let Some(block) = block_at(pos) else { return };
            let table = animator.table();
            if let (Some(signal), Some(rotation)) =
                (table.campfire(block), table.campfire_rotation(block))
            {
                animator.campfire_tick(pos, signal, view, out);
                animator.campfire_food_tick(pos, rotation, slots, view, out);
            }
        }
        Ticker::Spawner {
            pos,
            range,
            has_entity,
        } => {
            if has_entity && player_within(state, pos, f64::from(range)) {
                animator.spawner_tick(pos, view, out);
            }
        }
        Ticker::TrialSpawner { pos } => {
            let Some(block) = block_at(pos) else { return };
            if let Some((trial, ominous)) = animator.table().trial_spawner(block) {
                animator.trial_spawner_tick(pos, trial, ominous, view, out, sounds);
            }
        }
        Ticker::Vault {
            pos,
            has_item,
            connected,
            range_tenths,
        } => {
            let Some(block) = block_at(pos) else { return };
            if let Some((facing, ominous)) = animator.table().vault(block) {
                let players: Vec<VaultPlayer> = connected
                    .iter()
                    .filter_map(|uuid| vault_player(state, *uuid))
                    .collect();
                animator.vault_tick(
                    pos,
                    facing,
                    ominous,
                    has_item,
                    &players,
                    f32::from(range_tenths) / 10.0,
                    view,
                    out,
                    sounds,
                );
            }
        }
        Ticker::Conduit {
            pos,
            ticks,
            frame,
            target,
        } => {
            let target = target.and_then(|uuid| conduit_target(state, uuid));
            animator.conduit_tick(pos, ticks, frame, target, view, out);
        }
    }
}

const PLAYER_HEIGHT: f64 = 1.8;

/// `Level.hasNearbyAlivePlayer` over the local player and the player entities we know of.
fn player_within(state: &ClientState, pos: [i32; 3], range: f64) -> bool {
    let at = pos.map(|c| f64::from(c) + 0.5);
    let near = |p: [f64; 3]| (0..3).map(|i| (p[i] - at[i]).powi(2)).sum::<f64>() < range * range;
    if !state.player.dead && near(state.player.pos()) {
        return true;
    }
    let data = mcr_data::v26_3();
    let Some(player) = data.entity_type_by_name("minecraft:player") else {
        return false;
    };
    state
        .entities
        .iter()
        .any(|e| e.kind == player as i32 && near(e.pos))
}

fn vault_player(state: &ClientState, uuid: Uuid) -> Option<VaultPlayer> {
    let (pos, height) = if uuid == state.uuid {
        (state.player.pos(), PLAYER_HEIGHT)
    } else {
        let e = state.entities.by_uuid(uuid)?;
        let (_, h) = entity_size(e.kind);
        (e.pos, f64::from(h))
    };
    Some(VaultPlayer {
        block: pos.map(|c| c.floor() as i32),
        center: [pos[0], pos[1] + height / 2.0, pos[2]],
    })
}

fn conduit_target(state: &ClientState, uuid: Uuid) -> Option<ConduitTarget> {
    let e = state.entities.by_uuid(uuid)?;
    let info = u32::try_from(e.kind)
        .ok()
        .and_then(|k| mcr_data::v26_3().entity_type_info(k))?;
    Some(ConduitTarget {
        eye: [e.pos[0], e.pos[1] + f64::from(info.eye_height), e.pos[2]],
        width: info.width,
        height: info.height,
    })
}

fn entity_size(kind: i32) -> (f32, f32) {
    u32::try_from(kind)
        .ok()
        .and_then(|k| mcr_data::v26_3().entity_type_info(k))
        .map_or((0.6, 1.8), |i| (i.width, i.height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Count(u32, u32);

    impl Sink for Count {
        fn particle(&mut self, _: &Particle, _: [f64; 3], _: [f64; 3], _: bool) {
            self.0 += 1;
        }

        fn sound(&mut self, _: &Cue) {
            self.1 += 1;
        }
    }

    struct Void;

    impl AnimateWorld for Void {
        fn state(&self, _: i32, _: i32, _: i32) -> Option<u16> {
            None
        }
    }

    #[test]
    fn one_budget_covers_the_scan_and_hundreds_of_campfires() {
        let data = mcr_data::v26_3();
        let mut animator = Animator::new(data, 1);
        let mut count = Count::default();
        let mut out = Capped::new(&mut count);
        for i in 0..5000 {
            animator.campfire_tick([i, 0, 0], true, &Void, &mut out);
            animator.campfire_food_tick([i, 0, 0], 0, [true; 4], &Void, &mut out);
        }
        for _ in 0..200 {
            out.sound(&Cue {
                event: "x",
                pos: None,
                volume: 1.0,
                pitch: 1.0,
                source: mcr_audio::SoundSource::Blocks,
            });
        }
        assert!(count.0 <= MAX_PARTICLES_PER_TICK && count.0 > 0);
        assert_eq!(count.1, MAX_SOUNDS_PER_TICK);
    }

    #[test]
    fn extreme_coordinates_are_ignored_not_overflowed() {
        let mut animator = Animator::new(mcr_data::v26_3(), 1);
        let mut count = Count::default();
        for x in [i32::MIN, i32::MAX, 30_000_001] {
            let p = [x, x, x];
            animator.tick(p, &Void, &mut count, true);
            animator.campfire_tick(p, false, &Void, &mut count);
            animator.campfire_food_tick(p, 3, [true; 4], &Void, &mut count);
            animator.spawner_tick(p, &Void, &mut count);
            animator.conduit_tick([0; 3], 0, &[p; 4], None, &Void, &mut count);
        }
        assert_eq!(count.0, 0);
    }

    #[test]
    fn markers_need_creative_mode_and_a_barrier_or_light() {
        assert_eq!(
            marker_target(true, Some("minecraft:barrier")),
            Some("minecraft:barrier")
        );
        assert_eq!(
            marker_target(true, Some("minecraft:light")),
            Some("minecraft:light")
        );
        assert_eq!(marker_target(false, Some("minecraft:barrier")), None);
        assert_eq!(marker_target(true, Some("minecraft:stone")), None);
        assert_eq!(marker_target(true, None), None);
    }

    #[test]
    fn creaking_is_active_from_dusk_until_just_before_dawn() {
        assert!(!creaking_active_at(12_599));
        assert!(creaking_active_at(12_600));
        assert!(creaking_active_at(23_400));
        assert!(!creaking_active_at(23_401));
        assert!(creaking_active_at(24_000 + 18_000));
        assert!(!creaking_active_at(6_000));
    }

    #[test]
    fn nether_biome_files_carry_their_ambient_particles() {
        let jar = std::path::Path::new("../../.cache/versions/26.3/client.jar");
        if !jar.exists() {
            return;
        }
        let assets = Assets::load(jar).unwrap();
        let ambient = |biome: &str| {
            let entry = assets
                .builtin
                .get("minecraft:worldgen/biome", biome)
                .unwrap_or_else(|| panic!("{biome} missing from the jar"));
            Visual::from_registry_entry(&entry)
        };
        let names = mcr_particles::PARTICLE_NAMES;
        for (biome, particle) in [
            ("soul_sand_valley", "ash"),
            ("basalt_deltas", "white_ash"),
            ("crimson_forest", "crimson_spore"),
            ("warped_forest", "warped_spore"),
        ] {
            let list = ambient(biome).ambient;
            assert_eq!(list.len(), 1, "{biome}");
            let Particle::Simple(id) = list[0].0 else {
                panic!("{biome}")
            };
            assert_eq!(names[id as usize], particle);
            assert!(list[0].1 > 0.0 && list[0].1 < 1.0);
        }
        assert!(ambient("plains").ambient.is_empty());
    }
}
