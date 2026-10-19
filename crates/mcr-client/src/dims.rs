//! Entity dimensions as vanilla's `getDimensions`: age, the scale attribute, pose and per-type sizes.

use mcr_physics::Aabb;
use mcr_protocol::types::entity_data::{Direction, EntityDataValue, Pose};

use crate::entity::{Entity, Lerp};
use crate::state::ClientState;

const POSE: u8 = 6;
const HEALTH: u8 = 9;
const BABY: u8 = 16;
const PIGLIN_BABY: u8 = 17;
const ARMOR_STAND_FLAGS: u8 = 15;
const ARMOR_STAND_SMALL: i8 = 0x01;
const ARMOR_STAND_MARKER: i8 = 0x10;
const INTERACTION_WIDTH: u8 = 8;
const INTERACTION_HEIGHT: u8 = 9;
const CUBE_SIZE: u8 = 18;
const PHANTOM_SIZE: u8 = 16;
const SHULKER_ATTACH: u8 = 16;
const SHULKER_PEEK: u8 = 17;
const HAPPY_GHAST_STAYS_STILL: u8 = 18;
const MAX_SIDE: f64 = 512.0;

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

fn sane(v: f32, max: f64) -> f64 {
    if v.is_finite() {
        f64::from(v).clamp(0.0, max)
    } else {
        0.0
    }
}

fn flag(e: &Entity, index: u8) -> bool {
    matches!(e.data.get(&index), Some(EntityDataValue::Boolean(true)))
}

fn int(e: &Entity, index: u8) -> Option<i32> {
    match e.data.get(&index) {
        Some(EntityDataValue::Int(v)) => Some(*v),
        _ => None,
    }
}

fn float(e: &Entity, index: u8) -> Option<f32> {
    match e.data.get(&index) {
        Some(EntityDataValue::Float(v)) => Some(*v),
        _ => None,
    }
}

pub(crate) fn pose(e: &Entity) -> Pose {
    match e.data.get(&POSE) {
        Some(EntityDataValue::Pose(p)) => *p,
        _ => Pose::Standing,
    }
}

/// Booleans at index 16 that are not `AgeableMob`'s baby flag.
fn baby_index(name: &str) -> Option<u8> {
    match name {
        "allay" | "ghast" | "guardian" | "elder_guardian" => None,
        _ if name.starts_with("piglin") && name != "piglin_brute" => Some(PIGLIN_BABY),
        _ => Some(BABY),
    }
}

impl ClientState {
    fn short_type_name(&self, e: &Entity) -> &'static str {
        u32::try_from(e.kind)
            .ok()
            .and_then(|k| self.data.entity_type_name(k))
            .map_or("", short)
    }

    /// Living entities sync their health as a float at index 9; interactions are handled first.
    pub(crate) fn is_living(&self, e: &Entity) -> bool {
        matches!(e.data.get(&HEALTH), Some(EntityDataValue::Float(_)))
    }

    /// The interpolation handler vanilla gives `e`'s type.
    pub(crate) fn lerp_for(&self, e: &Entity) -> Lerp {
        let name = self.short_type_name(e);
        if self.is_living(e) {
            return match name {
                "shulker" => Lerp::Snap,
                "player" | "allay" | "mannequin" => Lerp::Stepped(2),
                _ => Lerp::Stepped(3),
            };
        }
        let linear = name.ends_with("_boat")
            || name.ends_with("_raft")
            || name.ends_with("minecart")
            || matches!(name, "experience_orb" | "fishing_bobber");
        if linear { Lerp::Linear(3) } else { Lerp::Snap }
    }

    /// The eye position `PlayerLookAt` aims at: vanilla `getEyePosition` for the entity's pose.
    pub(crate) fn entity_eye(&self, e: &Entity) -> [f64; 3] {
        let base = u32::try_from(e.kind)
            .ok()
            .and_then(|k| self.data.entity_type_info(k));
        let (_, h) = self.entity_dims(e);
        let eye = match self.short_type_name(e) {
            "player" | "mannequin" if self.is_living(e) => {
                let standing = match pose(e) {
                    Pose::Crouching => 1.27,
                    Pose::Swimming | Pose::FallFlying | Pose::SpinAttack => 0.4,
                    Pose::Sleeping | Pose::Dying => 0.2,
                    _ => 1.62,
                };
                standing * self.entity_scale(e)
            }
            _ => base.map_or(0.0, |i| {
                let full = f64::from(i.height);
                let eye = f64::from(i.eye_height);
                if full > 0.0 { eye * h / full } else { eye }
            }),
        };
        [e.pos[0], e.pos[1] + eye, e.pos[2]]
    }

    /// Vanilla `isAlive` for a living entity: health above zero.
    pub(crate) fn entity_alive(&self, e: &Entity) -> bool {
        float(e, HEALTH).is_none_or(|h| h > 0.0 || h.is_nan())
    }

    fn entity_scale(&self, e: &Entity) -> f64 {
        let s = self
            .ids
            .scale
            .and_then(|id| e.attributes.get(&id))
            .map_or(1.0, |a| a.value(None));
        if s.is_finite() {
            s.clamp(0.0625, 16.0)
        } else {
            1.0
        }
    }

    pub(crate) fn entity_is_baby(&self, e: &Entity) -> bool {
        let name = self.short_type_name(e);
        if name == "armor_stand" {
            return matches!(
                e.data.get(&ARMOR_STAND_FLAGS),
                Some(EntityDataValue::Byte(f)) if f & ARMOR_STAND_SMALL != 0
            );
        }
        baby_index(name).is_some_and(|i| flag(e, i))
    }

    fn player_attribute(&self, id: Option<i32>, default: f64) -> f64 {
        id.and_then(|id| self.player.attributes.get(&id))
            .map_or(default, |a| a.value(None))
    }

    /// `Player#blockInteractionRange`.
    pub fn block_interaction_range(&self) -> f64 {
        self.player_attribute(self.ids.block_range, 4.5)
    }

    /// `Player#entityInteractionRange`.
    pub fn entity_interaction_range(&self) -> f64 {
        self.player_attribute(self.ids.entity_range, 3.0)
    }

    /// Width and height of `e` in its current state.
    pub fn entity_dims(&self, e: &Entity) -> (f64, f64) {
        let Some(info) = u32::try_from(e.kind)
            .ok()
            .and_then(|k| self.data.entity_type_info(k))
        else {
            return (0.0, 0.0);
        };
        let name = self.short_type_name(e);
        let (mut w, mut h) = (f64::from(info.width), f64::from(info.height));
        if name == "interaction" {
            let w = float(e, INTERACTION_WIDTH).map_or(1.0, |v| sane(v, MAX_SIDE));
            let h = float(e, INTERACTION_HEIGHT).map_or(1.0, |v| sane(v, MAX_SIDE));
            return (w, h);
        }
        if !self.is_living(e) {
            return (w, h);
        }
        let pose = pose(e);
        if pose == Pose::Sleeping {
            return (0.2, 0.2);
        }
        match name {
            "player" | "mannequin" => {
                (w, h) = match pose {
                    Pose::Crouching => (0.6, 1.5),
                    Pose::Swimming | Pose::FallFlying | Pose::SpinAttack => (0.6, 0.6),
                    Pose::Dying => return (0.2, 0.2),
                    _ => (0.6, 1.8),
                };
            }
            "armor_stand"
                if matches!(
                    e.data.get(&ARMOR_STAND_FLAGS),
                    Some(EntityDataValue::Byte(f)) if f & ARMOR_STAND_MARKER != 0
                ) =>
            {
                return (0.0, 0.0);
            }
            "slime" | "magma_cube" | "sulfur_cube" => {
                let size = f64::from(int(e, CUBE_SIZE).unwrap_or(1).clamp(1, 127));
                (w, h) = (w * size, h * size);
            }
            "phantom" => {
                let k = 1.0 + 0.15 * f64::from(int(e, PHANTOM_SIZE).unwrap_or(0).clamp(0, 64));
                (w, h) = (w * k, h * k);
            }
            _ if self.entity_is_baby(e) => {
                let k = if name == "turtle" { 0.3 } else { 0.5 };
                (w, h) = (w * k, h * k);
            }
            _ => {}
        }
        let s = self.entity_scale(e);
        (w * s, h * s)
    }

    /// The entity's bounding box at its interpolated position.
    pub fn entity_box(&self, e: &Entity) -> Aabb {
        if self.short_type_name(e) == "shulker" {
            return self.shulker_box(e);
        }
        let (w, h) = self.entity_dims(e);
        let p = e.pos;
        Aabb::new(
            [p[0] - w / 2.0, p[1], p[2] - w / 2.0],
            [p[0] + w / 2.0, p[1] + h, p[2] + w / 2.0],
        )
    }

    /// `Shulker#makeBoundingBox`: the shell grows away from its attach face as it peeks.
    fn shulker_box(&self, e: &Entity) -> Aabb {
        let size = self.entity_scale(e);
        let peek = match e.data.get(&SHULKER_PEEK) {
            Some(EntityDataValue::Byte(b)) => f32::from(*b).clamp(0.0, 100.0) / 100.0,
            _ => 0.0,
        };
        let physical = f64::from(0.5 - ((0.5 + peek) * std::f32::consts::PI).sin() * 0.5);
        let step = match e.data.get(&SHULKER_ATTACH) {
            Some(EntityDataValue::Direction(d)) => match d {
                Direction::Down => [0.0, 1.0, 0.0],
                Direction::Up => [0.0, -1.0, 0.0],
                Direction::North => [0.0, 0.0, 1.0],
                Direction::South => [0.0, 0.0, -1.0],
                Direction::West => [1.0, 0.0, 0.0],
                Direction::East => [-1.0, 0.0, 0.0],
            },
            _ => [0.0, 1.0, 0.0],
        };
        let p = e.pos;
        let mut bb = Aabb::new(
            [p[0] - size / 2.0, p[1], p[2] - size / 2.0],
            [p[0] + size / 2.0, p[1] + size, p[2] + size / 2.0],
        );
        bb = bb.expand_towards(step.map(|s| s * physical * size));
        bb
    }

    /// Vanilla `canBeCollidedWith` towards the local player: boats, live shulkers and adult happy
    /// ghasts the player is above or that stay still.
    pub(crate) fn entity_collidable(&self, e: &Entity, bb: &Aabb) -> bool {
        let name = self.short_type_name(e);
        if name.ends_with("_boat") || name.ends_with("_raft") {
            return true;
        }
        match name {
            "shulker" => self.entity_alive(e),
            "happy_ghast" => {
                !self.entity_is_baby(e)
                    && self.entity_alive(e)
                    && (self.player.physics.pos[1] >= bb.max[1] || flag(e, HAPPY_GHAST_STAYS_STILL))
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use mcr_protocol::types::entity_data::{EntityDataValue, Pose};
    use uuid::Uuid;

    use crate::config::ClientConfig;
    use crate::entity::{AttributeState, Entity};
    use crate::state::ClientState;

    fn state() -> ClientState {
        ClientState::new(&ClientConfig::default(), Uuid::nil())
    }

    fn entity(name: &str) -> Entity {
        let kind = mcr_data::v26_3()
            .entity_type_by_name(&format!("minecraft:{name}"))
            .unwrap();
        let mut e = Entity::new(1, Uuid::nil(), kind as i32, [0.0, 64.0, 0.0]);
        e.data.insert(9, EntityDataValue::Float(10.0));
        e
    }

    fn close(a: (f64, f64), b: (f64, f64)) {
        assert!(
            (a.0 - b.0).abs() < 1e-5 && (a.1 - b.1).abs() < 1e-5,
            "{a:?} vs {b:?}"
        );
    }

    #[test]
    fn babies_are_half_size() {
        let s = state();
        let mut cow = entity("cow");
        let adult = s.entity_dims(&cow);
        cow.data.insert(16, EntityDataValue::Boolean(true));
        close(s.entity_dims(&cow), (adult.0 * 0.5, adult.1 * 0.5));
    }

    #[test]
    fn scale_attribute_and_pose() {
        let s = state();
        let id = s.ids.scale.unwrap();
        let mut zombie = entity("zombie");
        zombie
            .attributes
            .insert(id, AttributeState::new(2.0, Vec::new()));
        close(s.entity_dims(&zombie), (1.2, 3.9));
        zombie
            .attributes
            .insert(id, AttributeState::new(1.0e9, Vec::new()));
        close(s.entity_dims(&zombie), (0.6 * 16.0, 1.95 * 16.0));
        let mut player = entity("player");
        player
            .data
            .insert(6, EntityDataValue::Pose(Pose::Crouching));
        close(s.entity_dims(&player), (0.6, 1.5));
        player.data.insert(6, EntityDataValue::Pose(Pose::Sleeping));
        close(s.entity_dims(&player), (0.2, 0.2));
    }

    #[test]
    fn slimes_and_interactions_use_synced_sizes() {
        let s = state();
        let mut slime = entity("slime");
        slime.data.insert(18, EntityDataValue::Int(4));
        close(s.entity_dims(&slime), (2.08, 2.08));
        let mut i = entity("interaction");
        i.data.insert(8, EntityDataValue::Float(3.0));
        i.data.insert(9, EntityDataValue::Float(f32::NAN));
        close(s.entity_dims(&i), (3.0, 0.0));
    }

    #[test]
    fn closed_shulker_is_a_unit_cube_and_opens_upward() {
        let s = state();
        let mut sh = entity("shulker");
        let bb = s.entity_box(&sh);
        assert_eq!(bb.max[1] - bb.min[1], 1.0);
        sh.data.insert(17, EntityDataValue::Byte(100));
        let bb = s.entity_box(&sh);
        assert!((bb.max[1] - 65.0 - 1.0).abs() < 1e-6, "{bb:?}");
    }
}
