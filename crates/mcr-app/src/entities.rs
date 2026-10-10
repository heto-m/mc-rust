//! Turns the client's entity state into per-frame [`EntityView`]s and draws them.

use std::sync::Arc;

use mcr_client::{ClientEvent, ClientState, Entity};
use mcr_entity_model::glam::{Mat4, Vec3};
use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::entity_data::{EntityDataValue, Pose as DataPose};
use mcr_entity_model::wgpu;
use mcr_entity_model::{
    EntityData, EntityScene, EntityView, FrameParams, LightInfo, PlayerView, Registries,
    SceneAssets, TargetFormats, slot,
};
use mcr_physics::Pose as PhysPose;
use mcr_render::{Camera, Frustum};
use mcr_world::{ChunkPos, ChunkStore};
use rustc_hash::FxHashMap;

const SWING_TICKS: i32 = 6;
const HURT_TICKS: i32 = 10;
const DEATH_TICKS: i32 = 20;
const HEAD_BODY_LIMIT: f32 = 50.0;
const WALK_SMOOTHING: f32 = 0.4;
const CULL_MARGIN: f32 = 0.5;
const NON_LIVING: &[&str] = &[
    "item",
    "orb",
    "arrow",
    "trident",
    "minecart",
    "boat",
    "raft",
    "painting",
    "tnt",
    "falling_block",
    "ball",
    "egg",
    "pearl",
    "fireball",
    "potion",
    "firework",
    "bolt",
    "marker",
    "display",
    "interaction",
    "area_effect",
    "leash",
    "fishing",
    "wind_charge",
    "fangs",
    "shulker_bullet",
    "llama_spit",
    "eye_of_ender",
    "experience_bottle",
    "armor_stand",
    "end_crystal",
    "ominous_item_spawner",
    "lightning",
    "wither_skull",
    "spear",
    "snowball",
];

type Mutation = Box<dyn FnOnce(&mut Track)>;

fn wrap_degrees(v: f32) -> f32 {
    let r = v.rem_euclid(360.0);
    if r >= 180.0 { r - 360.0 } else { r }
}

fn lerp_degrees(a: f32, b: f32, t: f32) -> f32 {
    a + wrap_degrees(b - a) * t
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Names for the client's synced registries.
pub struct SyncedRegistries<'a>(pub &'a mcr_client::Registries);

impl Registries for SyncedRegistries<'_> {
    fn entry_name(&self, registry: &str, id: i32) -> Option<&str> {
        Some(self.0.get(registry)?.get(id)?.id.as_str())
    }
}

/// Everything that is rendered per tick for one entity.
#[derive(Debug, Clone)]
pub struct Track {
    prev_pos: [f64; 3],
    pos: [f64; 3],
    prev_yaw: f32,
    yaw: f32,
    prev_head: f32,
    head: f32,
    prev_pitch: f32,
    pitch: f32,
    prev_body: f32,
    body: f32,
    walk: f32,
    speed: f32,
    prev_speed: f32,
    age: u32,
    swing: i32,
    swing_off: bool,
    hurt: i32,
    death: i32,
    seen: u64,
    living: bool,
}

impl Track {
    fn new(pos: [f64; 3], yaw: f32, head: f32, pitch: f32, living: bool) -> Self {
        Self {
            prev_pos: pos,
            pos,
            prev_yaw: yaw,
            yaw,
            prev_head: head,
            head,
            prev_pitch: pitch,
            pitch,
            prev_body: yaw,
            body: yaw,
            walk: 0.0,
            speed: 0.0,
            prev_speed: 0.0,
            age: 0,
            swing: 0,
            swing_off: false,
            hurt: 0,
            death: 0,
            seen: 0,
            living,
        }
    }

    fn advance(&mut self, pos: [f64; 3], yaw: f32, head: f32, pitch: f32, dead: bool) {
        self.prev_pos = self.pos;
        self.prev_yaw = self.yaw;
        self.prev_head = self.head;
        self.prev_pitch = self.pitch;
        self.prev_body = self.body;
        self.prev_speed = self.speed;
        (self.pos, self.yaw, self.head, self.pitch) = (pos, yaw, head, pitch);
        let dx = (self.pos[0] - self.prev_pos[0]) as f32;
        let dz = (self.pos[2] - self.prev_pos[2]) as f32;
        let target = ((dx * dx + dz * dz).sqrt() * 4.0).min(1.0);
        self.speed += (target - self.speed) * WALK_SMOOTHING;
        self.walk += self.speed;
        self.age = self.age.wrapping_add(1);
        self.swing = (self.swing - 1).max(0);
        self.hurt = (self.hurt - 1).max(0);
        if dead {
            self.death = (self.death + 1).min(DEATH_TICKS);
        }
        if self.living {
            self.turn_body(dx, dz);
        } else {
            self.body = self.yaw;
            self.prev_body = self.yaw;
        }
    }

    fn turn_body(&mut self, dx: f32, dz: f32) {
        let mut target = self.body;
        if dx * dx + dz * dz > 0.0025 {
            let walk = dz.atan2(dx).to_degrees() - 90.0;
            let diff = (wrap_degrees(self.yaw) - walk).abs();
            target = if diff > 95.0 && diff < 265.0 {
                walk - 180.0
            } else {
                walk
            };
        }
        if self.swing > 0 {
            target = self.yaw;
        }
        self.body += wrap_degrees(target - self.body) * 0.3;
        let head_rel = wrap_degrees(self.yaw - self.body);
        if head_rel.abs() > HEAD_BODY_LIMIT {
            self.body += head_rel - head_rel.signum() * HEAD_BODY_LIMIT;
        }
    }

    fn start_swing(&mut self, off_hand: bool) {
        self.swing = SWING_TICKS;
        self.swing_off = off_hand;
    }
}

fn is_living(name: &str) -> bool {
    let name = name.strip_prefix("minecraft:").unwrap_or(name);
    !NON_LIVING.iter().any(|p| name.contains(p))
}

/// Local player state sampled once per tick.
#[derive(Debug, Clone, Copy)]
pub struct LocalSample {
    pub feet: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub dead: bool,
}

pub struct EntityLayer {
    pub scene: EntityScene,
    tracks: FxHashMap<i32, Track>,
    local: Track,
    local_data: FxHashMap<u8, EntityDataValue>,
    local_equipment: [ItemStack; 8],
    player_type: i32,
    scale_attribute: Option<i32>,
    tick: u64,
    drawn_list: Vec<(i32, [f64; 3], f32, bool)>,
    pub last_drawn: usize,
    pub last_culled: usize,
}

impl std::fmt::Debug for EntityLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityLayer")
            .field("tracks", &self.tracks.len())
            .finish_non_exhaustive()
    }
}

/// Per-frame culling inputs.
pub struct Cull {
    pub camera_pos: [f64; 3],
    pub frustum: Frustum,
    pub distance_scale: f64,
}

pub struct FrameCtx<'a> {
    pub state: &'a ClientState,
    pub partial: f32,
    pub cull: &'a Cull,
    pub show_local: bool,
    pub local_swing: f32,
    pub local_using: bool,
}

impl EntityLayer {
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
        let mut scene = EntityScene::new(device, queue, formats, assets)
            .map_err(|e| eprintln!("entities: {e}"))
            .ok()?;
        scene.set_shadows(true);
        let data = mcr_data::v26_3();
        let scale_attribute = data
            .registry("minecraft:attribute")
            .and_then(|r| r.id_of("minecraft:scale"))
            .map(|id| id as i32);
        Some(Self {
            scene,
            tracks: FxHashMap::default(),
            local: Track::new([0.0; 3], 0.0, 0.0, 0.0, true),
            local_data: FxHashMap::default(),
            local_equipment: Default::default(),
            player_type: data
                .entity_type_by_name("minecraft:player")
                .map_or(-1, |id| id as i32),
            scale_attribute,
            tick: 0,
            drawn_list: Vec::new(),
            last_drawn: 0,
            last_culled: 0,
        })
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    pub fn on_event(&mut self, event: &ClientEvent, local_id: i32) {
        let (id, f): (i32, Mutation) = match *event {
            ClientEvent::EntityAnimation { entity_id, action } => match action {
                0 | 3 => (entity_id, Box::new(move |t| t.start_swing(action == 3))),
                _ => return,
            },
            ClientEvent::EntityEvent { entity_id, event } => match event {
                2 | 33 | 36 | 37 | 44 | 57 => (entity_id, Box::new(|t| t.hurt = HURT_TICKS)),
                3 => (entity_id, Box::new(|t| t.death = t.death.max(1))),
                _ => return,
            },
            ClientEvent::EntityDamaged { entity_id, .. } => {
                (entity_id, Box::new(|t| t.hurt = HURT_TICKS))
            }
            _ => return,
        };
        if id == local_id {
            f(&mut self.local);
        } else if let Some(t) = self.tracks.get_mut(&id) {
            f(t);
        }
    }

    /// One 20 Hz step: records where every entity is now so frames can interpolate from the last tick.
    pub fn tick(&mut self, state: &ClientState, local: LocalSample) {
        self.tick += 1;
        let data = mcr_data::v26_3();
        for e in state.entities.iter() {
            let living = data.entity_type_name(e.kind as u32).is_some_and(is_living);
            let dead = health(e).is_some_and(|h| h <= 0.0);
            let tick = self.tick;
            let t = self
                .tracks
                .entry(e.id)
                .or_insert_with(|| Track::new(e.pos, e.yaw, e.head_yaw, e.pitch, living));
            t.seen = tick;
            t.advance(e.pos, e.yaw, e.head_yaw, e.pitch, dead);
        }
        let tick = self.tick;
        self.tracks.retain(|_, t| t.seen == tick);
        if self.local.age == 0 {
            self.local = Track::new(local.feet, local.yaw, local.yaw, local.pitch, true);
        }
        self.local
            .advance(local.feet, local.yaw, local.yaw, local.pitch, local.dead);
    }

    fn refresh_local(&mut self, state: &ClientState) {
        let p = &state.player;
        let physics = &p.physics;
        self.local_data.clone_from(&p.data);
        let mut flags = 0i8;
        if physics.crouching {
            flags |= 0x02;
        }
        if physics.sprinting {
            flags |= 0x08;
        }
        if physics.swimming {
            flags |= 0x10;
        }
        self.local_data.insert(0, EntityDataValue::Byte(flags));
        let pose = match physics.pose {
            PhysPose::Standing => DataPose::Standing,
            PhysPose::Crouching => DataPose::Crouching,
            PhysPose::Swimming => DataPose::Swimming,
            PhysPose::FallFlying => DataPose::FallFlying,
            PhysPose::SpinAttack => DataPose::SpinAttack,
        };
        self.local_data.insert(6, EntityDataValue::Pose(pose));
        self.local_data.insert(
            9,
            EntityDataValue::Float(if p.dead { 0.0 } else { p.health }),
        );
        let inv = &state.inventory;
        let menu = |index: i32| {
            mcr_client::inventory_index_to_menu_slot(index)
                .and_then(|i| inv.player.slots.get(i))
                .cloned()
                .unwrap_or_default()
        };
        self.local_equipment[slot::MAINHAND] = inv.hotbar(p.held_slot).clone();
        self.local_equipment[slot::FEET] = menu(36);
        self.local_equipment[slot::LEGS] = menu(37);
        self.local_equipment[slot::CHEST] = menu(38);
        self.local_equipment[slot::HEAD] = menu(39);
        self.local_equipment[slot::OFFHAND] = menu(40);
    }

    /// Builds this frame's instances; returns the drawn entity count.
    pub fn update(&mut self, cx: &FrameCtx<'_>) {
        self.refresh_local(cx.state);
        let Self {
            scene,
            tracks,
            local,
            local_data,
            local_equipment,
            player_type,
            scale_attribute,
            drawn_list,
            ..
        } = self;
        drawn_list.clear();
        let registries = SyncedRegistries(&cx.state.registries);
        let data = mcr_data::v26_3();
        let mut culled = 0usize;
        let mut drawn = 0usize;
        let local_id = cx.state.player.entity_id;
        let local_view = cx.show_local.then(|| {
            let mut v = make_view(
                local_id,
                cx.state.uuid,
                *player_type,
                local,
                cx.partial,
                local_data,
                1.0,
            );
            v.equipment = Some(&*local_equipment);
            v.attack_time = cx.local_swing;
            v.player = PlayerView {
                crouching: cx.state.player.physics.crouching,
                swimming: cx.state.player.physics.swimming,
                using_item: cx.local_using,
                ..PlayerView::default()
            };
            v.light = light_at(&cx.state.chunks, v.pos, 1.8);
            v
        });
        let views = tracks
            .iter()
            .filter_map(|(id, t)| {
                let e = cx.state.entities.get(*id)?;
                let (w, h) = type_size(data, e.kind);
                let scale = scale_attribute
                    .and_then(|a| e.attributes.get(&a))
                    .map_or(1.0, |s| s.value(None) as f32)
                    .clamp(0.0625, 16.0);
                let mut pos = lerp3(t.prev_pos, t.pos, f64::from(cx.partial));
                if let Some(vehicle) = e.vehicle.and_then(|v| cx.state.entities.get(v)) {
                    let (_, vh) = type_size(data, vehicle.kind);
                    let names = (
                        data.entity_type_name(vehicle.kind as u32).unwrap_or(""),
                        data.entity_type_name(e.kind as u32).unwrap_or(""),
                    );
                    let seat = crate::seats::offset(names.0, vh, vehicle.yaw, names.1);
                    pos = [
                        vehicle.pos[0] + seat[0],
                        vehicle.pos[1] + seat[1],
                        vehicle.pos[2] + seat[2],
                    ];
                }
                if !visible(cx, state_chunks(cx.state), pos, w * scale, h * scale) {
                    culled += 1;
                    return None;
                }
                drawn += 1;
                drawn_list.push((
                    e.id,
                    pos,
                    h * scale,
                    matches!(
                        e.data.get(&6),
                        Some(EntityDataValue::Pose(DataPose::Crouching))
                    ),
                ));
                let mut view = make_view(e.id, e.uuid, e.kind, t, cx.partial, &e.data, scale);
                view.pos = pos;
                view.passenger = e.vehicle.is_some();
                view.object_data = e.object_data;
                view.equipment = Some(&e.equipment);
                if e.kind == *player_type {
                    view.player = PlayerView {
                        crouching: view.pose() == DataPose::Crouching,
                        swimming: view.pose() == DataPose::Swimming,
                        using_item: view.living_flag(0),
                        ..PlayerView::default()
                    };
                }
                view.light = light_at(&cx.state.chunks, pos, h * scale);
                Some(view)
            })
            .chain(local_view);
        let chunks = &cx.state.chunks;
        scene.update_grounded(&registries, views, &|x, y, z| ground_top(chunks, x, y, z));
        self.last_drawn = drawn;
        self.last_culled = culled;
    }

    fn has_parts(&self) -> bool {
        self.last_drawn > 0 || self.scene.stats().parts > 0
    }

    pub fn draw_opaque(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.has_parts() {
            self.scene.draw_opaque(pass);
        }
    }

    pub fn draw_blended(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.has_parts() {
            self.scene.draw_blended(pass);
        }
    }

    /// The local player as a standing view for the inventory preview.
    pub fn local_view(&mut self, state: &ClientState) -> EntityView<'_> {
        self.refresh_local(state);
        self.local_data.insert(0, EntityDataValue::Byte(0));
        self.local_data
            .insert(6, EntityDataValue::Pose(DataPose::Standing));
        let mut v = make_view(
            state.player.entity_id,
            state.uuid,
            self.player_type,
            &self.local,
            0.0,
            &self.local_data,
            1.0,
        );
        v.equipment = Some(&self.local_equipment);
        v
    }

    pub fn drawn_entities(
        &self,
        _state: &ClientState,
        _partial: f32,
    ) -> impl Iterator<Item = (i32, [f64; 3], f32, bool)> + '_ {
        self.drawn_list.iter().copied()
    }

    pub fn frame_params(
        &self,
        camera: &Camera,
        aspect: f32,
        env: &mcr_render::Environment,
    ) -> FrameParams {
        let (rd_start, rd_end) = env.render_fog_range();
        FrameParams {
            view_proj: camera.view_proj(aspect),
            camera: camera.position.to_array(),
            light: LightInfo {
                sky_brightness: env.sky_factor,
                fog: [env.fog_start, env.fog_end, rd_start, rd_end],
                fog_color: env.fog_color,
            },
        }
    }
}

fn ground_top(chunks: &ChunkStore, x: i32, y: i32, z: i32) -> Option<f32> {
    let state = chunks.block(x, y, z)?;
    mcr_data::v26_3()
        .collision_shape(u32::from(state.0))
        .iter()
        .map(|b| b.max[1])
        .reduce(f32::max)
}

fn state_chunks(state: &ClientState) -> &ChunkStore {
    &state.chunks
}

fn health(e: &Entity) -> Option<f32> {
    match e.data.get(&9) {
        Some(EntityDataValue::Float(h)) => Some(*h),
        _ => None,
    }
}

fn type_size(data: &'static mcr_data::Version, kind: i32) -> (f32, f32) {
    u32::try_from(kind)
        .ok()
        .and_then(|k| data.entity_type_info(k))
        .map_or((0.6, 1.8), |i| (i.width, i.height))
}

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn visible(cx: &FrameCtx<'_>, chunks: &ChunkStore, pos: [f64; 3], w: f32, h: f32) -> bool {
    if !pos.iter().all(|c| c.is_finite() && c.abs() < 3.0e7) {
        return false;
    }
    let rel = [
        pos[0] - cx.cull.camera_pos[0],
        pos[1] - cx.cull.camera_pos[1],
        pos[2] - cx.cull.camera_pos[2],
    ];
    let size = f64::from((w + h + w) / 3.0).max(0.25);
    let limit = size * 64.0 * cx.cull.distance_scale;
    if rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2] > limit * limit {
        return false;
    }
    let chunk = ChunkPos::new((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4);
    if chunks.column(chunk).is_none() {
        return false;
    }
    let half = w.max(0.1) / 2.0 + CULL_MARGIN;
    let min = Vec3::new(
        rel[0] as f32 - half,
        rel[1] as f32 - CULL_MARGIN,
        rel[2] as f32 - half,
    );
    let max = Vec3::new(
        rel[0] as f32 + half,
        rel[1] as f32 + h + CULL_MARGIN,
        rel[2] as f32 + half,
    );
    cx.cull.frustum.intersects_aabb(min, max)
}

pub fn light_at(chunks: &ChunkStore, pos: [f64; 3], height: f32) -> [u8; 2] {
    let x = pos[0].floor() as i32;
    let y = (pos[1] + f64::from(height) * 0.5).floor() as i32;
    let z = pos[2].floor() as i32;
    let Some(column) = chunks.column(ChunkPos::new(x >> 4, z >> 4)) else {
        return [0, 15];
    };
    let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
    [
        column.block_light(lx, y, lz).unwrap_or(0),
        column.sky_light(lx, y, lz).unwrap_or(15),
    ]
}

fn make_view<'a>(
    id: i32,
    uuid: uuid::Uuid,
    kind: i32,
    t: &Track,
    partial: f32,
    data: &'a dyn EntityData,
    scale: f32,
) -> EntityView<'a> {
    let mut v = EntityView::new(kind, data);
    v.id = id;
    v.uuid = uuid;
    v.pos = lerp3(t.prev_pos, t.pos, f64::from(partial));
    v.yaw = lerp_degrees(t.prev_yaw, t.yaw, partial);
    v.body_yaw = lerp_degrees(t.prev_body, t.body, partial);
    v.head_yaw = lerp_degrees(t.prev_head, t.head, partial);
    v.pitch = lerp(t.prev_pitch, t.pitch, partial);
    v.limb_swing = t.walk - t.speed * (1.0 - partial);
    v.limb_amount = lerp(t.prev_speed, t.speed, partial).min(1.0);
    v.age_ticks = t.age as f32 + partial;
    v.scale = scale;
    v.hurt = t.hurt > 0;
    v.death_time = if t.death > 0 {
        t.death as f32 + partial
    } else {
        0.0
    };
    v.attack_time = if t.swing > 0 {
        (SWING_TICKS - t.swing) as f32 / SWING_TICKS as f32 + partial / SWING_TICKS as f32
    } else {
        0.0
    };
    v.invisible = v.shared_flag(5);
    v
}

/// View-projection of the vanilla hand: fixed 70 degrees, no world rotation.
pub fn hand_view_proj(aspect: f32) -> Mat4 {
    mcr_render::glam::camera::rh::proj::directx::perspective_infinite_reverse(
        70f32.to_radians(),
        aspect,
        0.05,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_wrap() {
        assert!((lerp_degrees(350.0, 10.0, 0.5) - 360.0).abs() < 1e-3);
        assert!((wrap_degrees(190.0) + 170.0).abs() < 1e-4);
    }

    #[test]
    fn walk_speed_converges_when_moving() {
        let mut t = Track::new([0.0; 3], 0.0, 0.0, 0.0, true);
        for i in 1..=20 {
            t.advance([f64::from(i) * 0.25, 0.0, 0.0], 0.0, 0.0, 0.0, false);
        }
        assert!(t.speed > 0.95, "{}", t.speed);
        assert!(t.walk > 10.0);
    }

    #[test]
    fn body_follows_travel_direction() {
        let mut t = Track::new([0.0; 3], 0.0, 0.0, 0.0, true);
        for i in 1..=40 {
            t.advance([0.0, 0.0, f64::from(i) * 0.3], 0.0, 0.0, 0.0, false);
        }
        assert!(wrap_degrees(t.body).abs() < 2.0, "{}", t.body);
    }

    #[test]
    fn head_stays_within_limit_of_body() {
        let mut t = Track::new([0.0; 3], 0.0, 0.0, 0.0, true);
        t.advance([0.0; 3], 170.0, 170.0, 0.0, false);
        assert!(wrap_degrees(t.yaw - t.body).abs() <= HEAD_BODY_LIMIT + 1e-3);
    }

    #[test]
    fn projectiles_are_not_living() {
        assert!(!is_living("minecraft:arrow"));
        assert!(!is_living("minecraft:item"));
        assert!(is_living("minecraft:zombie"));
        assert!(is_living("minecraft:villager"));
    }
}
