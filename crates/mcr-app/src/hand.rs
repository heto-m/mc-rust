//! The first-person hand: arm, held items and their equip, swing and use animations.

use std::f32::consts::PI;

use mcr_client::ClientState;
use mcr_entity_model::glam::{Mat4, Vec3};
use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::component::{DataComponent, component_type_id};
use mcr_entity_model::wgpu;
use mcr_entity_model::{
    EntityScene, EntityView, FrameParams, ItemContext, LightInfo, NoData, SceneAssets,
    TargetFormats,
};
use mcr_render::OverlayCtx;
use mcr_settings::Options;

use crate::entities::{SyncedRegistries, hand_view_proj};

const MAX_HEIGHT_STEP: f32 = 0.4;
const DEFAULT_SWING_TICKS: f32 = 6.0;
const DEFAULT_ATTACK_SPEED: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseKind {
    None,
    Eat,
    Drink,
    Bow,
    Crossbow,
    Trident,
    Block,
    Spyglass,
    Brush,
    Horn,
}

impl UseKind {
    pub fn of(name: &str) -> Self {
        match name.strip_prefix("minecraft:").unwrap_or(name) {
            "bow" => Self::Bow,
            "crossbow" => Self::Crossbow,
            "trident" => Self::Trident,
            "shield" => Self::Block,
            "spyglass" => Self::Spyglass,
            "brush" => Self::Brush,
            "goat_horn" => Self::Horn,
            "potion" | "milk_bucket" | "honey_bottle" | "ominous_bottle" => Self::Drink,
            n if is_edible(n) => Self::Eat,
            _ => Self::None,
        }
    }

    /// Ticks the item can be used for (vanilla `useDuration`).
    pub fn duration(self, name: &str) -> f32 {
        match (self, name.strip_prefix("minecraft:").unwrap_or(name)) {
            (Self::Eat | Self::Drink, "dried_kelp") => 16.0,
            (Self::Drink, "honey_bottle") => 40.0,
            (Self::Eat | Self::Drink, _) => 32.0,
            (Self::Brush, _) => 200.0,
            (Self::Horn, _) => 140.0,
            (Self::None, _) => 0.0,
            _ => 72000.0,
        }
    }
}

fn is_edible(name: &str) -> bool {
    const EXACT: &[&str] = &[
        "apple",
        "golden_apple",
        "enchanted_golden_apple",
        "melon_slice",
        "sweet_berries",
        "glow_berries",
        "carrot",
        "golden_carrot",
        "potato",
        "baked_potato",
        "poisonous_potato",
        "beetroot",
        "dried_kelp",
        "bread",
        "cookie",
        "pumpkin_pie",
        "cake",
        "beef",
        "porkchop",
        "mutton",
        "chicken",
        "rabbit",
        "cod",
        "salmon",
        "tropical_fish",
        "pufferfish",
        "rotten_flesh",
        "spider_eye",
        "chorus_fruit",
        "suspicious_stew",
        "mushroom_stew",
        "beetroot_soup",
        "rabbit_stew",
        "honey_bottle",
        "pumpkin_seeds_cooked",
    ];
    EXACT.contains(&name) || name.starts_with("cooked_")
}

fn name_of(stack: &ItemStack) -> &'static str {
    u32::try_from(stack.item)
        .ok()
        .and_then(|i| mcr_data::v26_3().item_name(i))
        .unwrap_or("")
}

fn charged(stack: &ItemStack) -> bool {
    component_type_id("charged_projectiles")
        .and_then(|id| stack.components.get(id))
        .is_some_and(|c| matches!(c, DataComponent::ChargedProjectiles(list) if !list.0.is_empty()))
}

fn same_item(a: &ItemStack, b: &ItemStack) -> bool {
    a.is_empty() && b.is_empty() || (!a.is_empty() && !b.is_empty() && a.item == b.item)
}

/// A pose stack with vanilla's operation order: every operation applies in the current local frame.
#[derive(Clone, Copy)]
struct Pose(Mat4);

impl Pose {
    fn translate(&mut self, x: f32, y: f32, z: f32) {
        self.0 *= Mat4::from_translation(Vec3::new(x, y, z));
    }

    fn scale(&mut self, x: f32, y: f32, z: f32) {
        self.0 *= Mat4::from_scale(Vec3::new(x, y, z));
    }

    fn rot_x(&mut self, degrees: f32) {
        self.0 *= Mat4::from_rotation_x(degrees.to_radians());
    }

    fn rot_y(&mut self, degrees: f32) {
        self.0 *= Mat4::from_rotation_y(degrees.to_radians());
    }

    fn rot_z(&mut self, degrees: f32) {
        self.0 *= Mat4::from_rotation_z(degrees.to_radians());
    }
}

fn wrap_degrees(v: f32) -> f32 {
    let r = v.rem_euclid(360.0);
    if r >= 180.0 { r - 360.0 } else { r }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Everything the animation needs from the world each frame.
pub struct HandFrame<'a> {
    pub state: &'a ClientState,
    pub options: &'a Options,
    pub partial: f32,
    pub aspect: f32,
    pub pitch: f32,
    pub yaw: f32,
    pub sky_brightness: f32,
    pub light: [u8; 2],
    pub invisible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Right,
    Left,
}

impl Side {
    fn sign(self) -> f32 {
        match self {
            Side::Right => 1.0,
            Side::Left => -1.0,
        }
    }

    fn context(self) -> ItemContext {
        match self {
            Side::Right => ItemContext::FirstPersonRight,
            Side::Left => ItemContext::FirstPersonLeft,
        }
    }
}

pub struct Hand {
    scene: EntityScene,
    main_item: ItemStack,
    off_item: ItemStack,
    main_height: f32,
    old_main_height: f32,
    off_height: f32,
    old_off_height: f32,
    swing_time: i32,
    swinging: bool,
    attack_anim: f32,
    old_attack_anim: f32,
    attack_ticker: u32,
    use_elapsed: u32,
    using: Option<UseKind>,
    x_bob: f32,
    old_x_bob: f32,
    y_bob: f32,
    old_y_bob: f32,
    walk_dist: f32,
    old_walk_dist: f32,
    bob: f32,
    old_bob: f32,
    last_feet: [f64; 3],
    pub drawn: bool,
}

impl std::fmt::Debug for Hand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hand").finish_non_exhaustive()
    }
}

pub struct HandTick<'a> {
    pub state: &'a ClientState,
    pub yaw: f32,
    pub pitch: f32,
    pub local_using: bool,
    pub server_using: bool,
}

impl Hand {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color: wgpu::TextureFormat,
        assets: std::sync::Arc<SceneAssets>,
    ) -> Option<Self> {
        let formats = TargetFormats {
            color,
            depth: wgpu::TextureFormat::Depth32Float,
            reverse_z: true,
            sample_count: 1,
        };
        let scene = EntityScene::new(device, queue, formats, assets)
            .map_err(|e| eprintln!("hand: {e}"))
            .ok()?;
        Some(Self {
            scene,
            main_item: ItemStack::default(),
            off_item: ItemStack::default(),
            main_height: 0.0,
            old_main_height: 0.0,
            off_height: 1.0,
            old_off_height: 1.0,
            swing_time: 0,
            swinging: false,
            attack_anim: 0.0,
            old_attack_anim: 0.0,
            attack_ticker: 100,
            use_elapsed: 0,
            using: None,
            x_bob: 0.0,
            old_x_bob: 0.0,
            y_bob: 0.0,
            old_y_bob: 0.0,
            walk_dist: 0.0,
            old_walk_dist: 0.0,
            bob: 0.0,
            old_bob: 0.0,
            last_feet: [0.0; 3],
            drawn: false,
        })
    }

    /// Starts the swing animation (attack, place, interact).
    pub fn swing(&mut self) {
        let duration = DEFAULT_SWING_TICKS as i32;
        if !self.swinging || self.swing_time >= duration / 2 || self.swing_time < 0 {
            self.swing_time = -1;
            self.swinging = true;
        }
    }

    pub fn reset_attack_strength(&mut self) {
        self.attack_ticker = 0;
    }

    fn attack_speed(state: &ClientState) -> f32 {
        mcr_data::v26_3()
            .registry("minecraft:attribute")
            .and_then(|r| r.id_of("minecraft:attack_speed"))
            .and_then(|id| state.player.attributes.get(&(id as i32)))
            .map_or(DEFAULT_ATTACK_SPEED, |a| a.value(None) as f32)
            .clamp(0.1, 1024.0)
    }

    fn attack_strength(&self, state: &ClientState) -> f32 {
        let delay = 20.0 / Self::attack_speed(state);
        ((self.attack_ticker as f32 + 0.5) / delay).clamp(0.0, 1.0)
    }

    pub fn tick(&mut self, t: &HandTick<'_>) {
        let state = t.state;
        let inv = &state.inventory;
        let main = inv.hotbar(state.player.held_slot).clone();
        let off = mcr_client::inventory_index_to_menu_slot(40)
            .and_then(|i| inv.player.slots.get(i))
            .cloned()
            .unwrap_or_default();
        self.attack_ticker = self.attack_ticker.saturating_add(1);
        self.old_main_height = self.main_height;
        self.old_off_height = self.off_height;
        let strength = self.attack_strength(state);
        let main_target = if same_item(&self.main_item, &main) {
            strength * strength * strength
        } else {
            0.0
        };
        let off_target = if same_item(&self.off_item, &off) {
            1.0
        } else {
            0.0
        };
        self.main_height +=
            (main_target - self.main_height).clamp(-MAX_HEIGHT_STEP, MAX_HEIGHT_STEP);
        self.off_height += (off_target - self.off_height).clamp(-MAX_HEIGHT_STEP, MAX_HEIGHT_STEP);
        if same_item(&self.main_item, &main) || self.main_height < 0.1 {
            self.main_item = main;
        }
        if same_item(&self.off_item, &off) || self.off_height < 0.1 {
            self.off_item = off;
        }

        self.old_attack_anim = self.attack_anim;
        if self.swinging {
            self.swing_time += 1;
            if self.swing_time >= DEFAULT_SWING_TICKS as i32 {
                self.swing_time = 0;
                self.swinging = false;
            }
        } else {
            self.swing_time = 0;
        }
        self.attack_anim = self.swing_time as f32 / DEFAULT_SWING_TICKS;

        let kind = UseKind::of(name_of(&self.main_item));
        let usable = kind != UseKind::None && (t.server_using || t.local_using);
        match (usable, self.using) {
            (true, Some(_)) => self.use_elapsed += 1,
            (true, None) => {
                self.using = Some(kind);
                self.use_elapsed = 1;
            }
            (false, _) => self.using = None,
        }

        self.old_x_bob = self.x_bob;
        self.old_y_bob = self.y_bob;
        self.x_bob += (t.pitch - self.x_bob) * 0.5;
        self.y_bob += wrap_degrees(t.yaw - self.y_bob) * 0.5;

        let feet = state.player.pos();
        let dx = (feet[0] - self.last_feet[0]) as f32;
        let dz = (feet[2] - self.last_feet[2]) as f32;
        self.last_feet = feet;
        let moved = (dx * dx + dz * dz).sqrt();
        self.old_walk_dist = self.walk_dist;
        self.walk_dist += moved * 0.6;
        self.old_bob = self.bob;
        let target = if state.player.physics.on_ground && !state.player.dead {
            moved.min(0.1)
        } else {
            0.0
        };
        self.bob += (target - self.bob) * 0.4;
    }

    fn attack_value(&self, partial: f32) -> f32 {
        let mut d = self.attack_anim - self.old_attack_anim;
        if d < 0.0 {
            d += 1.0;
        }
        self.old_attack_anim + d * partial
    }

    /// Local player swing progress for the third-person model.
    pub fn swing_progress(&self, partial: f32) -> f32 {
        if self.swinging {
            self.attack_value(partial)
        } else {
            0.0
        }
    }

    pub fn hide(&mut self) {
        self.drawn = false;
    }

    pub fn scoping(&self) -> bool {
        self.using == Some(UseKind::Spyglass)
    }

    fn use_remaining(&self, duration: f32) -> f32 {
        (duration - self.use_elapsed as f32).max(0.0)
    }

    /// Builds the hand instances for this frame and uploads them.
    pub fn prepare(&mut self, f: &HandFrame<'_>) {
        self.drawn = false;
        self.scene
            .update(&SyncedRegistries(&f.state.registries), std::iter::empty());
        if f.state.player.dead || self.scoping() {
            return;
        }
        let (main_right, partial) = (
            f.options.main_hand == mcr_settings::value::HumanoidArm::Right,
            f.partial,
        );
        let main_side = if main_right { Side::Right } else { Side::Left };
        let off_side = if main_right { Side::Left } else { Side::Right };
        let (render_main, render_off) = self.hands_to_render();
        let attack = self.attack_value(partial);
        let mut base = Pose(Mat4::IDENTITY);
        self.apply_bob(&mut base, f);
        let x_rot = f.pitch;
        base.rot_x((x_rot - lerp(self.old_x_bob, self.x_bob, partial)) * 0.1);
        let yaw_lag = wrap_degrees(f.yaw - lerp(self.old_y_bob, self.y_bob, partial));
        base.rot_y(yaw_lag * 0.1);
        if render_main {
            let height = 1.0 - lerp(self.old_main_height, self.main_height, partial);
            let item = self.main_item.clone();
            let swing = if self.swinging { attack } else { 0.0 };
            self.arm_with_item(f, base, main_side, true, &item, swing, height, x_rot);
        }
        if render_off {
            let height = 1.0 - lerp(self.old_off_height, self.off_height, partial);
            let item = self.off_item.clone();
            self.arm_with_item(f, base, off_side, false, &item, 0.0, height, x_rot);
        }
        self.drawn = true;
    }

    fn hands_to_render(&self) -> (bool, bool) {
        let main_kind = UseKind::of(name_of(&self.main_item));
        if self.using.is_some() && matches!(main_kind, UseKind::Bow | UseKind::Crossbow) {
            return (true, false);
        }
        if main_kind == UseKind::Crossbow && charged(&self.main_item) {
            return (true, false);
        }
        if UseKind::of(name_of(&self.off_item)) == UseKind::Crossbow && charged(&self.off_item) {
            return (false, true);
        }
        (true, true)
    }

    fn apply_bob(&self, pose: &mut Pose, f: &HandFrame<'_>) {
        if !f.options.bob_view {
            return;
        }
        let p = f.partial;
        let dist = self.walk_dist - self.old_walk_dist;
        let walked = -(self.walk_dist + dist * p);
        let amount = lerp(self.old_bob, self.bob, p);
        pose.translate(
            (walked * PI).sin() * amount * 0.5,
            -((walked * PI).cos() * amount).abs(),
            0.0,
        );
        pose.rot_z((walked * PI).sin() * amount * 3.0);
        pose.rot_x((((walked * PI) - 0.2).cos() * amount).abs() * 5.0);
    }

    #[allow(clippy::too_many_arguments)]
    fn arm_with_item(
        &mut self,
        f: &HandFrame<'_>,
        base: Pose,
        side: Side,
        is_main: bool,
        item: &ItemStack,
        attack: f32,
        height: f32,
        x_rot: f32,
    ) {
        let mut pose = base;
        let inv = side.sign();
        let partial = f.partial;
        let kind = UseKind::of(name_of(item));
        let duration = kind.duration(name_of(item));
        let using_here = is_main && self.using.is_some();
        let remaining = self.use_remaining(duration);
        let is_map = name_of(item).ends_with("filled_map");
        if item.is_empty() {
            if is_main && !f.invisible {
                self.player_arm(f, pose, side, height, attack);
            }
            return;
        }
        if is_map {
            let id = mcr_entity_model::stack_map_id(item).unwrap_or(-1);
            if is_main && self.off_item.is_empty() {
                self.two_handed_map(f, pose, x_rot, height, attack, id);
            } else {
                self.one_handed_map(f, pose, side, height, attack, id);
            }
            return;
        }
        let mut use_state = None;
        if kind == UseKind::Crossbow {
            item_arm_transform(&mut pose, side, height);
            let is_charged = charged(item);
            if using_here && remaining > 0.0 && !is_charged {
                pose.translate(inv * -0.478_568_2, -0.094_387, 0.057_315_31);
                pose.rot_x(-11.935);
                pose.rot_y(inv * 65.3);
                pose.rot_z(inv * -9.785);
                let held = self.use_elapsed as f32 + partial;
                let charge = 25.0;
                let power = (held / charge).min(1.0);
                shake(&mut pose, held, power);
                pose.translate(0.0, 0.0, power * 0.04);
                pose.scale(1.0, 1.0, 1.0 + power * 0.2);
                pose.rot_y(-inv * 45.0);
                use_state = Some((held, charge - held));
            } else {
                swing_arm(&mut pose, attack, side);
                if is_charged && attack < 0.001 && is_main {
                    pose.translate(inv * -0.641_864, 0.0, 0.0);
                    pose.rot_y(inv * 10.0);
                }
            }
        } else if using_here && remaining > 0.0 && kind != UseKind::None {
            let held = self.use_elapsed as f32 + partial;
            use_state = Some((held, (duration - held).max(0.0)));
            if kind != UseKind::Spyglass {
                item_arm_transform(&mut pose, side, height);
            }
            match kind {
                UseKind::Eat | UseKind::Drink => {
                    eat_transform(&mut pose, side, remaining - partial, duration);
                    item_arm_transform(&mut pose, side, height);
                }
                UseKind::Block => {}
                UseKind::Bow => {
                    pose.translate(inv * -0.278_568_2, 0.183_443_87, 0.157_315_31);
                    pose.rot_x(-13.935);
                    pose.rot_y(inv * 35.3);
                    pose.rot_z(inv * -9.785);
                    let mut power = held / 20.0;
                    power = ((power * power + power * 2.0) / 3.0).min(1.0);
                    shake(&mut pose, held, power);
                    pose.translate(0.0, 0.0, power * 0.04);
                    pose.scale(1.0, 1.0, 1.0 + power * 0.2);
                    pose.rot_y(-inv * 45.0);
                }
                UseKind::Trident => {
                    pose.translate(inv * -0.5, 0.7, 0.1);
                    pose.rot_x(-55.0);
                    pose.rot_y(inv * 35.3);
                    pose.rot_z(inv * -9.785);
                    let power = (held / 10.0).min(1.0);
                    shake(&mut pose, held, power);
                    pose.translate(0.0, 0.0, power * 0.2);
                    pose.scale(1.0, 1.0, 1.0 + power * 0.2);
                    pose.rot_y(-inv * 45.0);
                }
                UseKind::Brush => brush_transform(&mut pose, side, remaining - partial),
                _ => {}
            }
        } else {
            item_arm_transform(&mut pose, side, height);
            if attack > 0.0 {
                swing_arm(&mut pose, attack, side);
            }
        }
        let light = f.light;
        self.scene
            .emit_item(item, side.context(), pose.0, light, use_state);
    }

    fn player_arm(&mut self, f: &HandFrame<'_>, base: Pose, side: Side, height: f32, attack: f32) {
        let mut pose = base;
        let inv = side.sign();
        let sqrt_attack = attack.sqrt();
        let x_swing = -0.3 * (sqrt_attack * PI).sin();
        let y_swing = 0.4 * (sqrt_attack * PI * 2.0).sin();
        let z_swing = -0.4 * (attack * PI).sin();
        pose.translate(
            inv * (x_swing + 0.640_000_05),
            y_swing + -0.6 + height * -0.6,
            z_swing + -0.719_999_97,
        );
        pose.rot_y(inv * 45.0);
        let z_rot = (attack * attack * PI).sin();
        let y_rot = (sqrt_attack * PI).sin();
        pose.rot_y(inv * y_rot * 70.0);
        pose.rot_z(inv * z_rot * -20.0);
        pose.translate(-inv, 3.6, 3.5);
        pose.rot_z(inv * 120.0);
        pose.rot_x(200.0);
        pose.rot_y(inv * -135.0);
        pose.translate(inv * 5.6, 0.0, 0.0);
        self.draw_arm(f, pose, side);
    }

    fn draw_arm(&mut self, f: &HandFrame<'_>, pose: Pose, side: Side) {
        let data = NoData;
        let mut view = EntityView::named("minecraft:player", &data);
        view.uuid = f.state.uuid;
        view.light = f.light;
        self.scene.emit_arm(
            &SyncedRegistries(&f.state.registries),
            &view,
            side == Side::Right,
            pose.0,
        );
    }

    fn map_hand(&mut self, f: &HandFrame<'_>, base: Pose, side: Side) {
        let mut pose = base;
        let inv = side.sign();
        pose.rot_y(92.0);
        pose.rot_x(45.0);
        pose.rot_z(inv * -41.0);
        pose.translate(inv * 0.3, -1.1, 0.45);
        self.draw_arm(f, pose, side);
    }

    fn one_handed_map(
        &mut self,
        f: &HandFrame<'_>,
        base: Pose,
        side: Side,
        height: f32,
        attack: f32,
        id: i32,
    ) {
        let mut pose = base;
        let inv = side.sign();
        pose.translate(inv * 0.125, -0.125, 0.0);
        if !f.invisible {
            let mut arm = pose;
            arm.rot_z(inv * 10.0);
            self.player_arm(f, arm, side, height, attack);
        }
        pose.translate(inv * 0.51, -0.08 + height * -1.2, -0.75);
        let sqrt_attack = attack.sqrt();
        let x_swing = (sqrt_attack * PI).sin();
        pose.translate(
            inv * -0.5 * x_swing,
            0.4 * (sqrt_attack * PI * 2.0).sin() - 0.3 * x_swing,
            -0.3 * (attack * PI).sin(),
        );
        pose.rot_x(x_swing * -45.0);
        pose.rot_y(inv * x_swing * -30.0);
        self.map_quad(f, pose, id);
    }

    fn two_handed_map(
        &mut self,
        f: &HandFrame<'_>,
        base: Pose,
        x_rot: f32,
        height: f32,
        attack: f32,
        id: i32,
    ) {
        let mut pose = base;
        let sqrt_attack = attack.sqrt();
        let y_swing = -0.2 * (attack * PI).sin();
        let z_swing = -0.4 * (sqrt_attack * PI).sin();
        pose.translate(0.0, -y_swing / 2.0, z_swing);
        let tilt = {
            let t = (1.0 - x_rot / 45.0 + 0.1).clamp(0.0, 1.0);
            -(t * PI).cos() * 0.5 + 0.5
        };
        pose.translate(0.0, 0.04 + height * -1.2 + tilt * -0.5, -0.72);
        pose.rot_x(tilt * -85.0);
        if !f.invisible {
            let mut hands = pose;
            hands.rot_y(90.0);
            self.map_hand(f, hands, Side::Right);
            self.map_hand(f, hands, Side::Left);
        }
        pose.rot_x((sqrt_attack * PI).sin() * 20.0);
        pose.scale(2.0, 2.0, 2.0);
        self.map_quad(f, pose, id);
    }

    fn map_quad(&mut self, f: &HandFrame<'_>, mut pose: Pose, id: i32) {
        pose.rot_y(180.0);
        pose.scale(0.38, 0.38, 0.38);
        self.scene.emit_map(id, pose.0, f.light, true, [0.0; 3]);
    }

    pub fn set_map(
        &mut self,
        id: i32,
        image: mcr_entity_model::ImageRgba,
        deco: Vec<mcr_entity_model::MapDecoration>,
    ) {
        self.scene.set_map(id, image, deco);
    }

    /// Uploads the instances; call after [`Self::prepare`].
    pub fn upload(&mut self, f: &HandFrame<'_>) {
        self.scene.flush_textures();
        self.scene.prepare(&FrameParams {
            view_proj: hand_view_proj(f.aspect),
            camera: [0.0; 3],
            light: LightInfo::with_brightness(f.sky_brightness),
        });
    }

    /// Draws over the finished frame with depth cleared, like vanilla's separate hand projection.
    pub fn record(&self, ctx: &mut OverlayCtx<'_>) {
        if !self.drawn {
            return;
        }
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hand"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.color,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: ctx.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        self.scene.draw(&mut pass);
    }
}

fn item_arm_transform(pose: &mut Pose, side: Side, height: f32) {
    pose.translate(side.sign() * 0.56, -0.52 + height * -0.6, -0.72);
}

fn swing_arm(pose: &mut Pose, attack: f32, side: Side) {
    let inv = side.sign();
    let s = attack.sqrt();
    pose.translate(
        inv * -0.4 * (s * PI).sin(),
        0.2 * (s * PI * 2.0).sin(),
        -0.2 * (attack * PI).sin(),
    );
    pose.rot_y(inv * (45.0 + (attack * attack * PI).sin() * -20.0));
    let xz = (s * PI).sin();
    pose.rot_z(inv * xz * -20.0);
    pose.rot_x(xz * -80.0);
    pose.rot_y(inv * -45.0);
}

fn eat_transform(pose: &mut Pose, side: Side, remaining: f32, duration: f32) {
    let used = remaining + 1.0;
    let scaled = used / duration;
    if scaled < 0.8 {
        pose.translate(0.0, ((used / 4.0 * PI).cos() * 0.1).abs(), 0.0);
    }
    let jiggle = 1.0 - scaled.max(0.0).powf(27.0);
    let inv = side.sign();
    pose.translate(jiggle * 0.6 * inv, jiggle * -0.5, 0.0);
    pose.rot_y(inv * jiggle * 90.0);
    pose.rot_x(jiggle * 10.0);
    pose.rot_z(inv * jiggle * 30.0);
}

fn shake(pose: &mut Pose, held: f32, power: f32) {
    if power > 0.1 {
        let s = ((held - 0.1) * 1.3).sin() * (power - 0.1);
        pose.translate(0.0, s * 0.004, 0.0);
    }
}

fn brush_transform(pose: &mut Pose, side: Side, remaining: f32) {
    let ticks = remaining.rem_euclid(10.0);
    let scaled = 1.0 - (ticks + 1.0) / 10.0;
    let angle = -15.0 + 75.0 * (scaled * 2.0 * PI).cos();
    if side == Side::Left {
        pose.translate(0.1, 0.83, 0.35);
        pose.rot_x(-80.0);
        pose.rot_y(-90.0);
        pose.rot_x(angle);
        pose.translate(-0.3, 0.22, 0.35);
    } else {
        pose.translate(-0.25, 0.22, 0.35);
        pose.rot_x(-80.0);
        pose.rot_y(90.0);
        pose.rot_x(angle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_kinds() {
        assert_eq!(UseKind::of("minecraft:bow"), UseKind::Bow);
        assert_eq!(UseKind::of("minecraft:cooked_beef"), UseKind::Eat);
        assert_eq!(UseKind::of("minecraft:potion"), UseKind::Drink);
        assert_eq!(UseKind::of("minecraft:stone"), UseKind::None);
        assert_eq!(UseKind::Eat.duration("minecraft:dried_kelp"), 16.0);
        assert_eq!(UseKind::Eat.duration("minecraft:bread"), 32.0);
    }

    #[test]
    fn eat_jiggle_is_off_at_start_and_full_at_end() {
        let mut start = Pose(Mat4::IDENTITY);
        eat_transform(&mut start, Side::Right, 31.0, 32.0);
        let mut end = Pose(Mat4::IDENTITY);
        eat_transform(&mut end, Side::Right, 1.0, 32.0);
        assert!(start.0.w_axis.x.abs() < 0.05);
        assert!(end.0.w_axis.x.abs() > 0.2);
    }

    #[test]
    fn same_item_ignores_count() {
        let a = ItemStack::new(5, 3);
        let b = ItemStack::new(5, 1);
        assert!(same_item(&a, &b));
        assert!(!same_item(&a, &ItemStack::default()));
        assert!(same_item(&ItemStack::default(), &ItemStack::default()));
    }
}
