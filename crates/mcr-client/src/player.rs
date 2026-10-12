use mcr_physics::{Abilities, Attributes, Effects, PlayerPhysics};
use mcr_protocol::play::clientbound::player::RelativeFlags;
use mcr_protocol::types::Vec3d;
use mcr_protocol::types::game::GameType;
use rustc_hash::FxHashMap;

use crate::entity::{AttributeState, MobEffect};

const XP_DISPLAY_TICKS: u32 = 100;

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
    pub vehicle: Option<i32>,
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
            vehicle: None,
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

    /// Fresh per-life state: vitals reset, position kept until the next teleport.
    pub fn reset_for_spawn(&mut self, game_mode: GameType, previous: Option<GameType>) {
        let entity_id = self.entity_id;
        let held = self.held_slot;
        let permission_level = self.permission_level;
        *self = Self {
            entity_id,
            held_slot: held,
            permission_level,
            game_mode,
            previous_game_mode: previous,
            ..Self::default()
        };
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
        let cur_pos = self.physics.pos;
        let cur_vel = self.physics.vel;
        let (cur_yaw, cur_pitch) = (self.physics.yaw, self.physics.pitch);
        let rel = |flag: RelativeFlags| relatives.contains(flag);
        let pos = [
            if rel(RelativeFlags::X) {
                cur_pos[0] + position.x
            } else {
                position.x
            },
            if rel(RelativeFlags::Y) {
                cur_pos[1] + position.y
            } else {
                position.y
            },
            if rel(RelativeFlags::Z) {
                cur_pos[2] + position.z
            } else {
                position.z
            },
        ];
        let yaw = if rel(RelativeFlags::Y_ROT) {
            cur_yaw + y_rot
        } else {
            y_rot
        };
        let pitch = if rel(RelativeFlags::X_ROT) {
            cur_pitch + x_rot
        } else {
            x_rot
        };
        let mut vel = cur_vel;
        if rel(RelativeFlags::ROTATE_DELTA) {
            vel = rotate_x(vel, (cur_pitch - pitch).to_radians());
            vel = rotate_y(vel, (cur_yaw - yaw).to_radians());
        }
        let vel = [
            if rel(RelativeFlags::DELTA_X) {
                vel[0] + delta.x
            } else {
                delta.x
            },
            if rel(RelativeFlags::DELTA_Y) {
                vel[1] + delta.y
            } else {
                delta.y
            },
            if rel(RelativeFlags::DELTA_Z) {
                vel[2] + delta.z
            } else {
                delta.z
            },
        ];
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

    #[test]
    fn creative_enables_flight_permission() {
        let mut p = LocalPlayer::default();
        p.apply_game_mode(GameType::Creative);
        assert!(p.physics.abilities.may_fly && p.instabuild);
    }
}
