//! Vanilla's `ClientLevel.animateTick` for the client state: ambient block, fluid and biome particles and
//! the sounds that go with them, from `mcr-animate`.

use std::sync::Arc;

use mcr_animate::{AnimateWorld, Animator, Cue, Height, Sink, Visual, tag};
use mcr_audio::registry_json;
use mcr_client::{ClientState, RegistryItem};
use mcr_mesh::BiomeColors;
use mcr_protocol::types::Particle;
use mcr_render::{Precipitation, WeatherSource};
use mcr_world::{ChunkPos, HeightmapKind};

use crate::ambience::biome_at;
use crate::assets::Assets;
use crate::audio::Audio;
use crate::effects::{ClientWeather, WorldEffects, light_at, precipitation_table};

const BIOME: &str = "minecraft:worldgen/biome";
const DIMENSION_TYPE: &str = "minecraft:dimension_type";
const BLOCK_REGISTRY: &str = "minecraft:block";
const MAX_CAMPFIRES: usize = 256;
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

    /// Servers omit entries the client knows from its own data pack, so those come from the jar.
    fn entry_visual(&self, directory: &str, entry: &RegistryItem) -> Visual {
        match &entry.data {
            Some(data) => Visual::from_registry_entry(data),
            None => {
                let id = entry.id.as_str();
                let (namespace, name) = id.split_once(':').unwrap_or(("minecraft", id));
                self.assets
                    .read(&format!("data/{namespace}/{directory}/{name}.json"))
                    .and_then(|bytes| registry_json(&bytes))
                    .map(|c| Visual::from_registry_entry(&c))
                    .unwrap_or_default()
            }
        }
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
        self.dimension = dimension_entry
            .map(|e| self.entry_visual("dimension_type", e))
            .unwrap_or_default();
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
                    .map(|e| self.entry_visual("worldgen/biome", e).over(&self.dimension))
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
        campfires: &mut dyn Iterator<Item = [i32; 3]>,
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
        let sounds = audio.is_some();
        let mut out = Out { effects, audio };
        let center = state.player.pos().map(|v| v.floor() as i32);
        self.animator.tick(center, &view, &mut out, sounds);
        for pos in campfires.take(MAX_CAMPFIRES) {
            let Some(block) = state.chunks.block(pos[0], pos[1], pos[2]) else {
                continue;
            };
            if let Some(signal) = self.animator.table().campfire(block.0) {
                self.animator.campfire_tick(pos, signal, &view, &mut out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let bytes = assets
                .read(&format!("data/minecraft/worldgen/biome/{biome}.json"))
                .unwrap_or_else(|| panic!("{biome} missing from the jar"));
            Visual::from_registry_entry(&registry_json(&bytes).unwrap())
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
