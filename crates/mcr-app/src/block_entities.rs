//! Keeps a [`BlockEntityScene`] in step with the client's chunks and block entity packets.

use std::sync::Arc;

use mcr_block_entity::{
    BeaconData, BeamBlock, BlockEntityData, BlockEntityFrame, BlockEntityKind, BlockEntityScene,
    DyeColor, PistonData, beam_sections,
};
use mcr_client::{ClientEvent, ClientState};
use mcr_entity_model::glam::Quat;
use mcr_entity_model::{FrameParams, SceneAssets, TargetFormats, wgpu};
use mcr_nbt::{Compound, Limits, Value, read_network};
use mcr_render::Camera;
use mcr_ui::Ui;
use mcr_world::{ChunkPos, ChunkStore};
use rustc_hash::FxHashMap;
use uuid::Uuid;

const MAX_PER_CHUNK: usize = 4096;
const MAX_TOTAL: usize = 1 << 16;
const BEACON_INTERVAL: u64 = 20;
const CONDUIT_INTERVAL: u64 = 40;
const CONDUIT_FRAME: [&str; 4] = [
    "prismarine",
    "prismarine_bricks",
    "sea_lantern",
    "dark_prismarine",
];

type Pos = [i32; 3];

pub struct BlockEntities {
    pub scene: BlockEntityScene,
    entries: FxHashMap<Pos, (BlockEntityKind, BlockEntityData)>,
    conduit_frames: FxHashMap<Pos, Vec<Pos>>,
}

impl std::fmt::Debug for BlockEntities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockEntities")
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

fn kind_of(type_id: i32) -> Option<BlockEntityKind> {
    let id = u32::try_from(type_id).ok()?;
    let name = mcr_data::v26_3()
        .registry("minecraft:block_entity_type")?
        .name_of(id)?;
    BlockEntityKind::from_type_name(name)
}

fn parse_tag(bytes: &[u8]) -> Compound {
    match read_network(&mut &bytes[..], Limits::NETWORK) {
        Ok(Some(Value::Compound(c))) => c,
        _ => Compound::default(),
    }
}

fn offset(at: Pos, d: Pos) -> Option<Pos> {
    Some([
        at[0].checked_add(d[0])?,
        at[1].checked_add(d[1])?,
        at[2].checked_add(d[2])?,
    ])
}

fn world_pos(chunk: ChunkPos, x: usize, y: i16, z: usize) -> Option<Pos> {
    Some([
        chunk
            .x
            .checked_mul(16)?
            .checked_add(i32::try_from(x & 15).ok()?)?,
        i32::from(y),
        chunk
            .z
            .checked_mul(16)?
            .checked_add(i32::try_from(z & 15).ok()?)?,
    ])
}

fn top_y(min_y: i32, height: u32) -> i32 {
    let height = i32::try_from(height).unwrap_or(i32::MAX);
    min_y.saturating_add(height).saturating_sub(1)
}

fn state_at(chunks: &ChunkStore, pos: Pos) -> Option<u32> {
    chunks.block(pos[0], pos[1], pos[2]).map(|s| s.0)
}

fn block_name(state: u32) -> &'static str {
    mcr_data::v26_3()
        .state_block_name(state)
        .map_or("", |n| n.strip_prefix("minecraft:").unwrap_or(n))
}

fn beam_block(state: u32) -> BeamBlock {
    let name = block_name(state);
    let color = name
        .strip_suffix("_stained_glass_pane")
        .or_else(|| name.strip_suffix("_stained_glass"))
        .and_then(DyeColor::from_name);
    if let Some(c) = color {
        return BeamBlock::Tint(c.diffuse_rgb());
    }
    let opaque = mcr_data::v26_3().light_block(state) >= 15 && name != "bedrock";
    if opaque {
        BeamBlock::Solid
    } else {
        BeamBlock::Open
    }
}

/// `ConduitBlockEntity.updateShape`: the frame blocks around a conduit that sits in water, empty
/// when the 3x3x3 cube around it is not all water.
fn conduit_frame(chunks: &ChunkStore, at: Pos) -> Vec<Pos> {
    let name_at = |d: [i32; 3]| {
        offset(at, d)
            .and_then(|p| state_at(chunks, p))
            .map_or("", block_name)
    };
    for x in -1..=1 {
        for y in -1..=1 {
            for z in -1..=1 {
                if !matches!(name_at([x, y, z]), "water" | "conduit") {
                    return Vec::new();
                }
            }
        }
    }
    let mut frame = Vec::new();
    for x in -2i32..=2 {
        for y in -2i32..=2 {
            for z in -2i32..=2 {
                let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
                let outside = ax > 1 || ay > 1 || az > 1;
                let line = (x == 0 && (ay == 2 || az == 2))
                    || (y == 0 && (ax == 2 || az == 2))
                    || (z == 0 && (ax == 2 || ay == 2));
                if outside && line && CONDUIT_FRAME.contains(&name_at([x, y, z])) {
                    frame.extend(offset(at, [x, y, z]));
                }
            }
        }
    }
    frame
}

/// A block entity the client ticks for particles and sounds.
#[derive(Debug)]
pub enum Ticker<'a> {
    Campfire {
        pos: Pos,
        slots: [bool; 4],
    },
    Spawner {
        pos: Pos,
        range: i16,
        has_entity: bool,
    },
    TrialSpawner {
        pos: Pos,
    },
    Vault {
        pos: Pos,
        has_item: bool,
        connected: &'a [Uuid],
        range_tenths: u16,
    },
    Conduit {
        pos: Pos,
        ticks: u32,
        frame: &'a [Pos],
        target: Option<Uuid>,
    },
}

impl BlockEntities {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: wgpu::TextureFormat,
        assets: Arc<SceneAssets>,
    ) -> Option<Self> {
        let formats = TargetFormats {
            color,
            depth: wgpu::TextureFormat::Depth32Float,
            reverse_z: true,
            sample_count: 1,
        };
        let scene = BlockEntityScene::new(device, queue, formats, assets)
            .map_err(|e| eprintln!("block entities: {e}"))
            .ok()?;
        Some(Self {
            scene,
            entries: FxHashMap::default(),
            conduit_frames: FxHashMap::default(),
        })
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.conduit_frames.clear();
        self.scene.clear();
    }

    fn put(
        &mut self,
        state: &ClientState,
        pos: Pos,
        kind: BlockEntityKind,
        mut data: BlockEntityData,
    ) {
        if self.entries.len() >= MAX_TOTAL && !self.entries.contains_key(&pos) {
            return;
        }
        let Some(block) = state_at(&state.chunks, pos) else {
            return;
        };
        match (kind, &mut data) {
            (BlockEntityKind::Beacon, d) => *d = Self::beacon_data(&state.chunks, pos),
            (BlockEntityKind::Conduit, BlockEntityData::Conduit(c)) => {
                let frame = conduit_frame(&state.chunks, pos);
                c.active = frame.len() >= 16;
                self.conduit_frames.insert(pos, frame);
            }
            _ => {}
        }
        self.scene.set(pos, block, kind, data.clone());
        self.entries.insert(pos, (kind, data));
    }

    fn beacon_data(chunks: &ChunkStore, pos: Pos) -> BlockEntityData {
        let top = top_y(chunks.config().min_y(), chunks.config().height());
        let sections = beam_sections(pos[1], top, |y| {
            state_at(chunks, [pos[0], y, pos[2]]).map_or(BeamBlock::Open, beam_block)
        });
        BlockEntityData::Beacon(BeaconData { sections })
    }

    /// Block entities with client-side particles and sounds.
    pub fn tickers(&self) -> impl Iterator<Item = Ticker<'_>> + '_ {
        self.entries.iter().filter_map(|(&pos, (kind, data))| {
            Some(match (kind, data) {
                (BlockEntityKind::Campfire, BlockEntityData::Campfire(c)) => Ticker::Campfire {
                    pos,
                    slots: c.items.each_ref().map(Option::is_some),
                },
                (BlockEntityKind::Spawner, BlockEntityData::Spawner(s)) => Ticker::Spawner {
                    pos,
                    range: s.range,
                    has_entity: s.entity_id.is_some(),
                },
                (BlockEntityKind::TrialSpawner, _) => Ticker::TrialSpawner { pos },
                (BlockEntityKind::Vault, BlockEntityData::Vault(v)) => Ticker::Vault {
                    pos,
                    has_item: v.display_item.is_some(),
                    connected: &v.connected,
                    range_tenths: v.connection_range_tenths,
                },
                (BlockEntityKind::Conduit, BlockEntityData::Conduit(c)) => Ticker::Conduit {
                    pos,
                    ticks: self.scene.conduit_ticks(pos),
                    frame: self.conduit_frames.get(&pos).map_or(&[], Vec::as_slice),
                    target: c.target,
                },
                _ => return None,
            })
        })
    }

    pub fn load_chunk(&mut self, state: &ClientState, pos: ChunkPos) {
        let Some(column) = state.chunks.column(pos) else {
            return;
        };
        for be in column.block_entities().iter().take(MAX_PER_CHUNK) {
            let Some(kind) = kind_of(be.type_id as i32) else {
                continue;
            };
            let Some(at) = world_pos(pos, be.x(), be.y, be.z()) else {
                continue;
            };
            let data = BlockEntityData::parse(kind, &parse_tag(&be.nbt));
            self.put(state, at, kind, data);
        }
    }

    fn remove(&mut self, pos: Pos) {
        self.conduit_frames.remove(&pos);
        if self.entries.remove(&pos).is_some() {
            self.scene.remove(pos);
        }
    }

    pub fn on_event(&mut self, event: &ClientEvent, state: &ClientState) {
        match event {
            ClientEvent::ChunkLoaded(pos) => self.load_chunk(state, *pos),
            ClientEvent::ChunkUnloaded(pos) => {
                let gone: Vec<Pos> = self
                    .entries
                    .keys()
                    .filter(|p| p[0] >> 4 == pos.x && p[2] >> 4 == pos.z)
                    .copied()
                    .collect();
                for p in gone {
                    self.remove(p);
                }
            }
            ClientEvent::BlockEntityChanged(p) => {
                let at = [p.x, p.y, p.z];
                if let Some((ty, tag)) = state.block_entities.get(p)
                    && let Some(kind) = kind_of(*ty)
                {
                    self.put(state, at, kind, BlockEntityData::parse(kind, tag));
                }
            }
            ClientEvent::BlockChanged { pos, state: block } => {
                let at = [pos.x, pos.y, pos.z];
                let still = mcr_data::v26_3()
                    .block_state(*block)
                    .is_some_and(|s| s.has_block_entity());
                match self.entries.get(&at).cloned() {
                    Some((kind, data)) if still => self.put(state, at, kind, data),
                    Some(_) => self.remove(at),
                    None if still => {
                        if let Some(kind) = BlockEntityKind::from_block_name(block_name(*block)) {
                            let data = BlockEntityData::parse(kind, &Compound::default());
                            self.put(state, at, kind, data);
                        }
                    }
                    None => {}
                }
            }
            ClientEvent::PistonMoving {
                pos,
                moved_state,
                facing,
                extending,
                source,
            } => {
                let data = BlockEntityData::Piston(PistonData {
                    moved_state: *moved_state,
                    progress: 0.0,
                    extending: *extending,
                    facing: *facing,
                    source: *source,
                });
                self.put(state, [pos.x, pos.y, pos.z], BlockEntityKind::Piston, data);
            }
            ClientEvent::BlockEvent { pos, b0, b1, .. } => {
                self.scene
                    .block_event([pos.x, pos.y, pos.z], *b0, *b1, state.world.game_time);
            }
            event if crate::net_world::resets_world(event) => self.clear(),
            _ => {}
        }
    }

    /// Re-derives world-dependent state (beacon beams, conduit shells) at their vanilla cadence.
    pub fn tick(&mut self, state: &ClientState, ticks: u64) {
        let due: Vec<(Pos, BlockEntityKind)> = self
            .entries
            .iter()
            .filter(|(_, (k, _))| match k {
                BlockEntityKind::Beacon => ticks.is_multiple_of(BEACON_INTERVAL),
                BlockEntityKind::Conduit => ticks.is_multiple_of(CONDUIT_INTERVAL),
                _ => false,
            })
            .map(|(p, (k, _))| (*p, *k))
            .collect();
        for (pos, kind) in due {
            if let Some((_, data)) = self.entries.get(&pos).cloned() {
                self.put(state, pos, kind, data);
            }
        }
    }

    pub fn update(
        &mut self,
        state: &ClientState,
        params: FrameParams,
        camera: &Camera,
        partial: f32,
        render_distance_blocks: f64,
        ui: &mut Ui,
    ) {
        let chunks = &state.chunks;
        let frame = BlockEntityFrame {
            params,
            game_time: state.world.game_time,
            partial_ticks: partial,
            camera_rotation: Quat::from_mat4(&camera.look_rotation().inverse()),
            player: Some(state.player.physics.pos),
            render_distance_blocks,
        };
        self.scene.update(&frame, Some(ui), &|p| {
            let Some(column) = chunks.column(ChunkPos::new(p[0] >> 4, p[2] >> 4)) else {
                return [0, 15];
            };
            let (lx, lz) = ((p[0] & 15) as usize, (p[2] & 15) as usize);
            [
                column.block_light(lx, p[1], lz).unwrap_or(0),
                column.sky_light_or_above(lx, p[1], lz).unwrap_or(15),
            ]
        });
        self.scene.prepare(&frame);
    }

    fn has_content(&self) -> bool {
        !self.entries.is_empty() || self.scene.stats().text_quads > 0
    }

    pub fn draw_opaque(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.has_content() {
            self.scene.draw_opaque(pass);
        }
    }

    pub fn draw_blended(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.has_content() {
            self.scene.draw_blended(pass);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extreme_chunk_coordinates_do_not_overflow() {
        assert_eq!(world_pos(ChunkPos::new(i32::MAX, 0), 15, 0, 0), None);
        assert_eq!(world_pos(ChunkPos::new(0, i32::MIN), 0, 0, 15), None);
        assert_eq!(
            world_pos(ChunkPos::new(1_875_000, -1_875_000), 15, -64, 0),
            Some([30_000_015, -64, -30_000_000])
        );
        assert_eq!(
            world_pos(ChunkPos::new(0, 0), 255, i16::MAX, 255),
            Some([15, 32767, 15])
        );
    }

    #[test]
    fn offsets_and_heights_saturate() {
        assert_eq!(offset([i32::MAX, 0, 0], [1, 0, 0]), None);
        assert_eq!(offset([i32::MIN, 0, 0], [-2, 0, 0]), None);
        assert_eq!(
            offset([30_000_000, 0, 0], [2, 1, -1]),
            Some([30_000_002, 1, -1])
        );
        assert_eq!(top_y(i32::MAX, u32::MAX), i32::MAX - 1);
        assert_eq!(top_y(-64, 384), 319);
    }

    #[test]
    fn hostile_nbt_gives_empty_tag() {
        assert!(parse_tag(&[0x0a, 0xff, 0xff]).is_empty());
        assert!(parse_tag(&[]).is_empty());
    }
}
