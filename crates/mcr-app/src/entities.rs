//! Turns the client's entity state into per-frame [`EntityView`]s and draws them.

use std::f32::consts::PI;
use std::sync::Arc;

use mcr_client::{ClientEvent, ClientState, Entity};
use mcr_entity_model::glam::{Mat4, Vec3};
use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::entity_data::{EntityDataValue, Pose as DataPose};
use mcr_entity_model::wgpu;
use mcr_entity_model::{
    BodyVisibility, DisplayTrack, EntityData, EntityScene, EntityView, FrameParams, Glide,
    LightInfo, NoData, PlayerView, PoseLerp, RailShape, Registries, SceneAssets, TargetFormats,
    VehicleAnim, display_local, hidden_while_fresh, idx, rail_pose, slot, teleport_duration,
};
use mcr_physics::Pose as PhysPose;
use mcr_protocol::play::serverbound::Facing;
use mcr_protocol::types::game::GameType;
use mcr_render::{Camera, Frustum};
use mcr_ui::Ui;
use mcr_world::{ChunkPos, ChunkStore};
use rustc_hash::FxHashMap;

use crate::text_display::TextDisplays;

const SWING_TICKS: i32 = 6;
const HURT_TICKS: i32 = 10;
const DEATH_TICKS: i32 = 20;
const HEAD_BODY_LIMIT: f32 = 50.0;
const WALK_SMOOTHING: f32 = 0.4;
const CULL_MARGIN: f32 = 0.5;
const FRESH_HIDE_DIST_SQ: f64 = 12.25;
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
    /// `getTicksUsingItem`: ticks since the using flag came on; `use_off` is the hand.
    use_ticks: u32,
    use_off: bool,
    glide: GlideTrack,
    cloak: CloakTrack,
    boat: BoatTrack,
    display: DisplayTrack,
    display_kind: DisplayKind,
    pose_lerp: PoseLerp,
}

/// `ClientAvatarState`: the cape's lagging anchor, walk bob and walked distance.
#[derive(Debug, Clone, Copy, Default)]
struct CloakTrack {
    cloak: [f64; 3],
    prev: [f64; 3],
    bob: f32,
    prev_bob: f32,
    walk: f32,
    prev_walk: f32,
}

impl CloakTrack {
    /// `pos` is where the player stood when the tick began; `bob` is `updateBob`'s target.
    fn tick(&mut self, pos: [f64; 3], bob: f32, walked: f32) {
        self.prev_walk = self.walk;
        self.walk += walked;
        self.prev = self.cloak;
        for ((cloak, prev), p) in self.cloak.iter_mut().zip(&mut self.prev).zip(pos) {
            let d = p - *cloak;
            if d.abs() > 10.0 || !d.is_finite() {
                *cloak = p;
                *prev = p;
            } else {
                *cloak += d * 0.25;
            }
        }
        self.prev_bob = self.bob;
        self.bob += (bob - self.bob) * 0.4;
    }

    /// `extractCapeState`: flap, lean and sideways lean in degrees.
    fn view(&self, partial: f32, pos: [f64; 3], body_yaw: f32, fall_flying: f32) -> [f32; 3] {
        let cloak = lerp3(self.prev, self.cloak, f64::from(partial));
        let d = [cloak[0] - pos[0], cloak[1] - pos[1], cloak[2] - pos[2]].map(|x| x as f32);
        let (fx, fz) = (body_yaw.to_radians().sin(), -body_yaw.to_radians().cos());
        let lean = ((d[0] * fx + d[2] * fz) * 100.0 * (1.0 - fall_flying)).clamp(0.0, 150.0);
        let lean2 = ((d[0] * fz - d[2] * fx) * 100.0).clamp(-20.0, 20.0);
        let bob = lerp(self.prev_bob, self.bob, partial);
        let walk = lerp(self.prev_walk, self.walk, partial);
        let flap = (d[1] * 10.0).clamp(-6.0, 32.0) + (walk * 6.0).sin() * 32.0 * bob;
        [flap, lean, lean2]
    }
}

/// `AbstractBoat`'s client-side paddle phases and bubble-column wobble.
#[derive(Debug, Clone, Copy, Default)]
struct BoatTrack {
    paddles: [f32; 2],
    rowing: [bool; 2],
    bubble_multiplier: f32,
    bubble: f32,
    prev_bubble: f32,
    under_water: bool,
}

impl BoatTrack {
    fn tick(&mut self, data: &FxHashMap<u8, EntityDataValue>, age: u32) {
        let flag = |i: u8| matches!(data.get(&i), Some(EntityDataValue::Boolean(true)));
        self.rowing = [flag(BOAT_PADDLE_LEFT), flag(BOAT_PADDLE_RIGHT)];
        for (p, on) in self.paddles.iter_mut().zip(self.rowing) {
            *p = if on {
                (*p + PI / 8.0) % (PI * 16.0)
            } else {
                0.0
            };
        }
        let bubbling =
            matches!(data.get(&BOAT_BUBBLE_TIME), Some(EntityDataValue::Int(t)) if *t > 0);
        self.bubble_multiplier =
            (self.bubble_multiplier + if bubbling { 0.05 } else { -0.1 }).clamp(0.0, 1.0);
        self.prev_bubble = self.bubble;
        self.bubble = 10.0 * (0.5 * f64::from(age)).sin() as f32 * self.bubble_multiplier;
    }

    fn rowing_time(&self, side: usize, partial: f32) -> f32 {
        if self.rowing[side] {
            let p = self.paddles[side];
            lerp(p - PI / 8.0, p, partial.clamp(0.0, 1.0))
        } else {
            0.0
        }
    }
}

const BOAT_PADDLE_LEFT: u8 = 11;
const BOAT_PADDLE_RIGHT: u8 = 12;
const BOAT_BUBBLE_TIME: u8 = 13;

/// `Entity#updateFluidOnEyes`: the eye sits 1/9 block below the eye height for fluid checks.
fn eye_in_water(state: &ClientState, e: &Entity) -> bool {
    let eye = u32::try_from(e.kind)
        .ok()
        .and_then(|k| state.data.entity_type_info(k))
        .map_or(0.0, |i| f64::from(i.eye_height));
    let at = [e.pos[0], e.pos[1] + eye - 0.111_111_11, e.pos[2]];
    mcr_physics::eye_in_water(&state.chunks, mcr_physics::PhysicsData::v26_3(), at)
}

fn is_boat(name: &str) -> bool {
    name.ends_with("boat") || name.ends_with("raft")
}

/// `VehicleEntity` hurt state for boats and minecarts, plus the boat's own animation.
fn vehicle_anim(
    name: &str,
    data: &FxHashMap<u8, EntityDataValue>,
    t: &Track,
    partial: f32,
) -> VehicleAnim {
    let int = |i: u8| match data.get(&i) {
        Some(EntityDataValue::Int(v)) => *v,
        _ => 0,
    };
    let damage = match data.get(&10) {
        Some(EntityDataValue::Float(d)) if d.is_finite() => d.clamp(0.0, 1.0e4),
        _ => 0.0,
    };
    let mut anim = VehicleAnim {
        hurt: int(8).clamp(0, 1000) as f32 - partial,
        hurt_dir: int(9).clamp(-1, 1) as f32,
        damage: (damage - partial).max(0.0),
        ..VehicleAnim::default()
    };
    if is_boat(name) {
        anim.rowing = [
            t.boat.rowing_time(0, partial),
            t.boat.rowing_time(1, partial),
        ];
        anim.bubble = lerp(t.boat.prev_bubble, t.boat.bubble, partial);
        anim.under_water = t.boat.under_water;
    }
    anim
}

fn rail_shape(chunks: &ChunkStore, b: [i32; 3]) -> Option<RailShape> {
    let state = chunks.block(b[0], b[1], b[2])?.0;
    let data = mcr_data::v26_3();
    let name = data.state_block_name(state)?;
    if !matches!(
        name,
        "minecraft:rail"
            | "minecraft:powered_rail"
            | "minecraft:detector_rail"
            | "minecraft:activator_rail"
    ) {
        return None;
    }
    data.state_properties(state)
        .find_map(|(k, v)| (k == "shape").then_some(v))
        .and_then(RailShape::parse)
}

/// `AbstractClientPlayer.updateBob`'s target.
fn bob_target(on_ground: bool, dead: bool, swimming: bool, vel: [f64; 3]) -> f32 {
    if on_ground && !dead && !swimming {
        (vel[0].hypot(vel[2]) as f32).min(0.1)
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisplayKind {
    None,
    Shape,
    Text,
}

/// `LivingEntity.fallFlyTicks` and `ElytraAnimationState`, ticked with the entity.
#[derive(Debug, Clone, Copy)]
struct GlideTrack {
    ticks: u32,
    wings: [f32; 3],
    prev_wings: [f32; 3],
    vel: [f64; 3],
    prev_vel: [f64; 3],
}

impl Default for GlideTrack {
    fn default() -> Self {
        Self {
            ticks: 0,
            wings: Glide::REST_WINGS,
            prev_wings: Glide::REST_WINGS,
            vel: [0.0; 3],
            prev_vel: [0.0; 3],
        }
    }
}

impl GlideTrack {
    fn tick(&mut self, gliding: bool, crouching: bool, vel: [f64; 3]) {
        self.prev_vel = self.vel;
        self.vel = vel;
        self.ticks = if gliding {
            self.ticks.saturating_add(1)
        } else {
            0
        };
        self.prev_wings = self.wings;
        let target = if gliding {
            let len = (vel[0] * vel[0] + vel[1] * vel[1] + vel[2] * vel[2]).sqrt();
            let ratio = if vel[1] < 0.0 && len > 0.0 {
                1.0 - (-vel[1] / len).powf(1.5) as f32
            } else {
                1.0
            };
            [
                lerp(PI / 12.0, PI / 9.0, ratio),
                0.0,
                lerp(-PI / 12.0, -PI / 2.0, ratio),
            ]
        } else if crouching {
            [PI * 2.0 / 9.0, 0.087_266_46, -PI / 4.0]
        } else {
            Glide::REST_WINGS
        };
        for (w, t) in self.wings.iter_mut().zip(target) {
            *w += (t - *w) * 0.3;
        }
    }

    /// `extractFlightData` plus the `speedValue` of `HumanoidMobRenderer`.
    fn view(&self, partial: f32, yaw: f32, pitch: f32, gliding: bool) -> Glide {
        let wings = std::array::from_fn(|i| lerp(self.prev_wings[i], self.wings[i], partial));
        if !gliding {
            return Glide {
                wings,
                ..Glide::default()
            };
        }
        let p = f64::from(partial);
        let mx = self.prev_vel[0] + (self.vel[0] - self.prev_vel[0]) * p;
        let mz = self.prev_vel[2] + (self.vel[2] - self.prev_vel[2]) * p;
        let (yaw, pitch) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
        let (lx, lz) = (-yaw.sin() * pitch.cos(), yaw.cos() * pitch.cos());
        let (m2, l2) = (mx * mx + mz * mz, lx * lx + lz * lz);
        let y_rot = (m2 > 1.0e-5 && l2 > 1.0e-5).then(|| {
            let dot = (mx * lx + mz * lz) / (m2.sqrt() * l2.sqrt());
            let sign = mx * lz - mz * lx;
            (sign.signum() * dot.abs().min(1.0).acos()) as f32
        });
        let v = self.vel;
        let speed = ((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]) / 0.2) as f32;
        Glide {
            ticks: self.ticks as f32 + partial,
            y_rot,
            speed: (speed * speed * speed).max(1.0),
            wings,
        }
    }
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
            use_ticks: 0,
            use_off: false,
            glide: GlideTrack::default(),
            cloak: CloakTrack::default(),
            boat: BoatTrack::default(),
            display: DisplayTrack::default(),
            display_kind: DisplayKind::None,
            pose_lerp: PoseLerp::default(),
        }
    }

    /// Vanilla replaces the local player on respawn, so a living sample never keeps the death pose.
    fn advance_local(&mut self, local: &LocalSample) {
        if self.age == 0 {
            *self = Track::new(local.feet, local.yaw, local.yaw, local.pitch, true);
        }
        self.advance(
            local.feet,
            local.yaw,
            local.yaw,
            local.pitch,
            local.dead,
            1.0,
        );
        self.glide.tick(local.gliding, local.crouching, local.vel);
        let walked = (self.pos[0] - self.prev_pos[0]).hypot(self.pos[2] - self.prev_pos[2]) as f32;
        let bob = bob_target(local.on_ground, local.dead, local.swimming, local.vel);
        self.cloak.tick(self.prev_pos, bob, walked * 0.6);
        if !local.dead {
            self.death = 0;
        }
    }

    /// `walk_scale` is `WalkAnimationState#update`'s position scale: 3 for babies.
    fn advance(
        &mut self,
        pos: [f64; 3],
        yaw: f32,
        head: f32,
        pitch: f32,
        dead: bool,
        walk_scale: f32,
    ) {
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
        self.walk += self.speed * walk_scale;
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

    fn tick_use(&mut self, flags: u8) {
        let (using, off) = (flags & 1 != 0, flags & 2 != 0);
        self.use_ticks = if using && (self.use_ticks == 0 || off == self.use_off) {
            self.use_ticks.saturating_add(1)
        } else {
            u32::from(using)
        };
        self.use_off = off;
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
    pub gliding: bool,
    pub crouching: bool,
    pub vel: [f64; 3],
    pub on_ground: bool,
    pub swimming: bool,
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
    drawn_list: Vec<(i32, [f64; 3], bool)>,
    texts: TextDisplays,
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
    pub hide_entity: Option<i32>,
    /// Elder guardian curse progress in 0..1, drawn in front of the camera.
    pub ghost: Option<f32>,
    pub eye: [f64; 3],
    pub local_swing: f32,
    pub local_using: bool,
    pub events: &'a crate::entity_events::EntityEvents,
    pub skins: &'a crate::skins::SkinStore,
    /// A spectator holds `key.spectatorOutlines`: every player glows.
    pub spectator_outlines: bool,
    /// `getBackgroundOpacity(0.25)`: the default text display background.
    pub text_background_opacity: f32,
    /// The local player's item use: off hand, then ticks used.
    pub local_use: Option<(bool, f32)>,
    pub entity_shadows: bool,
    /// Sky light the time of day takes away (`Level#getSkyDarken`).
    pub sky_darken: u8,
    /// The dimension type's `ambient_light`.
    pub ambient_light: f32,
    /// `gameplay/piglins_zombify` of the dimension: piglins and hoglins shake.
    pub piglins_zombify: bool,
}

/// `Entity.getTeamColor` when vanilla's `shouldEntityAppearGlowing` holds; `entry` is the
/// scoreboard name (player name or UUID).
fn glow_color(
    cx: &FrameCtx<'_>,
    data: &FxHashMap<u8, EntityDataValue>,
    is_player: bool,
    entry: impl FnOnce() -> Option<String>,
) -> Option<[u8; 3]> {
    let flagged = matches!(data.get(&0), Some(EntityDataValue::Byte(f)) if (*f as u8) & 0x40 != 0);
    if !flagged && !(is_player && cx.spectator_outlines) {
        return None;
    }
    let board = &cx.state.scoreboard;
    let rgb = entry()
        .and_then(|name| board.teams.get(board.team_of(&name)?)?.parameters.color)
        .map_or(0xFF_FFFF, crate::nametags::team_color);
    Some([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8])
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
            texts: TextDisplays::default(),
            last_drawn: 0,
            last_culled: 0,
        })
    }

    pub fn clear(&mut self) {
        self.tracks.clear();
        self.local = Track::new([0.0; 3], 0.0, 0.0, 0.0, true);
    }

    pub fn local_death_time(&self) -> i32 {
        self.local.death
    }

    pub fn on_event(&mut self, event: &ClientEvent, local_id: i32) {
        let (id, f): (i32, Mutation) = match *event {
            ClientEvent::EntitySwing {
                entity_id, offhand, ..
            } => (entity_id, Box::new(move |t| t.start_swing(offhand))),
            ClientEvent::EntityEvent { entity_id, event } => match event {
                33 | 36 | 37 | 44 | 57 => (entity_id, Box::new(|t| t.hurt = HURT_TICKS)),
                3 => (entity_id, Box::new(|t| t.death = t.death.max(1))),
                _ => return,
            },
            ClientEvent::EntityDamaged { entity_id, .. }
            | ClientEvent::EntityHurt { entity_id } => {
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
            let name = data.entity_type_name(e.kind as u32);
            let living = name.is_some_and(is_living);
            let display_kind = match name {
                Some("minecraft:text_display") => DisplayKind::Text,
                Some(n) if n.ends_with("_display") => DisplayKind::Shape,
                _ => DisplayKind::None,
            };
            let dead = health(e).is_some_and(|h| h <= 0.0);
            let tick = self.tick;
            let t = self
                .tracks
                .entry(e.id)
                .or_insert_with(|| Track::new(e.pos, e.yaw, e.head_yaw, e.pitch, living));
            t.seen = tick;
            t.display_kind = display_kind;
            let (pos, yaw, pitch) = if display_kind == DisplayKind::None {
                (e.pos, e.yaw, e.pitch)
            } else {
                t.pose_lerp
                    .step(teleport_duration(&e.data), e.pos, e.yaw, e.pitch)
            };
            let baby = living && mcr_entity_model::is_baby(&EntityView::new(e.kind, &e.data));
            t.advance(
                pos,
                yaw,
                e.head_yaw,
                pitch,
                dead,
                if baby { 3.0 } else { 1.0 },
            );
            if living {
                let flags = match e.data.get(&8) {
                    Some(EntityDataValue::Byte(f)) => *f as u8,
                    _ => 0,
                };
                t.tick_use(flags);
            }
            if display_kind != DisplayKind::None {
                t.display.tick(&e.data);
            }
            if living {
                let gliding = matches!(e.data.get(&0), Some(EntityDataValue::Byte(f)) if *f as u8 & 0x80 != 0);
                let crouching = matches!(
                    e.data.get(&6),
                    Some(EntityDataValue::Pose(DataPose::Crouching))
                );
                t.glide.tick(gliding, crouching, e.velocity);
            }
            if name.is_some_and(is_boat) {
                t.boat.tick(&e.data, t.age);
                t.boat.under_water = eye_in_water(state, e);
            }
            if e.kind == self.player_type {
                let swimming = matches!(
                    e.data.get(&6),
                    Some(EntityDataValue::Pose(DataPose::Swimming))
                );
                let bob = bob_target(e.on_ground, dead, swimming, e.velocity);
                t.cloak.tick(t.prev_pos, bob, 0.0);
            }
        }
        let tick = self.tick;
        self.tracks.retain(|_, t| t.seen == tick);
        let tracks = &self.tracks;
        self.texts.forget_missing(|id| tracks.contains_key(&id));
        self.local.advance_local(&local);
        self.local.hurt = state.player.hurt_time;
    }

    fn refresh_local(&mut self, state: &ClientState) {
        let p = &state.player;
        let physics = &p.physics;
        self.local_data.clone_from(&p.data);
        let mut flags = match p.data.get(&0) {
            Some(EntityDataValue::Byte(server)) => server & 0x61,
            _ => 0i8,
        };
        if physics.crouching {
            flags |= 0x02;
        }
        if physics.sprinting {
            flags |= 0x08;
        }
        if physics.swimming {
            flags |= 0x10;
        }
        if physics.fall_flying {
            flags |= 0x80u8 as i8;
        }
        self.local_data.insert(0, EntityDataValue::Byte(flags));
        let pose = match physics.pose {
            PhysPose::Standing => DataPose::Standing,
            PhysPose::Crouching => DataPose::Crouching,
            PhysPose::Swimming => DataPose::Swimming,
            PhysPose::FallFlying => DataPose::FallFlying,
            PhysPose::SpinAttack => DataPose::SpinAttack,
            PhysPose::Sleeping => DataPose::Sleeping,
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
    pub fn update(&mut self, cx: &FrameCtx<'_>, ui: &mut Ui) {
        self.refresh_local(cx.state);
        let server = match self.local_data.get(&8) {
            Some(EntityDataValue::Byte(f)) => *f as u8 & !3,
            _ => 0,
        };
        let using = cx
            .local_use
            .map_or(0, |(off_hand, _)| 1 | u8::from(off_hand) << 1);
        self.local_data
            .insert(8, EntityDataValue::Byte((server | using) as i8));
        let Self {
            scene,
            tracks,
            local,
            local_data,
            local_equipment,
            player_type,
            scale_attribute,
            drawn_list,
            texts,
            ..
        } = self;
        drawn_list.clear();
        let registries = SyncedRegistries(&cx.state.registries);
        let data = mcr_data::v26_3();
        let culled = std::cell::Cell::new(0usize);
        let drawn = std::cell::Cell::new(0usize);
        let local_id = cx.state.player.entity_id;
        texts.begin();
        let camera_angles = scene.camera_angles();
        for (id, t) in tracks.iter() {
            if t.display_kind != DisplayKind::Text {
                continue;
            }
            let Some(e) = cx.state.entities.get(*id) else {
                continue;
            };
            let Some(sample) = t.display.sample(cx.partial) else {
                continue;
            };
            let pos = entity_pos(cx, data, e, t, *scale_attribute);
            if !display_visible(cx, pos, e) {
                continue;
            }
            let mut view = make_view(e.id, e.uuid, e.kind, t, cx.partial, &e.data, 1.0);
            view.display = Some(sample);
            texts.push(
                ui,
                e,
                &sample,
                display_local(camera_angles, &view),
                pos,
                display_light(e, light_at(&cx.state.chunks, pos, eye_height(data, e.kind))),
                cx.text_background_opacity,
            );
        }
        let local_view = cx.show_local.then(|| {
            let mut v = make_view(
                local_id,
                cx.state.uuid,
                *player_type,
                local,
                cx.partial,
                local_data,
                cx.state.player.physics.attributes.scale.clamp(0.0625, 16.0) as f32,
            );
            v.equipment = Some(&*local_equipment);
            v.bed = bed_of(cx.state, &v);
            v.passenger = cx.state.player.vehicle.is_some();
            v.use_ticks = cx.local_use.map_or(0.0, |(_, ticks)| ticks);
            apply_events(&mut v, cx.events.view(local_id, cx.partial));
            v.attack_time = cx.local_swing;
            v.player = PlayerView {
                crouching: cx.state.player.physics.crouching,
                swimming: cx.state.player.physics.swimming,
                using_item: cx.local_using,
                skin: cx.skins.get(cx.state.uuid),
                cloak: cloak_of(local, &v, cx.partial),
                extra_ears: cx.state.name == "deadmau5",
                ..PlayerView::default()
            };
            v.light = light_at(&cx.state.chunks, v.pos, eye_height(data, *player_type));
            if v.shared_flag(0) {
                v.light[0] = 15;
            }
            v.outline = glow_color(cx, local_data, true, || Some(cx.state.name.clone()));
            let spectator = cx.state.player.game_mode == GameType::Spectator;
            decorate(cx, &mut v, "minecraft:player", true, spectator, || {
                Some(cx.state.name.clone())
            });
            v.upside_down = is_upside_down_name(&cx.state.name) && v.player_part(0);
            flip_head(&mut v);
            v
        });
        let views = tracks
            .iter()
            .filter_map(|(id, t)| {
                if cx.hide_entity == Some(*id) {
                    return None;
                }
                let e = cx.state.entities.get(*id)?;
                let name = data.entity_type_name(e.kind as u32).unwrap_or("");
                if t.age < 2
                    && hidden_while_fresh(name)
                    && dist_sq(e.pos, cx.state.player.pos()) < FRESH_HIDE_DIST_SQ
                {
                    return None;
                }
                let (w, h) = type_size(data, e.kind);
                let scale = scale_attribute
                    .and_then(|a| e.attributes.get(&a))
                    .map_or(1.0, |s| s.value(None) as f32)
                    .clamp(0.0625, 16.0);
                let pos = entity_pos(cx, data, e, t, *scale_attribute);
                let in_view = match t.display_kind {
                    DisplayKind::None => {
                        visible(cx, state_chunks(cx.state), pos, w * scale, h * scale)
                    }
                    _ => display_visible(cx, pos, e),
                };
                if !in_view {
                    culled.set(culled.get() + 1);
                    return None;
                }
                drawn.set(drawn.get() + 1);
                drawn_list.push((
                    e.id,
                    pos,
                    matches!(
                        e.data.get(&6),
                        Some(EntityDataValue::Pose(DataPose::Crouching))
                    ),
                ));
                let mut view = make_view(e.id, e.uuid, e.kind, t, cx.partial, &e.data, scale);
                view.pos = pos;
                view.passenger = e.vehicle.is_some();
                view.on_ground = e.on_ground || (t.pos[1] - t.prev_pos[1]).abs() < 1.0e-3;
                view.object_data = e.object_data;
                view.equipment = Some(&e.equipment);
                view.bed = bed_of(cx.state, &view);
                view.flight = e.flight.as_ref().map(|f| f.lean(cx.partial));
                view.leash = leash_line(cx, tracks, data, e, &view, scale);
                view.fuse = e.fuse_remaining(cx.partial);
                view.swelling = e.swelling(cx.partial);
                if is_boat(name) || name.ends_with("minecart") {
                    view.vehicle = vehicle_anim(name, &e.data, t, cx.partial);
                }
                if name.ends_with("minecart") {
                    view.rail = rail_pose(pos, |b| rail_shape(&cx.state.chunks, b));
                }
                apply_events(&mut view, cx.events.view(e.id, cx.partial));
                if e.kind == *player_type {
                    view.player = PlayerView {
                        crouching: view.pose() == DataPose::Crouching,
                        swimming: view.pose() == DataPose::Swimming,
                        using_item: view.living_flag(0),
                        skin: cx.skins.get(e.uuid),
                        cloak: cloak_of(t, &view, cx.partial),
                        extra_ears: cx.state.players.name_of(e.uuid) == Some("deadmau5"),
                        ..PlayerView::default()
                    };
                }
                view.light = light_at(&cx.state.chunks, pos, eye_height(data, e.kind) * scale);
                if view.shared_flag(0) {
                    view.light[0] = 15;
                }
                if t.display_kind == DisplayKind::Shape {
                    view.display = t.display.sample(cx.partial);
                    view.light = display_light(e, view.light);
                }
                let is_player = e.kind == *player_type;
                let entry = || {
                    if is_player {
                        cx.state.players.name_of(e.uuid).map(str::to_owned)
                    } else {
                        Some(e.uuid.to_string())
                    }
                };
                view.outline = glow_color(cx, &e.data, is_player, entry);
                let spectator = is_player
                    && cx
                        .state
                        .players
                        .get(e.uuid)
                        .is_some_and(|p| p.game_mode == GameType::Spectator);
                decorate(cx, &mut view, name, is_living(name), spectator, entry);
                view.upside_down = if is_player {
                    cx.state
                        .players
                        .name_of(e.uuid)
                        .is_some_and(is_upside_down_name)
                        && view.player_part(0)
                } else {
                    is_living(name)
                        && crate::nametags::custom_name(e)
                            .is_some_and(|c| is_upside_down_name(&c.to_plain_string()))
                };
                flip_head(&mut view);
                Some(view)
            })
            .chain(local_view)
            .chain(cx.ghost.map(|progress| {
                let mut view = EntityView::named("minecraft:elder_guardian", &NoData);
                view.pos = cx.eye;
                view.ghost = Some(progress.clamp(0.0, 1.0));
                view.light = [15, 15];
                view
            }))
            .chain(cx.state.transient_blocks.iter().filter_map(|b| {
                let pos = [
                    f64::from(b.pos[0]) + 0.5,
                    f64::from(b.pos[1]),
                    f64::from(b.pos[2]) + 0.5,
                ];
                if !in_view(cx.cull, pos, 1.0, 1.0) {
                    return None;
                }
                let mut view = EntityView::named("minecraft:falling_block", &NoData);
                view.pos = pos;
                view.object_data = b.state;
                view.light = light_at(&cx.state.chunks, pos, 0.5);
                Some(view)
            }))
            .chain(cx.state.pickups.iter().filter_map(|p| {
                let e = &p.entity;
                let (w, h) = type_size(data, e.kind);
                if !in_view(cx.cull, p.position(cx.partial), w, h) {
                    culled.set(culled.get() + 1);
                    return None;
                }
                drawn.set(drawn.get() + 1);
                let mut t = Track::new(e.pos, e.yaw, e.head_yaw, e.pitch, false);
                t.age = e.age_ticks();
                let mut view = make_view(e.id, e.uuid, e.kind, &t, 1.0, &e.data, 1.0);
                view.pos = p.position(cx.partial);
                view.object_data = e.object_data;
                view.light = light_at(&cx.state.chunks, view.pos, 0.0);
                Some(view)
            }));
        let chunks = &cx.state.chunks;
        scene.set_shadows(cx.entity_shadows);
        let (darken, ambient) = (cx.sky_darken, cx.ambient_light);
        scene.update_grounded(&registries, views, &|x, y, z| {
            shadow_ground(chunks, [x, y, z], darken, ambient)
        });
        self.last_drawn = drawn.get();
        self.last_culled = culled.get();
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

    pub fn draw_decals(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.has_parts() {
            self.scene.draw_decals(pass);
        }
    }

    pub fn prepare_oit(&mut self, info: mcr_render::OitInfo<'_>) -> bool {
        self.has_parts() && self.scene.prepare_oit(info)
    }

    pub fn draw_oit(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        stage: mcr_render::OitStage,
        oit: &wgpu::BindGroup,
    ) {
        self.scene.draw_oit(pass, stage, oit);
    }

    pub fn take_text_draws(&mut self) -> Vec<mcr_block_entity::TextDisplayDraw> {
        self.texts.take()
    }

    pub fn has_outlines(&self) -> bool {
        self.has_parts() && self.scene.has_outlines()
    }

    pub fn draw_outlines(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.scene.draw_outlines(pass);
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

    /// The local player's body yaw in degrees, as the entity renderer turns it.
    pub fn local_body_yaw(&self, partial: f32) -> f32 {
        lerp_degrees(self.local.prev_body, self.local.body, partial)
    }

    pub fn drawn_entities(
        &self,
        _state: &ClientState,
        _partial: f32,
    ) -> impl Iterator<Item = (i32, [f64; 3], bool)> + '_ {
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
                fog: [env.fog_start, env.fog_end, rd_start, rd_end],
                fog_color: env.fog_color,
                ..LightInfo::with_lightmap(&env.lightmap(1.0))
            },
        }
    }
}

/// `EntityRenderer#extractShadowPiece`: a visible full block below and raw light above 3 at `p`.
fn shadow_ground(chunks: &ChunkStore, p: [i32; 3], sky_darken: u8, ambient: f32) -> Option<f32> {
    let data = mcr_data::v26_3();
    let below = chunks.block(p[0], p[1] - 1, p[2])?;
    let info = data.block_state(below.0)?;
    if info.is_air() || data.block_name(info.block) == Some("minecraft:barrier") {
        return None;
    }
    let full = matches!(
        data.collision_shape(below.0),
        [b] if b.min == [0.0; 3] && b.max == [1.0; 3]
    );
    if !full {
        return None;
    }
    let column = chunks.column(ChunkPos::new(p[0] >> 4, p[2] >> 4))?;
    let (lx, lz) = ((p[0] & 15) as usize, (p[2] & 15) as usize);
    let block = column.block_light(lx, p[1], lz).unwrap_or(0);
    let sky = column.sky_light_or_above(lx, p[1], lz).unwrap_or(15);
    let raw = block.max(sky.saturating_sub(sky_darken));
    if raw <= 3 {
        return None;
    }
    let v = f32::from(raw) / 15.0;
    let curved = v / (4.0 - 3.0 * v);
    Some(ambient + (1.0 - ambient) * curved)
}

/// An upside-down entity looks the other way relative to its body.
fn flip_head(v: &mut EntityView<'_>) {
    if v.upside_down {
        v.pitch = -v.pitch;
        v.head_yaw = 2.0 * v.body_yaw - v.head_yaw;
    }
}

fn is_upside_down_name(name: &str) -> bool {
    name == "Dinnerbone" || name == "Grumm"
}

/// Burning only without fire immunity (`EntityType#fireImmune`).
fn fire_immune(name: &str) -> bool {
    matches!(
        name.strip_prefix("minecraft:").unwrap_or(name),
        "area_effect_cloud"
            | "blaze"
            | "ender_dragon"
            | "end_crystal"
            | "ghast"
            | "magma_cube"
            | "shulker"
            | "strider"
            | "tnt"
            | "vex"
            | "warden"
            | "wither"
            | "wither_skeleton"
            | "zoglin"
            | "zombified_piglin"
    )
}

/// Render state vanilla derives per frame: invisibility, fire, shaking and walk for riders and the dead.
fn decorate(
    cx: &FrameCtx<'_>,
    v: &mut EntityView<'_>,
    name: &str,
    living: bool,
    spectator: bool,
    entry: impl FnOnce() -> Option<String>,
) {
    v.body = body_visibility(cx, v.invisible && living, entry);
    v.on_fire = v.shared_flag(0) && !spectator && !fire_immune(name);
    if !living {
        return;
    }
    if v.passenger || v.death_time > 0.0 || v.float(9).is_some_and(|h| h <= 0.0) {
        v.limb_swing = 0.0;
        v.limb_amount = 0.0;
    }
    let flag = |index: u8| v.boolean(index);
    let short = name.strip_prefix("minecraft:").unwrap_or(name);
    let converting = match short {
        "zombie" | "husk" | "drowned" => flag(18),
        "zombie_villager" => flag(idx::ZOMBIE_VILLAGER_CONVERTING),
        "skeleton" => flag(16),
        "strider" => flag(idx::STRIDER_SUFFOCATING),
        "piglin" | "piglin_brute" => cx.piglins_zombify && !flag(16) && !v.mob_flag(0),
        "hoglin" => cx.piglins_zombify && !flag(18) && !v.mob_flag(0),
        _ => false,
    };
    v.shaking = converting || v.int(7).is_some_and(|t| t >= 140);
}

/// `LivingEntityRenderer#getRenderType`: `Entity#isInvisibleTo` the local player.
fn body_visibility(
    cx: &FrameCtx<'_>,
    invisible: bool,
    entry: impl FnOnce() -> Option<String>,
) -> BodyVisibility {
    if !invisible {
        return BodyVisibility::Visible;
    }
    if cx.state.player.game_mode == GameType::Spectator {
        return BodyVisibility::Translucent;
    }
    let board = &cx.state.scoreboard;
    let friendly = entry()
        .and_then(|name| {
            let team = board.team_of(&name)?;
            (board.team_of(&cx.state.name) == Some(team))
                .then(|| board.teams.get(team))
                .flatten()
        })
        .is_some_and(|t| t.parameters.options & 2 != 0);
    if friendly {
        BodyVisibility::Translucent
    } else {
        BodyVisibility::Hidden
    }
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

fn leash_line(
    cx: &FrameCtx<'_>,
    tracks: &FxHashMap<i32, Track>,
    data: &'static mcr_data::Version,
    e: &mcr_client::Entity,
    view: &EntityView<'_>,
    scale: f32,
) -> Option<([f64; 3], [f64; 3])> {
    let holder_id = e.leash_holder?;
    let holder = cx.state.entities.get(holder_id);
    let holder_pos = if holder_id == cx.state.player.entity_id {
        cx.state.player.pos()
    } else {
        let h = holder?;
        tracks
            .get(&holder_id)
            .map_or(h.pos, |t| lerp3(t.prev_pos, t.pos, f64::from(cx.partial)))
    };
    let knot = holder
        .is_some_and(|h| data.entity_type_name(h.kind as u32) == Some("minecraft:leash_knot"));
    let holder_eye = holder.map_or(1.62, |h| eye_height(data, h.kind));
    let end_lift = if knot {
        0.2
    } else {
        f64::from(holder_eye) * 0.7
    };
    let (width, _) = type_size(data, e.kind);
    let offset = [
        0.0,
        f64::from(eye_height(data, e.kind) * scale),
        f64::from(width * scale * 0.4),
    ];
    let yaw = -f64::from(view.body_yaw).to_radians();
    let (sin, cos) = yaw.sin_cos();
    let start = [
        view.pos[0] + offset[0] * cos + offset[2] * sin,
        view.pos[1] + offset[1],
        view.pos[2] + offset[2] * cos - offset[0] * sin,
    ];
    Some((
        start,
        [holder_pos[0], holder_pos[1] + end_lift, holder_pos[2]],
    ))
}

pub fn eye_position(data: &'static mcr_data::Version, e: &mcr_client::Entity) -> [f64; 3] {
    [
        e.pos[0],
        e.pos[1] + f64::from(eye_height(data, e.kind)),
        e.pos[2],
    ]
}

fn eye_height(data: &'static mcr_data::Version, kind: i32) -> f32 {
    u32::try_from(kind)
        .ok()
        .and_then(|k| data.entity_type_info(k))
        .map_or(1.62, |i| i.eye_height)
}

fn type_size(data: &'static mcr_data::Version, kind: i32) -> (f32, f32) {
    u32::try_from(kind)
        .ok()
        .and_then(|k| data.entity_type_info(k))
        .map_or((0.6, 1.8), |i| (i.width, i.height))
}

/// Where the entity is drawn: interpolated, or on its vehicle's seat.
fn entity_pos(
    cx: &FrameCtx<'_>,
    data: &'static mcr_data::Version,
    e: &Entity,
    t: &Track,
    scale_attribute: Option<i32>,
) -> [f64; 3] {
    let pos = lerp3(t.prev_pos, t.pos, f64::from(cx.partial));
    let Some(vehicle) = e.vehicle.and_then(|v| cx.state.entities.get(v)) else {
        return pos;
    };
    let scale = |e: &Entity| {
        scale_attribute
            .and_then(|a| e.attributes.get(&a))
            .map_or(1.0, |s| s.value(None) as f32)
    };
    let seat = vehicle
        .passengers
        .iter()
        .position(|&p| p == e.id)
        .unwrap_or(0);
    let seat = mcr_client::attach::offset(
        &mcr_client::attach::Vehicle::of(data, vehicle, scale(vehicle)),
        seat,
        &mcr_client::attach::Rider::of(data, e, scale(e)),
    );
    [
        vehicle.pos[0] + seat[0],
        vehicle.pos[1] + seat[1],
        vehicle.pos[2] + seat[2],
    ]
}

/// `Display.getPackedBrightnessOverride`: `block << 4 | sky << 20`, or -1 for the world light.
fn display_light(e: &Entity, world: [u8; 2]) -> [u8; 2] {
    match e.data.get(&idx::DISPLAY_BRIGHTNESS) {
        Some(EntityDataValue::Int(v)) if *v != -1 => {
            [((*v >> 4) & 15) as u8, ((*v >> 20) & 15) as u8]
        }
        _ => world,
    }
}

fn display_visible(cx: &FrameCtx<'_>, pos: [f64; 3], e: &Entity) -> bool {
    if !pos.iter().all(|c| c.is_finite() && c.abs() < 3.0e7) {
        return false;
    }
    let chunk = ChunkPos::new((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4);
    if state_chunks(cx.state).column(chunk).is_none() {
        return false;
    }
    let float = |index: u8, default: f32| match e.data.get(&index) {
        Some(EntityDataValue::Float(v)) if v.is_finite() => *v,
        _ => default,
    };
    display_in_view(
        cx.cull,
        pos,
        float(idx::DISPLAY_VIEW_RANGE, 1.0),
        float(idx::DISPLAY_WIDTH, 0.0),
        float(idx::DISPLAY_HEIGHT, 0.0),
    )
}

/// `Display.shouldRenderAtSqrDistance` and the culling box; a zero width or height disables culling.
pub fn display_in_view(cull: &Cull, pos: [f64; 3], view_range: f32, w: f32, h: f32) -> bool {
    let rel = [
        pos[0] - cull.camera_pos[0],
        pos[1] - cull.camera_pos[1],
        pos[2] - cull.camera_pos[2],
    ];
    let limit = f64::from(view_range.clamp(0.0, 1024.0)) * 64.0 * cull.distance_scale;
    if rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2] >= limit * limit {
        return false;
    }
    let (w, h) = (w.clamp(0.0, 1024.0), h.clamp(0.0, 1024.0));
    if w == 0.0 || h == 0.0 {
        return true;
    }
    let half = w / 2.0 + CULL_MARGIN;
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
    cull.frustum.intersects_aabb(min, max)
}

fn dist_sq(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]) * (a[i] - b[i])).sum()
}

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn visible(cx: &FrameCtx<'_>, chunks: &ChunkStore, pos: [f64; 3], w: f32, h: f32) -> bool {
    if !pos.iter().all(|c| c.is_finite() && c.abs() < 3.0e7) {
        return false;
    }
    let chunk = ChunkPos::new((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4);
    chunks.column(chunk).is_some() && in_view(cx.cull, pos, w, h)
}

/// Vanilla `shouldRender` range (mean box edge x 64 x scale), then the box inflated by 0.5 against the frustum.
pub fn in_view(cull: &Cull, pos: [f64; 3], w: f32, h: f32) -> bool {
    if !pos.iter().all(|c| c.is_finite() && c.abs() < 3.0e7) {
        return false;
    }
    let rel = [
        pos[0] - cull.camera_pos[0],
        pos[1] - cull.camera_pos[1],
        pos[2] - cull.camera_pos[2],
    ];
    let size = f64::from((w + h + w) / 3.0).max(0.25);
    let limit = size * 64.0 * cull.distance_scale;
    if rel[0] * rel[0] + rel[1] * rel[1] + rel[2] * rel[2] > limit * limit {
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
    cull.frustum.intersects_aabb(min, max)
}

/// `[block, sky]` at the entity's eye, which is where vanilla probes the light.
pub fn light_at(chunks: &ChunkStore, pos: [f64; 3], eye_height: f32) -> [u8; 2] {
    let x = pos[0].floor() as i32;
    let y = (pos[1] + f64::from(eye_height)).floor() as i32;
    let z = pos[2].floor() as i32;
    let Some(column) = chunks.column(ChunkPos::new(x >> 4, z >> 4)) else {
        return [0, 15];
    };
    let (lx, lz) = ((x & 15) as usize, (z & 15) as usize);
    [
        column.block_light(lx, y, lz).unwrap_or(0),
        column.sky_light_or_above(lx, y, lz).unwrap_or(15),
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
    v.use_ticks = if t.use_ticks > 0 {
        t.use_ticks as f32 - 1.0 + partial
    } else {
        0.0
    };
    let gliding = v.shared_flag(7) || v.pose() == DataPose::FallFlying;
    v.glide = t.glide.view(partial, v.yaw, v.pitch, gliding);
    v
}

fn cloak_of(t: &Track, v: &EntityView<'_>, partial: f32) -> [f32; 3] {
    let pos = lerp3(t.prev_pos, t.pos, f64::from(partial));
    t.cloak.view(partial, pos, v.body_yaw, v.glide.scale())
}

/// The bed a living entity in the sleeping pose lies in (its synced sleeping position, index 14).
fn bed_of(state: &ClientState, v: &EntityView<'_>) -> Option<mcr_entity_model::BedFacing> {
    use mcr_entity_model::BedFacing as B;
    if v.pose() != DataPose::Sleeping {
        return None;
    }
    let Some(EntityDataValue::OptionalBlockPos(Some(pos))) = v.data.get(14) else {
        return None;
    };
    Some(match state.bed_orientation(*pos)? {
        Facing::North => B::North,
        Facing::South => B::South,
        Facing::West => B::West,
        Facing::East => B::East,
        Facing::Down | Facing::Up => return None,
    })
}

fn apply_events(v: &mut EntityView<'_>, ev: crate::entity_events::EventView) {
    v.events = ev.anim;
    v.attack_ticks = ev.attack_ticks;
    v.offer_ticks = ev.offer_ticks;
    v.shake = ev.shake;
}

/// View-projection of the vanilla hand: `fov` degrees (70 unless dying or in a fluid), no world
/// rotation.
pub fn hand_view_proj(aspect: f32, fov: f32) -> Mat4 {
    mcr_render::glam::camera::rh::proj::directx::perspective_infinite_reverse(
        fov.clamp(1.0, 170.0).to_radians(),
        aspect,
        0.05,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cull_with(scale: f64) -> Cull {
        Cull {
            camera_pos: [0.0; 3],
            frustum: Camera::default().frustum(16.0 / 9.0),
            distance_scale: scale,
        }
    }

    #[test]
    fn entity_frustum_with_half_block_margin() {
        let cull = cull_with(1.0);
        assert!(in_view(&cull, [0.0, 0.0, -10.0], 0.6, 1.8));
        assert!(!in_view(&cull, [0.0, 0.0, 10.0], 0.6, 1.8));
        assert!(!in_view(&cull, [50.0, 0.0, -10.0], 0.6, 1.8));
        assert!(!in_view(&cull, [0.0, 50.0, -10.0], 0.6, 1.8));
        assert!(in_view(&cull, [0.0, -2.0, -10.0], 0.6, 5.0));
    }

    #[test]
    fn entity_distance_follows_box_size_and_scaling() {
        let near = cull_with(1.0);
        assert!(in_view(&near, [0.0, 0.0, -60.0], 0.6, 1.8));
        assert!(!in_view(&near, [0.0, 0.0, -70.0], 0.6, 1.8));
        assert!(in_view(&near, [0.0, 0.0, -200.0], 4.0, 4.0));
        assert!(!in_view(&near, [0.0, 0.0, -300.0], 4.0, 4.0));
        assert!(in_view(&cull_with(5.0), [0.0, 0.0, -300.0], 0.6, 1.8));
        assert!(!in_view(&cull_with(0.0), [0.0, 0.0, -5.0], 0.6, 1.8));
    }

    #[test]
    fn hostile_positions_are_culled() {
        let cull = cull_with(1.0);
        assert!(!in_view(&cull, [f64::NAN, 0.0, -5.0], 0.6, 1.8));
        assert!(!in_view(&cull, [0.0, f64::INFINITY, -5.0], 0.6, 1.8));
        assert!(!in_view(&cull, [0.0, 0.0, -1.0e9], 0.6, 1.8));
    }

    #[test]
    fn local_death_pose_ends_when_alive_again() {
        let sample = |dead| LocalSample {
            feet: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            dead,
            gliding: false,
            crouching: false,
            vel: [0.0; 3],
            on_ground: true,
            swimming: false,
        };
        let mut t = Track::new([0.0; 3], 0.0, 0.0, 0.0, true);
        t.death = 1;
        for _ in 0..40 {
            t.advance_local(&sample(true));
        }
        assert_eq!(t.death, DEATH_TICKS);
        t.advance_local(&sample(false));
        assert_eq!(t.death, 0);
    }

    #[test]
    fn angles_wrap() {
        assert!((lerp_degrees(350.0, 10.0, 0.5) - 360.0).abs() < 1e-3);
        assert!((wrap_degrees(190.0) + 170.0).abs() < 1e-4);
    }

    #[test]
    fn walk_speed_converges_when_moving() {
        let mut t = Track::new([0.0; 3], 0.0, 0.0, 0.0, true);
        for i in 1..=20 {
            t.advance([f64::from(i) * 0.25, 0.0, 0.0], 0.0, 0.0, 0.0, false, 1.0);
        }
        assert!(t.speed > 0.95, "{}", t.speed);
        assert!(t.walk > 10.0);
    }

    #[test]
    fn body_follows_travel_direction() {
        let mut t = Track::new([0.0; 3], 0.0, 0.0, 0.0, true);
        for i in 1..=40 {
            t.advance([0.0, 0.0, f64::from(i) * 0.3], 0.0, 0.0, 0.0, false, 1.0);
        }
        assert!(wrap_degrees(t.body).abs() < 2.0, "{}", t.body);
    }

    #[test]
    fn head_stays_within_limit_of_body() {
        let mut t = Track::new([0.0; 3], 0.0, 0.0, 0.0, true);
        t.advance([0.0; 3], 170.0, 170.0, 0.0, false, 1.0);
        assert!(wrap_degrees(t.yaw - t.body).abs() <= HEAD_BODY_LIMIT + 1e-3);
    }

    #[test]
    fn projectiles_are_not_living() {
        assert!(!is_living("minecraft:arrow"));
        assert!(!is_living("minecraft:item"));
        assert!(is_living("minecraft:zombie"));
        assert!(is_living("minecraft:villager"));
    }

    #[test]
    fn elytra_wings_spread_toward_vanilla_targets() {
        let mut g = GlideTrack::default();
        for _ in 0..40 {
            g.tick(true, false, [1.0, 0.0, 0.0]);
        }
        let w = g.view(1.0, 0.0, 0.0, true).wings;
        assert!(
            (w[0] - PI / 9.0).abs() < 1e-4 && (w[2] + PI / 2.0).abs() < 1e-4,
            "{w:?}"
        );
        for _ in 0..40 {
            g.tick(true, false, [0.0, -1.0, 0.0]);
        }
        let w = g.view(1.0, 0.0, 0.0, true).wings;
        assert!(
            (w[0] - PI / 12.0).abs() < 1e-4 && (w[2] + PI / 12.0).abs() < 1e-4,
            "{w:?}"
        );
        for _ in 0..40 {
            g.tick(false, true, [0.0; 3]);
        }
        let w = g.view(1.0, 0.0, 0.0, false).wings;
        assert!((w[1] - 0.087_266_46).abs() < 1e-4 && (w[2] + PI / 4.0).abs() < 1e-4);
    }

    #[test]
    fn glide_view_counts_ticks_rolls_and_damps_limbs() {
        let mut g = GlideTrack::default();
        for _ in 0..3 {
            g.tick(true, false, [1.0, 0.0, 0.0]);
        }
        let v = g.view(0.5, 0.0, 0.0, true);
        assert_eq!(v.ticks, 3.5);
        assert!((v.y_rot.unwrap() - PI / 2.0).abs() < 1e-5, "{:?}", v.y_rot);
        assert!((v.speed - 125.0).abs() < 1e-3);
        let left = g.view(0.5, 90.0, 0.0, true);
        assert!(
            left.y_rot.unwrap().abs() < 1e-3,
            "flying where it looks: no roll"
        );
        g.tick(false, false, [0.0; 3]);
        let v = g.view(0.5, 0.0, 0.0, false);
        assert_eq!((v.ticks, v.y_rot, v.speed), (0.0, None, 1.0));
    }
}
