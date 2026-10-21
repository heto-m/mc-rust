//! The first-person hand: arm, held items and their equip, swing and use animations.

use std::f32::consts::PI;

use mcr_client::ClientState;
use mcr_entity_model::glam::{Mat4, Vec3};
use mcr_entity_model::mcr_protocol::types::ItemStack;
use mcr_entity_model::mcr_protocol::types::component::{
    DataComponent, ItemUseAnimation, KineticCondition, SwingAnimationType, component_type_id,
};
use mcr_entity_model::mcr_protocol::types::entity_data::EntityDataValue;
use mcr_entity_model::mcr_protocol::types::game::Hand as GameHand;
use mcr_entity_model::wgpu;
use mcr_entity_model::{
    EntityScene, EntityView, FrameParams, ItemContext, LightInfo, NoData, SceneAssets,
    TargetFormats,
};
use mcr_render::OverlayCtx;
use mcr_settings::Options;

use crate::entities::{SyncedRegistries, hand_view_proj};

const MAX_HEIGHT_STEP: f32 = 0.4;
/// `SwingAnimation.DEFAULT`: a whack over 6 ticks.
pub const DEFAULT_SWING: (SwingAnimationType, i32) = (SwingAnimationType::Whack, 6);
const LIVING_FLAGS: u8 = 8;
const LONG_USE: i32 = 72000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UseKind {
    None,
    Eat,
    Drink,
    Block,
    Bow,
    Crossbow,
    Trident,
    Spyglass,
    Horn,
    Brush,
    Bundle,
    Spear,
}

impl UseKind {
    fn of_animation(a: ItemUseAnimation) -> Self {
        match a {
            ItemUseAnimation::None => Self::None,
            ItemUseAnimation::Eat => Self::Eat,
            ItemUseAnimation::Drink => Self::Drink,
            ItemUseAnimation::Block => Self::Block,
            ItemUseAnimation::Bow => Self::Bow,
            ItemUseAnimation::Trident => Self::Trident,
            ItemUseAnimation::Crossbow => Self::Crossbow,
            ItemUseAnimation::Spyglass => Self::Spyglass,
            ItemUseAnimation::TootHorn => Self::Horn,
            ItemUseAnimation::Brush => Self::Brush,
            ItemUseAnimation::Bundle => Self::Bundle,
            ItemUseAnimation::Spear => Self::Spear,
        }
    }

    /// `ItemUseAnimation#hasCustomArmTransform`.
    fn custom_arm_transform(self) -> bool {
        matches!(self, Self::Eat | Self::Drink | Self::Spear)
    }
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn name_of<'a>(state: &'a ClientState, stack: &ItemStack) -> &'a str {
    u32::try_from(stack.item)
        .ok()
        .and_then(|i| state.data.item_name(i))
        .map_or("", short)
}

fn ticks_of(seconds: f32) -> i32 {
    (seconds * 20.0).clamp(0.0, LONG_USE as f32) as i32
}

/// `Item#getUseAnimation` and `getUseDuration`: the item subclasses first, then the consumable,
/// `blocks_attacks` and `kinetic_weapon` components.
fn item_use(state: &ClientState, stack: &ItemStack) -> (UseKind, i32) {
    if stack.is_empty() {
        return (UseKind::None, 0);
    }
    let name = name_of(state, stack);
    match name {
        "bow" => return (UseKind::Bow, LONG_USE),
        "crossbow" => return (UseKind::Crossbow, state.crossbow_charge_ticks(stack) + 3),
        "trident" => return (UseKind::Trident, LONG_USE),
        "spyglass" => return (UseKind::Spyglass, 1200),
        "goat_horn" => return (UseKind::Horn, 140),
        "brush" => return (UseKind::Brush, 200),
        n if n.ends_with("bundle") => return (UseKind::Bundle, 200),
        _ => {}
    }
    if state.has_component(stack, "minecraft:consumable") {
        let patched = stack.components.added.iter().find_map(|c| match c {
            DataComponent::Consumable(c) => Some(c),
            _ => None,
        });
        return match patched {
            Some(c) => (
                UseKind::of_animation(c.animation),
                ticks_of(c.consume_seconds),
            ),
            None => {
                let seconds = u32::try_from(stack.item)
                    .ok()
                    .and_then(|i| state.data.item_consume_seconds(i))
                    .unwrap_or(1.6);
                let drink = matches!(
                    name,
                    "potion" | "milk_bucket" | "honey_bottle" | "ominous_bottle"
                );
                let kind = if drink { UseKind::Drink } else { UseKind::Eat };
                (kind, ticks_of(seconds))
            }
        };
    }
    if state.has_component(stack, "minecraft:blocks_attacks") {
        (UseKind::Block, LONG_USE)
    } else if state.has_component(stack, "minecraft:kinetic_weapon") {
        (UseKind::Spear, LONG_USE)
    } else {
        (UseKind::None, 0)
    }
}

fn charged(stack: &ItemStack) -> bool {
    component_type_id("charged_projectiles")
        .and_then(|id| stack.components.get(id))
        .is_some_and(|c| matches!(c, DataComponent::ChargedProjectiles(list) if !list.0.is_empty()))
}

/// `ItemStack#matchesIgnoringComponents` with `ignoreSwapAnimation`: only `damage` is ignored.
fn same_for_swap(a: &ItemStack, b: &ItemStack) -> bool {
    if a.is_empty() || b.is_empty() {
        return a.is_empty() && b.is_empty();
    }
    let kept = |c: &&DataComponent| !matches!(c, DataComponent::Damage(_));
    let damage = component_type_id("damage");
    let kept_id = |id: &&i32| Some(**id) != damage;
    let (pa, pb) = (&a.components, &b.components);
    a.count == b.count
        && a.item == b.item
        && pa.added.iter().filter(kept).count() == pb.added.iter().filter(kept).count()
        && pa.added.iter().filter(kept).all(|c| pb.added.contains(c))
        && pa.removed.iter().filter(kept_id).count() == pb.removed.iter().filter(kept_id).count()
        && pa
            .removed
            .iter()
            .filter(kept_id)
            .all(|id| pb.removed.contains(id))
}

/// `LivingEntity#getModifiedSwingDuration`: Haste or Conduit Power shorten it, Mining Fatigue
/// lengthens it.
pub fn swing_duration(base: i32, haste: Option<i32>, fatigue: Option<i32>) -> i32 {
    match (haste, fatigue) {
        (Some(a), _) => base.saturating_sub(1 + a.clamp(0, 255)),
        (None, Some(a)) => base.saturating_add((1 + a.clamp(0, 255)) * 2),
        (None, None) => base,
    }
}

fn modified_swing(state: &ClientState, base: i32) -> i32 {
    let haste = [
        state.effect_amplifier("haste"),
        state.effect_amplifier("conduit_power"),
    ]
    .into_iter()
    .flatten()
    .max();
    swing_duration(base, haste, state.effect_amplifier("mining_fatigue"))
}

/// `ItemStack#getAttackAnimation` or `getInteractAnimation`; spears stab for their attack time.
fn item_swing(state: &ClientState, stack: &ItemStack, attack: bool) -> (SwingAnimationType, i32) {
    let patched = stack.components.added.iter().find_map(|c| match c {
        DataComponent::AttackAnimation(a) if attack => Some(a),
        DataComponent::InteractAnimation(a) if !attack => Some(a),
        _ => None,
    });
    if let Some(a) = patched {
        return (a.kind, a.duration);
    }
    let name = if attack {
        "attack_animation"
    } else {
        "interact_animation"
    };
    let removed = component_type_id(name).is_some_and(|id| stack.components.removed.contains(&id));
    if attack
        && !removed
        && let Some(ticks) = spear_attack_ticks(name_of(state, stack))
    {
        return (SwingAnimationType::Stab, ticks);
    }
    DEFAULT_SWING
}

/// `(int)(attackDuration * 20)` of each spear's `attack_animation`.
fn spear_attack_ticks(name: &str) -> Option<i32> {
    Some(match name {
        "wooden_spear" => 13,
        "stone_spear" => 15,
        "copper_spear" => 17,
        "iron_spear" | "golden_spear" => 19,
        "diamond_spear" => 21,
        "netherite_spear" => 23,
        _ => return None,
    })
}

/// The `kinetic_weapon` timings the first-person spear use reads, in ticks.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Kinetic {
    delay: f32,
    dismount: f32,
    knockback: f32,
    damage: f32,
}

fn kinetic(state: &ClientState, stack: &ItemStack) -> Option<Kinetic> {
    let ticks = |t: i32| t.clamp(0, LONG_USE) as f32;
    let patched = stack.components.added.iter().find_map(|c| match c {
        DataComponent::KineticWeapon(k) => Some(k),
        _ => None,
    });
    if let Some(k) = patched {
        let cond =
            |c: &Option<KineticCondition>| c.as_ref().map_or(0.0, |c| ticks(c.max_duration_ticks));
        return Some(Kinetic {
            delay: ticks(k.delay_ticks),
            dismount: cond(&k.dismount_conditions),
            knockback: cond(&k.knockback_conditions),
            damage: cond(&k.damage_conditions),
        });
    }
    if !state.has_component(stack, "minecraft:kinetic_weapon") {
        return None;
    }
    let (delay, dismount, knockback, damage) = match name_of(state, stack) {
        "wooden_spear" => (15, 100, 200, 300),
        "stone_spear" => (14, 90, 180, 275),
        "copper_spear" => (13, 80, 165, 250),
        "iron_spear" => (12, 50, 135, 225),
        "golden_spear" => (14, 70, 170, 275),
        "diamond_spear" => (10, 60, 130, 200),
        "netherite_spear" => (8, 50, 110, 175),
        _ => return None,
    };
    Some(Kinetic {
        delay: delay as f32,
        dismount: dismount as f32,
        knockback: knockback as f32,
        damage: damage as f32,
    })
}

mod ease {
    use std::f32::consts::PI;

    pub fn in_out_sine(x: f32) -> f32 {
        -((PI * x).cos() - 1.0) / 2.0
    }

    pub fn out_back(x: f32) -> f32 {
        1.0 + 2.70158 * (x - 1.0).powi(3) + 1.70158 * (x - 1.0).powi(2)
    }

    pub fn out_quart(x: f32) -> f32 {
        1.0 - (1.0 - x).powi(4)
    }

    pub fn out_cubic(x: f32) -> f32 {
        1.0 - (1.0 - x).powi(3)
    }

    pub fn in_out_expo(x: f32) -> f32 {
        if x < 0.5 {
            if x == 0.0 {
                0.0
            } else {
                2f32.powf(20.0 * x - 10.0) / 2.0
            }
        } else if x == 1.0 {
            1.0
        } else {
            (2.0 - 2f32.powf(-20.0 * x + 10.0)) / 2.0
        }
    }

    pub fn out_circ(x: f32) -> f32 {
        (1.0 - (x - 1.0).powi(2)).max(0.0).sqrt()
    }

    pub fn in_circ(x: f32) -> f32 {
        1.0 - (1.0 - x * x).max(0.0).sqrt()
    }

    pub fn in_out_elastic(x: f32) -> f32 {
        if x == 0.0 || x == 1.0 {
            return x;
        }
        let sin = ((20.0 * x - 11.125) * PI * 4.0 / 9.0).sin();
        if x < 0.5 {
            -(2f32.powf(20.0 * x - 10.0) * sin) / 2.0
        } else {
            2f32.powf(-20.0 * x + 10.0) * sin / 2.0 + 1.0
        }
    }

    pub fn in_out_back(x: f32) -> f32 {
        if x < 0.5 {
            4.0 * x * x * (7.189_819 * x - 2.594_909_4) / 2.0
        } else {
            let dt = 2.0 * x - 2.0;
            (dt * dt * (3.594_909_4 * dt + 2.594_909_4) + 2.0) / 2.0
        }
    }
}

/// `SpearAnimations#progress`; a zero-length window is a step at its end.
fn progress(t: f32, start: f32, end: f32) -> f32 {
    if end == start {
        return if t >= end { 1.0 } else { 0.0 };
    }
    ((t - start) / (end - start)).clamp(0.0, 1.0)
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

    fn rotate_around(&mut self, rotation: Mat4, pivot: Vec3) {
        self.0 *= Mat4::from_translation(pivot) * rotation * Mat4::from_translation(-pivot);
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
    /// The hand projection's FOV in degrees (`Camera#hudFov`).
    pub fov: f32,
    /// Ticks since the last kinetic hit feedback, 0 before any.
    pub kinetic_feedback: f32,
    /// `GameRenderer#bobHurt` and `bobView`, which move the hand like the world.
    pub shake: Mat4,
    pub pitch: f32,
    pub yaw: f32,
    pub lightmap: &'a [[f32; 4]],
    pub light: [u8; 2],
    pub invisible: bool,
    pub skin: Option<&'a mcr_entity_model::Skin>,
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

/// `LivingEntity.SwingDescription`: the hand, animation and its modified length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Swing {
    hand: GameHand,
    kind: SwingAnimationType,
    base: i32,
    duration: i32,
}

/// `LivingEntity.SwingState`.
#[derive(Debug, Default)]
struct SwingState {
    current: Option<Swing>,
    ticks: i32,
    old: f32,
    animation: f32,
}

impl SwingState {
    fn start_if_able(&mut self, swing: Swing) -> bool {
        if let Some(c) = self.current
            && self.ticks <= c.duration / 2
            && self.ticks > 0
        {
            return false;
        }
        if self.current != Some(swing) {
            self.old = 0.0;
            self.animation = 0.0;
        }
        self.current = Some(swing);
        self.ticks = 0;
        true
    }

    fn tick(&mut self) {
        self.old = self.animation;
        let Some(c) = self.current else {
            self.animation = 0.0;
            return;
        };
        if c.duration > 0 {
            self.animation = (self.ticks as f32 / c.duration as f32).min(1.0);
        }
        let ticks = self.ticks;
        self.ticks = self.ticks.saturating_add(1);
        if ticks > c.duration {
            self.current = None;
            self.animation = 0.0;
        }
    }

    fn at(&self, partial: f32) -> f32 {
        if self.animation >= self.old {
            lerp(self.old, self.animation, partial)
        } else {
            lerp(self.old, self.animation + 1.0, partial)
        }
    }
}

/// What `evaluateWhichHandsToRender` needs to know about a held stack.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Held {
    bow: bool,
    crossbow: bool,
    charged: bool,
}

impl Held {
    fn of(state: &ClientState, stack: &ItemStack) -> Self {
        let name = name_of(state, stack);
        Self {
            bow: name == "bow",
            crossbow: name == "crossbow",
            charged: name == "crossbow" && charged(stack),
        }
    }
}

/// `FirstPersonHandsAndItems#evaluateWhichHandsToRender`: `(main, off)`.
fn hands_to_render(main: Held, off: Held, using: Option<GameHand>) -> (bool, bool) {
    if !(main.bow || off.bow || main.crossbow || off.crossbow) {
        return (true, true);
    }
    let Some(hand) = using else {
        return (true, !main.charged);
    };
    let used = if hand == GameHand::Main { main } else { off };
    if !used.bow && !used.crossbow {
        return (true, !(hand == GameHand::Main && off.charged));
    }
    (hand == GameHand::Main, hand == GameHand::Off)
}

fn held_stacks(state: &ClientState) -> (ItemStack, ItemStack) {
    let inv = &state.inventory;
    let main = inv.hotbar(state.player.held_slot).clone();
    let off = mcr_client::inventory_index_to_menu_slot(40)
        .and_then(|i| inv.player.slots.get(i))
        .cloned()
        .unwrap_or_default();
    (main, off)
}

/// The animation state of both hands, apart from the GPU scene.
#[derive(Debug)]
struct HandState {
    main_item: ItemStack,
    off_item: ItemStack,
    main_height: f32,
    old_main_height: f32,
    off_height: f32,
    old_off_height: f32,
    swing: SwingState,
    use_hand: Option<GameHand>,
    use_elapsed: u32,
    using_spyglass: bool,
    auto_spin: bool,
    x_bob: f32,
    old_x_bob: f32,
    y_bob: f32,
    old_y_bob: f32,
}

impl Default for HandState {
    fn default() -> Self {
        Self {
            main_item: ItemStack::default(),
            off_item: ItemStack::default(),
            main_height: 0.0,
            old_main_height: 0.0,
            off_height: 1.0,
            old_off_height: 1.0,
            swing: SwingState::default(),
            use_hand: None,
            use_elapsed: 0,
            using_spyglass: false,
            auto_spin: false,
            x_bob: 0.0,
            old_x_bob: 0.0,
            y_bob: 0.0,
            old_y_bob: 0.0,
        }
    }
}

impl HandState {
    /// `FirstPersonHandsAndItems#tick`; `plays_swap` is the item model's `hand_animation_on_swap`.
    fn tick(&mut self, t: &HandTick<'_>, plays_swap: &dyn Fn(&ItemStack) -> bool) {
        let state = t.state;
        let (main, off) = held_stacks(state);
        self.old_main_height = self.main_height;
        self.old_off_height = self.off_height;
        let main_now = same_for_swap(&self.main_item, &main) || !plays_swap(&main);
        let off_now = same_for_swap(&self.off_item, &off) || !plays_swap(&off);
        if main_now {
            self.main_item = main.clone();
        }
        if off_now {
            self.off_item = off.clone();
        }
        if t.hands_busy {
            self.main_height = (self.main_height - MAX_HEIGHT_STEP).clamp(0.0, 1.0);
            self.off_height = (self.off_height - MAX_HEIGHT_STEP).clamp(0.0, 1.0);
        } else {
            let s = t.swap_scale;
            let main_target = if main_now { s * s * s } else { 0.0 };
            let off_target = if off_now { 1.0 } else { 0.0 };
            self.main_height +=
                (main_target - self.main_height).clamp(-MAX_HEIGHT_STEP, MAX_HEIGHT_STEP);
            self.off_height +=
                (off_target - self.off_height).clamp(-MAX_HEIGHT_STEP, MAX_HEIGHT_STEP);
        }
        if self.main_height < 0.1 {
            self.main_item = main.clone();
        }
        if self.off_height < 0.1 {
            self.off_item = off.clone();
        }

        self.swing.tick();

        match t.using {
            Some(h) if self.use_hand == Some(h) => {
                self.use_elapsed = self.use_elapsed.saturating_add(1);
            }
            Some(h) => {
                self.use_hand = Some(h);
                self.use_elapsed = 1;
            }
            None => {
                self.use_hand = None;
                self.use_elapsed = 0;
            }
        }
        let used = match self.use_hand {
            Some(GameHand::Main) => Some(&main),
            Some(GameHand::Off) => Some(&off),
            None => None,
        };
        self.using_spyglass = used.is_some_and(|s| name_of(state, s) == "spyglass");
        self.auto_spin = matches!(
            state.player.data.get(&LIVING_FLAGS),
            Some(EntityDataValue::Byte(f)) if f & 4 != 0
        );

        self.old_x_bob = self.x_bob;
        self.old_y_bob = self.y_bob;
        self.x_bob += (t.pitch - self.x_bob) * 0.5;
        self.y_bob += wrap_degrees(t.yaw - self.y_bob) * 0.5;
    }

    /// `LivingEntity#swing`: starts `hand`'s animation unless the current one is in its first half.
    fn swing(
        &mut self,
        state: &ClientState,
        hand: GameHand,
        (kind, base): (SwingAnimationType, i32),
    ) {
        self.swing.start_if_able(Swing {
            hand,
            kind,
            base,
            duration: modified_swing(state, base),
        });
    }

    /// `FirstPersonHandsAndItems#itemUsed`: the hand drops and raises again.
    fn item_used(&mut self, hand: GameHand) {
        match hand {
            GameHand::Main => self.main_height = 0.0,
            GameHand::Off => self.off_height = 0.0,
        }
    }

    /// Ticks of use left for `hand`'s item lasting `duration` (`getUseItemRemainingTicks`).
    fn use_remaining(&self, hand: GameHand, duration: i32) -> Option<i32> {
        (self.use_hand == Some(hand))
            .then(|| duration.saturating_sub(self.use_elapsed.min(LONG_USE as u32) as i32))
            .filter(|r| *r > 0)
    }
}

pub struct Hand {
    scene: EntityScene,
    st: HandState,
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
    /// The hand an item is in use with: the local prediction, else the server's living flags.
    pub using: Option<GameHand>,
    /// The player's `getItemSwapScale(1)`, which raises the main hand after a swap or attack.
    pub swap_scale: f32,
    /// `LocalPlayer#isHandsBusy`: both hands lower while paddling.
    pub hands_busy: bool,
}

/// One arm's inputs to [`Hand::arm_with_item`].
struct Arm<'a> {
    hand: GameHand,
    side: Side,
    item: &'a ItemStack,
    attack: f32,
    height: f32,
    x_rot: f32,
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
            st: HandState::default(),
            drawn: false,
        })
    }

    /// The swing of `hand`'s item: its attack or interact animation.
    pub fn swing(&mut self, state: &ClientState, hand: GameHand, attack: bool) {
        let (main, off) = held_stacks(state);
        let stack = if hand == GameHand::Main { &main } else { &off };
        let animation = item_swing(state, stack, attack);
        self.st.swing(state, hand, animation);
    }

    /// A swing with an explicit animation: dropping an item, or the server's swing packet.
    pub fn swing_with(
        &mut self,
        state: &ClientState,
        hand: GameHand,
        animation: (SwingAnimationType, i32),
    ) {
        self.st.swing(state, hand, animation);
    }

    pub fn item_used(&mut self, hand: GameHand) {
        self.st.item_used(hand);
    }

    pub fn tick(&mut self, t: &HandTick<'_>) {
        let items = &self.scene.assets().items;
        let plays_swap = |stack: &ItemStack| {
            model_id(t.state, stack)
                .and_then(|id| items.get(&mcr_assets::Identifier::parse(&id)))
                .is_none_or(|d| d.hand_animation_on_swap)
        };
        self.st.tick(t, &plays_swap);
    }

    /// `ItemModelResolver#swapAnimationScale`.
    fn swap_scale(&self, state: &ClientState, stack: &ItemStack) -> f32 {
        model_id(state, stack)
            .and_then(|id| {
                self.scene
                    .assets()
                    .items
                    .get(&mcr_assets::Identifier::parse(&id))
                    .map(|d| d.swap_animation_scale)
            })
            .filter(|s| s.is_finite())
            .unwrap_or(1.0)
    }

    /// Local player swing progress for the third-person model.
    pub fn swing_progress(&self, partial: f32) -> f32 {
        self.st.swing.at(partial)
    }

    pub fn hide(&mut self) {
        self.drawn = false;
    }

    /// `Player#isScoping`: a spyglass in use in either hand.
    pub fn scoping(&self) -> bool {
        self.st.using_spyglass
    }

    /// Builds the hand instances for this frame and uploads them.
    pub fn prepare(&mut self, f: &HandFrame<'_>) {
        self.drawn = false;
        self.scene
            .update(&SyncedRegistries(&f.state.registries), std::iter::empty());
        if f.state.player.dead || self.scoping() {
            return;
        }
        let partial = f.partial;
        let main_right = f.options.main_hand == mcr_settings::value::HumanoidArm::Right;
        let main_side = if main_right { Side::Right } else { Side::Left };
        let off_side = if main_right { Side::Left } else { Side::Right };
        let (main, off) = held_stacks(f.state);
        let (render_main, render_off) = hands_to_render(
            Held::of(f.state, &main),
            Held::of(f.state, &off),
            self.st.use_hand,
        );
        let attack = self.st.swing.at(partial);
        let attack_hand = self.st.swing.current.map_or(GameHand::Main, |s| s.hand);
        let mut base = Pose(f.shake);
        let x_rot = f.pitch;
        base.rot_x((x_rot - lerp(self.st.old_x_bob, self.st.x_bob, partial)) * 0.1);
        let yaw_lag = wrap_degrees(f.yaw - lerp(self.st.old_y_bob, self.st.y_bob, partial));
        base.rot_y(yaw_lag * 0.1);
        for (hand, side, render) in [
            (GameHand::Main, main_side, render_main),
            (GameHand::Off, off_side, render_off),
        ] {
            if !render {
                continue;
            }
            let (item, height, old_height) = match hand {
                GameHand::Main => (
                    self.st.main_item.clone(),
                    self.st.main_height,
                    self.st.old_main_height,
                ),
                GameHand::Off => (
                    self.st.off_item.clone(),
                    self.st.off_height,
                    self.st.old_off_height,
                ),
            };
            let height =
                self.swap_scale(f.state, &item) * (1.0 - lerp(old_height, height, partial));
            let arm = Arm {
                hand,
                side,
                item: &item,
                attack: if attack_hand == hand { attack } else { 0.0 },
                height,
                x_rot,
            };
            self.arm_with_item(f, base, &arm);
        }
        self.drawn = true;
    }

    fn arm_with_item(&mut self, f: &HandFrame<'_>, base: Pose, a: &Arm<'_>) {
        let mut pose = base;
        let (side, item, attack, height) = (a.side, a.item, a.attack, a.height);
        let inv = side.sign();
        let partial = f.partial;
        let is_main = a.hand == GameHand::Main;
        if item.is_empty() {
            if is_main && !f.invisible {
                self.player_arm(f, pose, side, height, attack);
            }
            return;
        }
        if name_of(f.state, item) == "filled_map" || mcr_entity_model::stack_map_id(item).is_some()
        {
            let id = mcr_entity_model::stack_map_id(item).unwrap_or(-1);
            if is_main && self.st.off_item.is_empty() {
                self.two_handed_map(f, pose, a.x_rot, height, attack, id);
            } else {
                self.one_handed_map(f, pose, side, height, attack, id);
            }
            return;
        }
        let (kind, duration) = item_use(f.state, item);
        let remaining = self.st.use_remaining(a.hand, duration);
        let held = |r: i32| duration as f32 - (r as f32 - partial + 1.0);
        let mut use_state = remaining.map(|r| {
            let held = held(r).max(0.0);
            (held, duration as f32 - held)
        });
        if kind == UseKind::Crossbow {
            item_arm_transform(&mut pose, side, height);
            let is_charged = charged(item);
            match remaining {
                Some(r) if !is_charged => {
                    pose.translate(inv * -0.478_568_2, -0.094_387, 0.057_315_31);
                    pose.rot_x(-11.935);
                    pose.rot_y(inv * 65.3);
                    pose.rot_z(inv * -9.785);
                    let held = held(r);
                    let charge = (duration - 3).max(1) as f32;
                    let power = (held / charge).min(1.0);
                    shake(&mut pose, held, power);
                    pose.translate(0.0, 0.0, power * 0.04);
                    pose.scale(1.0, 1.0, 1.0 + power * 0.2);
                    pose.rot_y(-inv * 45.0);
                }
                _ => {
                    use_state = None;
                    swing_arm(&mut pose, attack, side);
                    if is_charged && attack < 0.001 && is_main {
                        pose.translate(inv * -0.641_864, 0.0, 0.0);
                        pose.rot_y(inv * 10.0);
                    }
                }
            }
        } else if let Some(r) = remaining {
            if !kind.custom_arm_transform() {
                item_arm_transform(&mut pose, side, height);
            }
            let held = held(r);
            match kind {
                UseKind::Eat | UseKind::Drink => {
                    eat_transform(&mut pose, side, r as f32 - partial, duration as f32);
                    item_arm_transform(&mut pose, side, height);
                }
                UseKind::Block if name_of(f.state, item) != "shield" => {
                    pose.translate(inv * -0.141_421_36, 0.08, 0.141_421_36);
                    pose.rot_x(-102.25);
                    pose.rot_y(inv * 13.365);
                    pose.rot_z(inv * 78.05);
                }
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
                UseKind::Brush => brush_transform(&mut pose, side, r as f32 - partial),
                UseKind::Bundle => swing_arm(&mut pose, attack, side),
                UseKind::Spear => {
                    pose.translate(inv * 0.56, -0.52, -0.72);
                    if let Some(k) = kinetic(f.state, item) {
                        spear_use(&mut pose, held, side, &k, f.kinetic_feedback);
                    }
                }
                _ => {}
            }
        } else if self.st.auto_spin {
            item_arm_transform(&mut pose, side, height);
            pose.translate(inv * -0.4, 0.8, 0.3);
            pose.rot_y(inv * 65.0);
            pose.rot_z(inv * -85.0);
        } else {
            item_arm_transform(&mut pose, side, height);
            match self
                .st
                .swing
                .current
                .filter(|s| s.hand == a.hand)
                .map(|s| s.kind)
            {
                Some(SwingAnimationType::Whack) => swing_arm(&mut pose, attack, side),
                Some(SwingAnimationType::Stab) => spear_attack(&mut pose, attack, side),
                _ => {}
            }
        }
        self.scene
            .emit_item(item, side.context(), pose.0, f.light, use_state);
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
        view.player = mcr_entity_model::PlayerView {
            skin: f.skin,
            ..Default::default()
        };
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

    /// Compass and clock inputs for held items, owned by the local player.
    pub fn set_item_world(
        &mut self,
        world: mcr_entity_model::ItemWorld,
        owner: mcr_entity_model::ItemOwner,
    ) {
        self.scene.set_item_world(world);
        self.scene.set_item_owner(Some(owner));
    }

    pub fn set_map(
        &mut self,
        id: i32,
        image: mcr_entity_model::ImageRgba,
        deco: Vec<mcr_entity_model::MapDecoration>,
        labels: Option<mcr_entity_model::MapLabels>,
    ) {
        self.scene.set_map(id, image, deco, labels);
    }

    pub fn has_map(&self, id: i32) -> bool {
        self.scene.has_map(id)
    }

    /// Uploads the instances; call after [`Self::prepare`].
    pub fn upload(&mut self, f: &HandFrame<'_>) {
        self.scene.flush_textures();
        self.scene.prepare(&FrameParams {
            view_proj: hand_view_proj(f.aspect, f.fov),
            camera: [0.0; 3],
            light: LightInfo::with_lightmap(f.lightmap),
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

/// The stack's item model id: its `item_model` component, else the item's own id.
fn model_id(state: &ClientState, stack: &ItemStack) -> Option<String> {
    if stack.is_empty() {
        return None;
    }
    let patched = stack.components.added.iter().find_map(|c| match c {
        DataComponent::ItemModel(id) => Some(id.as_str().to_owned()),
        _ => None,
    });
    patched.or_else(|| {
        u32::try_from(stack.item)
            .ok()
            .and_then(|i| state.data.item_name(i))
            .map(str::to_owned)
    })
}

/// `SpearAnimations.UseParams#fromKineticWeapon`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SpearUse {
    raise: f32,
    raise_start: f32,
    raise_middle: f32,
    raise_end: f32,
    sway: f32,
    lower: f32,
    raise_back: f32,
    sway_slow: f32,
    sway_fast: f32,
}

impl SpearUse {
    fn new(k: &Kinetic, time: f32) -> Self {
        let finish_raise = k.delay;
        let finish_sway = k.dismount + finish_raise;
        let finish_lower = k.knockback + finish_raise;
        let finish_back = k.damage + finish_raise;
        let raise = progress(time, 0.0, finish_raise);
        let sway = progress(time, finish_sway - 20.0, finish_sway);
        let raise_back = progress(time, finish_back - 5.0, finish_back);
        let intensity = 2.0 * ease::out_circ(sway) - 2.0 * ease::in_circ(raise_back);
        Self {
            raise,
            raise_start: progress(raise, 0.0, 0.5),
            raise_middle: progress(raise, 0.5, 0.8),
            raise_end: progress(raise, 0.8, 1.0),
            sway,
            lower: ease::out_cubic(ease::in_out_elastic(progress(
                time - 20.0,
                finish_lower - 40.0,
                finish_lower,
            ))),
            raise_back,
            sway_slow: (time * 19f32.to_radians()).sin() * intensity,
            sway_fast: (time * 30f32.to_radians()).sin() * intensity,
        }
    }
}

/// `SpearAnimations#firstPersonUse`.
fn spear_use(pose: &mut Pose, time: f32, side: Side, k: &Kinetic, feedback: f32) {
    let p = SpearUse::new(k, time);
    let inv = side.sign();
    pose.translate(
        inv * (p.raise * 0.15 + p.raise_end * -0.05 + p.sway * -0.1 + p.sway_slow * 0.005),
        p.raise * -0.075 + p.raise_middle * 0.075 + p.sway_fast * 0.01,
        p.raise_start * 0.05 + p.raise_end * -0.05 + p.sway_slow * 0.005,
    );
    let x = -65.0 * ease::in_out_back(p.raise) - 35.0 * p.lower
        + 100.0 * p.raise_back
        + -0.5 * p.sway_fast;
    pose.rotate_around(
        Mat4::from_rotation_x(x.to_radians()),
        Vec3::new(0.0, 0.1, 0.0),
    );
    let y = inv * (-90.0 * progress(p.raise, 0.5, 0.55) + 90.0 * p.sway + 2.0 * p.sway_slow);
    pose.rotate_around(
        Mat4::from_rotation_y(-y.to_radians()),
        Vec3::new(inv * 0.15, 0.0, 0.0),
    );
    pose.translate(0.0, -hit_feedback(feedback), 0.0);
}

/// `SpearAnimations#hitFeedbackAmount`: a quick dip after a kinetic hit that eases back by tick 10.
fn hit_feedback(ticks: f32) -> f32 {
    0.4 * (ease::out_quart(progress(ticks, 1.0, 3.0))
        - ease::in_out_sine(progress(ticks, 3.0, 10.0)))
}

/// `SpearAnimations#firstPersonAttack`: the stab swing.
fn spear_attack(pose: &mut Pose, attack: f32, side: Side) {
    let inv = side.sign();
    let start = ease::in_out_sine(progress(attack, 0.0, 0.05));
    let middle = ease::out_back(progress(attack, 0.05, 0.2));
    let end = ease::in_out_expo(progress(attack, 0.4, 1.0));
    pose.translate(
        inv * 0.1 * (start - middle),
        -0.075 * (start - end),
        0.65 * (start - middle),
    );
    pose.rot_x(-70.0 * (start - end));
    pose.translate(0.0, 0.0, -0.25 * (end - middle));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> ClientState {
        ClientState::new(&mcr_client::ClientConfig::default(), uuid::Uuid::nil())
    }

    fn item(state: &ClientState, name: &str) -> ItemStack {
        let id = state.data.item_by_name(name).expect(name);
        ItemStack::new(id as i32, 1)
    }

    fn tick(st: &mut HandState, state: &ClientState, using: Option<GameHand>) {
        st.tick(
            &HandTick {
                state,
                yaw: 0.0,
                pitch: 0.0,
                using,
                swap_scale: 1.0,
                hands_busy: false,
            },
            &|_| true,
        );
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
    fn hit_feedback_dips_then_recovers() {
        assert_eq!(hit_feedback(0.0), 0.0);
        assert_eq!(hit_feedback(1.0), 0.0);
        assert!((hit_feedback(3.0) - 0.4).abs() < 1e-5);
        assert!(hit_feedback(6.5) > 0.0 && hit_feedback(6.5) < 0.4);
        assert!(hit_feedback(10.0).abs() < 1e-5);
        assert!(hit_feedback(40.0).abs() < 1e-5);
    }

    #[test]
    fn swap_match_ignores_damage_but_not_count() {
        let a = ItemStack::new(5, 3);
        let mut b = a.clone();
        b.components.added.push(DataComponent::Damage(4.into()));
        assert!(same_for_swap(&a, &b));
        assert!(!same_for_swap(&a, &ItemStack::new(5, 2)));
        assert!(!same_for_swap(&a, &ItemStack::default()));
        assert!(same_for_swap(&ItemStack::default(), &ItemStack::default()));
    }

    #[test]
    fn use_kinds_and_durations() {
        let s = state();
        assert_eq!(
            item_use(&s, &item(&s, "minecraft:bow")),
            (UseKind::Bow, LONG_USE)
        );
        assert_eq!(
            item_use(&s, &item(&s, "minecraft:bread")),
            (UseKind::Eat, 32)
        );
        assert_eq!(
            item_use(&s, &item(&s, "minecraft:dried_kelp")),
            (UseKind::Eat, 16)
        );
        assert_eq!(
            item_use(&s, &item(&s, "minecraft:potion")),
            (UseKind::Drink, 32)
        );
        assert_eq!(
            item_use(&s, &item(&s, "minecraft:honey_bottle")),
            (UseKind::Drink, 40)
        );
        assert_eq!(
            item_use(&s, &item(&s, "minecraft:crossbow")),
            (UseKind::Crossbow, 28)
        );
        assert_eq!(
            item_use(&s, &item(&s, "minecraft:shield")),
            (UseKind::Block, LONG_USE)
        );
        assert_eq!(
            item_use(&s, &item(&s, "minecraft:iron_spear")),
            (UseKind::Spear, LONG_USE)
        );
        assert_eq!(
            item_use(&s, &item(&s, "minecraft:stone")),
            (UseKind::None, 0)
        );
    }

    #[test]
    fn off_hand_use_animates_the_off_hand_only() {
        let s = state();
        let mut st = HandState::default();
        tick(&mut st, &s, Some(GameHand::Off));
        tick(&mut st, &s, Some(GameHand::Off));
        assert_eq!(st.use_remaining(GameHand::Off, 32), Some(30));
        assert_eq!(st.use_remaining(GameHand::Main, 32), None);
        assert_eq!(st.use_remaining(GameHand::Off, 2), None);
        tick(&mut st, &s, Some(GameHand::Main));
        assert_eq!(st.use_remaining(GameHand::Main, 32), Some(31));
        tick(&mut st, &s, None);
        assert_eq!(st.use_remaining(GameHand::Main, 32), None);
    }

    #[test]
    fn crossbow_hand_selection() {
        let none = Held::default();
        let bow = Held { bow: true, ..none };
        let loaded = Held {
            crossbow: true,
            charged: true,
            ..none
        };
        let empty = Held {
            crossbow: true,
            ..none
        };
        assert_eq!(hands_to_render(none, none, None), (true, true));
        assert_eq!(hands_to_render(loaded, none, None), (true, false));
        assert_eq!(hands_to_render(none, loaded, None), (true, true));
        assert_eq!(
            hands_to_render(none, empty, Some(GameHand::Off)),
            (false, true)
        );
        assert_eq!(
            hands_to_render(bow, none, Some(GameHand::Main)),
            (true, false)
        );
        assert_eq!(
            hands_to_render(none, loaded, Some(GameHand::Main)),
            (true, false)
        );
        assert_eq!(
            hands_to_render(bow, loaded, Some(GameHand::Off)),
            (false, true)
        );
    }

    #[test]
    fn item_used_dips_the_hand() {
        let s = state();
        let mut st = HandState::default();
        for _ in 0..4 {
            tick(&mut st, &s, None);
        }
        assert_eq!((st.main_height, st.off_height), (1.0, 1.0));
        st.item_used(GameHand::Off);
        assert_eq!((st.main_height, st.off_height), (1.0, 0.0));
        tick(&mut st, &s, None);
        assert!((st.off_height - 0.4).abs() < 1e-6);
    }

    #[test]
    fn hands_busy_lowers_both_hands() {
        let s = state();
        let mut st = HandState::default();
        tick(&mut st, &s, None);
        tick(&mut st, &s, None);
        tick(&mut st, &s, None);
        let busy = HandTick {
            state: &s,
            yaw: 0.0,
            pitch: 0.0,
            using: None,
            swap_scale: 1.0,
            hands_busy: true,
        };
        st.tick(&busy, &|_| true);
        st.tick(&busy, &|_| true);
        assert!(st.main_height < 0.3 && st.off_height < 0.3);
    }

    #[test]
    fn swing_duration_with_haste_and_fatigue() {
        assert_eq!(swing_duration(6, None, None), 6);
        assert_eq!(swing_duration(6, Some(1), None), 4);
        assert_eq!(swing_duration(6, Some(1), Some(2)), 4);
        assert_eq!(swing_duration(6, None, Some(0)), 8);
        assert_eq!(swing_duration(19, None, Some(2)), 25);
        assert!(swing_duration(6, Some(i32::MAX), None) < 0);
    }

    #[test]
    fn swing_runs_its_duration_and_refuses_early_restarts() {
        let mut sw = SwingState::default();
        let whack = Swing {
            hand: GameHand::Main,
            kind: SwingAnimationType::Whack,
            base: 6,
            duration: 4,
        };
        assert!(sw.start_if_able(whack));
        sw.tick();
        sw.tick();
        assert!((sw.at(1.0) - 0.25).abs() < 1e-6);
        assert!(!sw.start_if_able(whack));
        sw.tick();
        assert!(sw.start_if_able(Swing {
            hand: GameHand::Off,
            ..whack
        }));
        let mut ticks = 0;
        while sw.current.is_some() {
            sw.tick();
            ticks += 1;
            assert!(ticks < 10);
        }
        assert_eq!(ticks, 6);
    }

    #[test]
    fn spears_stab_for_their_attack_time() {
        let s = state();
        let spear = item(&s, "minecraft:diamond_spear");
        assert_eq!(item_swing(&s, &spear, true), (SwingAnimationType::Stab, 21));
        assert_eq!(item_swing(&s, &spear, false), DEFAULT_SWING);
        assert_eq!(
            item_swing(&s, &item(&s, "minecraft:stone"), true),
            DEFAULT_SWING
        );
    }

    #[test]
    fn spear_use_raises_then_settles() {
        let k = Kinetic {
            delay: 10.0,
            dismount: 60.0,
            knockback: 130.0,
            damage: 200.0,
        };
        let start = SpearUse::new(&k, 0.0);
        assert_eq!((start.raise, start.sway, start.raise_back), (0.0, 0.0, 0.0));
        let raised = SpearUse::new(&k, 10.0);
        assert_eq!(raised.raise, 1.0);
        let done = SpearUse::new(&k, 300.0);
        assert_eq!((done.sway, done.raise_back), (1.0, 1.0));
        let zero = Kinetic {
            delay: 0.0,
            dismount: 0.0,
            knockback: 0.0,
            damage: 0.0,
        };
        let p = SpearUse::new(&zero, 0.0);
        assert!(p.raise.is_finite() && p.lower.is_finite() && p.sway_fast.is_finite());
    }
}
