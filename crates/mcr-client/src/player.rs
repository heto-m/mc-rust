use mcr_physics::{Abilities, Attributes, Effects, PlayerPhysics};
use mcr_protocol::play::clientbound::entity::{PositionMoveRotation, Relatives};
use mcr_protocol::play::clientbound::player::{RelativeFlags, Respawn};
use mcr_protocol::types::Vec3d;
use mcr_protocol::types::game::GameType;
use rustc_hash::FxHashMap;

use crate::entity::{AttributeState, MobEffect};

const XP_DISPLAY_TICKS: u32 = 100;
pub(crate) const SLEEPING_POS_INDEX: u8 = 14;

pub const SPRINT_MODIFIER_ID: &str = "minecraft:sprinting";

#[derive(Debug, Clone)]
pub struct LocalPlayer {
    pub entity_id: i32,
    pub physics: PlayerPhysics,
    pub game_mode: GameType,
    pub previous_game_mode: Option<GameType>,
    pub health: f32,
    pub food: i32,
    pub saturation: f32,
    pub xp_progress: f32,
    pub xp_level: i32,
    pub xp_total: i32,
    /// Ticks left that the experience bar outranks the locator bar (vanilla's 100-tick display window).
    pub xp_show_ticks: u32,
    xp_display_armed: bool,
    pub invulnerable: bool,
    pub instabuild: bool,
    pub walking_speed: f32,
    pub held_slot: u8,
    /// Operator level 0-4 from entity events 24-28.
    pub permission_level: u8,
    pub dead: bool,
    pub show_death_screen: bool,
    /// Vanilla `sleepCounter`: up to 100 while asleep, then 101..110 fading out after waking.
    pub sleep_timer: u32,
    pub vehicle: Option<i32>,
    /// `Player#getLastDeathLocation`, from the last login or respawn.
    pub last_death: Option<mcr_protocol::types::component::GlobalPos>,
    /// A teleport has been received since login/respawn.
    pub placed: bool,
    /// Vanilla `clientLoaded`: movement runs and is sent only after this.
    pub loaded: bool,
    pub loaded_waited_ticks: u32,
    pub teleports_received: u32,
    /// The server rotated the view this tick; the next physics step must not overwrite it with stale input.
    pub view_snapped: bool,
    pub hurt_time: i32,
    pub hurt_duration: i32,
    pub hurt_dir: f32,
    pub damage_cooldown: i32,
    /// Vanilla `flashOnSetHealth`: the first health packet after spawn never flashes.
    pub flash_on_set_health: bool,
    pub last_teleport_id: Option<i32>,
    pub effects: FxHashMap<i32, MobEffect>,
    pub attributes: FxHashMap<i32, AttributeState>,
    /// Synched entity data of the local player (air supply, absorption).
    pub data: FxHashMap<u8, mcr_protocol::types::entity_data::EntityDataValue>,
}

impl Default for LocalPlayer {
    fn default() -> Self {
        Self {
            entity_id: 0,
            physics: PlayerPhysics::new([0.0; 3]),
            game_mode: GameType::Survival,
            previous_game_mode: None,
            health: 20.0,
            food: 20,
            saturation: 5.0,
            xp_progress: 0.0,
            xp_level: 0,
            xp_total: 0,
            xp_show_ticks: 0,
            xp_display_armed: false,
            invulnerable: false,
            instabuild: false,
            walking_speed: 0.1,
            held_slot: 0,
            permission_level: 0,
            dead: false,
            show_death_screen: true,
            sleep_timer: 0,
            vehicle: None,
            last_death: None,
            placed: false,
            loaded: false,
            loaded_waited_ticks: 0,
            teleports_received: 0,
            view_snapped: false,
            hurt_time: 0,
            hurt_duration: 0,
            hurt_dir: 0.0,
            damage_cooldown: 0,
            flash_on_set_health: false,
            last_teleport_id: None,
            effects: FxHashMap::default(),
            attributes: FxHashMap::default(),
            data: FxHashMap::default(),
        }
    }
}

impl LocalPlayer {
    pub fn can_use_game_master_blocks(&self) -> bool {
        self.instabuild && self.permission_level >= 2
    }

    pub fn pos(&self) -> [f64; 3] {
        self.physics.pos
    }

    /// The bed the server has the player sleeping in: vanilla `getSleepingPos`.
    pub fn sleeping_pos(&self) -> Option<mcr_buf::BlockPos> {
        match self.data.get(&SLEEPING_POS_INDEX) {
            Some(mcr_protocol::types::entity_data::EntityDataValue::OptionalBlockPos(p)) => *p,
            _ => None,
        }
    }

    pub fn is_sleeping(&self) -> bool {
        self.sleeping_pos().is_some()
    }

    /// `Player#stopSleepInBed` on the wake-up animation: the fade runs its last 10 ticks.
    pub fn wake_up(&mut self) {
        self.data.remove(&SLEEPING_POS_INDEX);
        self.physics.sleeping = false;
        self.sleep_timer = 100;
    }

    /// `Player#tick`'s sleep counter, which drives the sleep fade.
    pub fn tick_sleep(&mut self) {
        if self.is_sleeping() {
            self.sleep_timer = (self.sleep_timer + 1).min(100);
        } else if self.sleep_timer > 0 {
            self.sleep_timer += 1;
            if self.sleep_timer >= 110 {
                self.sleep_timer = 0;
            }
        }
    }

    /// Fresh per-life state: vitals reset, position kept until the next teleport. `keep` holds the
    /// respawn packet's data-kept flags; `None` is a login, which starts from nothing.
    pub fn reset_for_spawn(
        &mut self,
        game_mode: GameType,
        previous: Option<GameType>,
        keep: Option<u8>,
    ) {
        let old = std::mem::take(self);
        *self = Self {
            entity_id: old.entity_id,
            held_slot: old.held_slot,
            permission_level: old.permission_level,
            game_mode,
            previous_game_mode: previous,
            ..Self::default()
        };
        if let Some(keep) = keep {
            self.show_death_screen = old.show_death_screen;
            self.attributes = old.attributes;
            if keep & Respawn::KEEP_ATTRIBUTE_MODIFIERS == 0 {
                for a in self.attributes.values_mut() {
                    a.modifiers.clear();
                }
            }
            if keep & Respawn::KEEP_ENTITY_DATA != 0 {
                self.data = old.data;
                self.physics.vel = old.physics.vel;
                self.physics.yaw = old.physics.yaw;
                self.physics.pitch = old.physics.pitch;
            } else {
                self.physics.yaw = -180.0;
            }
        }
        self.apply_game_mode(game_mode);
    }

    pub fn start_hurt(&mut self) {
        self.hurt_duration = 10;
        self.hurt_time = self.hurt_duration;
    }

    pub fn hurt_to(&mut self, new_health: f32) {
        if !self.flash_on_set_health {
            self.flash_on_set_health = true;
        } else if self.health - new_health > 0.0 {
            self.damage_cooldown = 20;
            self.start_hurt();
        } else if self.health - new_health < 0.0 {
            self.damage_cooldown = 10;
        }
        self.health = new_health;
    }

    /// `setExperienceValues`: a progress change opens the display window, except the first one.
    pub fn set_experience(&mut self, progress: f32, total: i32, level: i32) {
        if progress != self.xp_progress {
            if self.xp_display_armed {
                self.xp_show_ticks = XP_DISPLAY_TICKS;
            }
            self.xp_display_armed = true;
        }
        self.xp_progress = progress;
        self.xp_total = total;
        self.xp_level = level;
    }

    pub fn tick_hurt(&mut self) {
        self.xp_show_ticks = self.xp_show_ticks.saturating_sub(1);
        self.hurt_time = (self.hurt_time - 1).max(0);
        self.damage_cooldown = (self.damage_cooldown - 1).max(0);
    }

    /// `MobEffectInstance#tickClient`: durations count down between server updates; -1 is endless.
    pub fn tick_effects(&mut self) {
        for e in self.effects.values_mut() {
            if e.duration_ticks > 0 {
                e.duration_ticks -= 1;
            }
        }
    }

    pub fn apply_game_mode(&mut self, mode: GameType) {
        self.game_mode = mode;
        self.physics.spectator = mode == GameType::Spectator;
        let a = &mut self.physics.abilities;
        match mode {
            GameType::Creative => {
                a.may_fly = true;
                self.instabuild = true;
                self.invulnerable = true;
            }
            GameType::Spectator => {
                a.may_fly = true;
                a.flying = true;
                self.instabuild = false;
                self.invulnerable = true;
            }
            GameType::Survival | GameType::Adventure => {
                a.may_fly = false;
                a.flying = false;
                self.instabuild = false;
                self.invulnerable = false;
            }
        }
    }

    pub fn set_abilities(&mut self, abilities: Abilities, instabuild: bool, invulnerable: bool) {
        self.physics.abilities = abilities;
        self.instabuild = instabuild;
        self.invulnerable = invulnerable;
    }

    pub fn set_food(&mut self, food: i32) {
        self.food = food;
        self.physics.food_level = food;
    }

    pub fn apply_attributes(&mut self, attributes: Attributes) {
        self.physics.attributes = attributes;
    }

    pub fn apply_effects(&mut self, effects: Effects) {
        self.physics.effects = effects;
    }

    /// Resolves relative flags against the current state exactly like vanilla's
    /// `PositionMoveRotation.calculateAbsolute`, then snaps physics to the result.
    pub fn apply_teleport(
        &mut self,
        position: Vec3d,
        delta: Vec3d,
        y_rot: f32,
        x_rot: f32,
        relatives: RelativeFlags,
    ) {
        let change = PositionMoveRotation {
            position,
            delta_movement: delta,
            y_rot,
            x_rot,
        };
        let cur = (
            self.physics.pos,
            self.physics.vel,
            self.physics.yaw,
            self.physics.pitch,
        );
        let (pos, vel, yaw, pitch) = absolute_move(cur, &change, u32::from(relatives.bits()));
        if !pos.iter().chain(vel.iter()).all(|v| v.is_finite())
            || !yaw.is_finite()
            || !pitch.is_finite()
        {
            return;
        }
        let clamp = |v: f64| v.clamp(-3.0e7, 3.0e7);
        let pos = [clamp(pos[0]), pos[1].clamp(-2.0e7, 2.0e7), clamp(pos[2])];
        self.physics
            .snap_to(pos, vel, yaw % 360.0, pitch.clamp(-90.0, 90.0));
        self.view_snapped = true;
        self.placed = true;
    }
}

/// Vanilla `PositionMoveRotation#calculateAbsolute`: `bits` are `Relatives` flags.
pub(crate) fn absolute_move(
    (pos, vel, yaw, pitch): ([f64; 3], [f64; 3], f32, f32),
    change: &PositionMoveRotation,
    bits: u32,
) -> ([f64; 3], [f64; 3], f32, f32) {
    let rel = |flag: u32| Relatives(bits).contains(flag);
    let offset = |flag: u32, cur: f64| if rel(flag) { cur } else { 0.0 };
    let c = &change.position;
    let new_pos = [
        offset(Relatives::X, pos[0]) + c.x,
        offset(Relatives::Y, pos[1]) + c.y,
        offset(Relatives::Z, pos[2]) + c.z,
    ];
    let new_yaw = if rel(Relatives::Y_ROT) { yaw } else { 0.0 } + change.y_rot;
    let new_pitch =
        (if rel(Relatives::X_ROT) { pitch } else { 0.0 } + change.x_rot).clamp(-90.0, 90.0);
    let mut rotated = vel;
    if rel(Relatives::ROTATE_DELTA) {
        rotated = rotate_x(rotated, (pitch - new_pitch).to_radians());
        rotated = rotate_y(rotated, (yaw - new_yaw).to_radians());
    }
    let d = &change.delta_movement;
    let new_vel = [
        offset(Relatives::DELTA_X, rotated[0]) + d.x,
        offset(Relatives::DELTA_Y, rotated[1]) + d.y,
        offset(Relatives::DELTA_Z, rotated[2]) + d.z,
    ];
    (new_pos, new_vel, new_yaw, new_pitch)
}

fn rotate_x(v: [f64; 3], angle: f32) -> [f64; 3] {
    let (sin, cos) = (f64::from(angle).sin(), f64::from(angle).cos());
    [v[0], v[1] * cos + v[2] * sin, v[2] * cos - v[1] * sin]
}

fn rotate_y(v: [f64; 3], angle: f32) -> [f64; 3] {
    let (sin, cos) = (f64::from(angle).sin(), f64::from(angle).cos());
    [v[0] * cos + v[2] * sin, v[1], v[2] * cos - v[0] * sin]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> Vec3d {
        Vec3d { x, y, z }
    }

    #[test]
    fn first_health_packet_does_not_flash_but_damage_does() {
        let mut p = LocalPlayer::default();
        p.hurt_to(10.0);
        assert_eq!((p.hurt_time, p.damage_cooldown), (0, 0));
        p.hurt_to(6.0);
        assert_eq!(
            (p.hurt_time, p.hurt_duration, p.damage_cooldown),
            (10, 10, 20)
        );
        p.hurt_to(8.0);
        assert_eq!(p.damage_cooldown, 10);
    }

    #[test]
    fn teleport_marks_view_for_the_next_physics_step() {
        let mut p = LocalPlayer::default();
        p.apply_teleport(
            v(0.0, 0.0, 0.0),
            v(0.0, 0.0, 0.0),
            180.0,
            30.0,
            RelativeFlags::default(),
        );
        assert!(p.view_snapped);
        assert_eq!((p.physics.yaw, p.physics.pitch), (180.0, 30.0));
    }

    #[test]
    fn absolute_teleport_replaces_everything() {
        let mut p = LocalPlayer::default();
        p.apply_teleport(
            v(1.0, 2.0, 3.0),
            v(0.0, 0.0, 0.0),
            90.0,
            10.0,
            RelativeFlags::default(),
        );
        assert_eq!(p.pos(), [1.0, 2.0, 3.0]);
        assert_eq!((p.physics.yaw, p.physics.pitch), (90.0, 10.0));
        assert!(p.placed);
    }

    #[test]
    fn relative_flags_add_to_current_values() {
        let mut p = LocalPlayer::default();
        p.physics
            .snap_to([10.0, 64.0, 10.0], [0.0, 0.5, 0.0], 30.0, 0.0);
        let flags = RelativeFlags::X
            .union(RelativeFlags::Z)
            .union(RelativeFlags::Y_ROT)
            .union(RelativeFlags::DELTA_Y);
        p.apply_teleport(v(1.0, 70.0, -2.0), v(0.0, 0.25, 0.0), 15.0, 5.0, flags);
        assert_eq!(p.pos(), [11.0, 70.0, 8.0]);
        assert_eq!(p.physics.yaw, 45.0);
        assert_eq!(p.physics.pitch, 5.0);
        assert_eq!(p.physics.vel[1], 0.75);
    }

    #[test]
    fn non_finite_teleports_are_ignored() {
        let mut p = LocalPlayer::default();
        p.apply_teleport(
            v(f64::NAN, 0.0, 0.0),
            v(0.0, 0.0, 0.0),
            0.0,
            0.0,
            RelativeFlags::default(),
        );
        assert!(!p.placed);
        assert_eq!(p.pos(), [0.0; 3]);
    }

    #[test]
    fn pitch_is_clamped() {
        let mut p = LocalPlayer::default();
        p.apply_teleport(
            v(0.0, 0.0, 0.0),
            v(0.0, 0.0, 0.0),
            0.0,
            400.0,
            RelativeFlags::default(),
        );
        assert_eq!(p.physics.pitch, 90.0);
    }

    fn veteran() -> LocalPlayer {
        use mcr_protocol::types::component::{AttributeModifier, AttributeOperation};
        use mcr_protocol::types::entity_data::EntityDataValue;
        let mut p = LocalPlayer {
            show_death_screen: false,
            ..LocalPlayer::default()
        };
        p.attributes.insert(
            5,
            AttributeState {
                base: 0.2,
                modifiers: vec![AttributeModifier {
                    id: mcr_buf::Identifier::parse("minecraft:effect.speed").unwrap(),
                    amount: 0.4,
                    operation: AttributeOperation::AddMultipliedTotal,
                }],
                range: None,
            },
        );
        p.data.insert(15, EntityDataValue::Float(4.0));
        p.physics.snap_to([0.0; 3], [0.1, 0.2, 0.3], 30.0, 10.0);
        p
    }

    #[test]
    fn respawn_keeps_the_death_screen_choice_and_base_attributes() {
        let mut p = veteran();
        p.reset_for_spawn(GameType::Survival, None, Some(0));
        assert!(!p.show_death_screen, "immediate respawn stays on");
        assert_eq!(p.attributes[&5].base, 0.2);
        assert!(p.attributes[&5].modifiers.is_empty());
        assert!(p.data.is_empty());
        assert_eq!(p.physics.yaw, -180.0);
    }

    #[test]
    fn respawn_flags_keep_modifiers_data_motion_and_rotation() {
        let mut p = veteran();
        p.reset_for_spawn(GameType::Survival, None, Some(Respawn::KEEP_ALL_DATA));
        assert_eq!(p.attributes[&5].modifiers.len(), 1);
        assert_eq!(p.data.len(), 1);
        assert_eq!(p.physics.vel, [0.1, 0.2, 0.3]);
        assert_eq!((p.physics.yaw, p.physics.pitch), (30.0, 10.0));
    }

    #[test]
    fn login_starts_a_fresh_player() {
        let mut p = veteran();
        p.reset_for_spawn(GameType::Survival, None, None);
        assert!(p.show_death_screen);
        assert!(p.attributes.is_empty());
    }

    #[test]
    fn creative_enables_flight_permission() {
        let mut p = LocalPlayer::default();
        p.apply_game_mode(GameType::Creative);
        assert!(p.physics.abilities.may_fly && p.instabuild);
    }
}
