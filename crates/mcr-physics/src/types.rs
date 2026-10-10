#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlayerInput {
    pub forward: bool,
    pub backward: bool,
    pub left: bool,
    pub right: bool,
    pub jump: bool,
    pub sneak: bool,
    pub sprint: bool,
    pub yaw: f32,
    pub pitch: f32,
}

impl PlayerInput {
    pub fn keys_equal(&self, o: &PlayerInput) -> bool {
        self.forward == o.forward
            && self.backward == o.backward
            && self.left == o.left
            && self.right == o.right
            && self.jump == o.jump
            && self.sneak == o.sneak
            && self.sprint == o.sprint
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Abilities {
    pub flying: bool,
    pub may_fly: bool,
    pub flying_speed: f32,
}

impl Default for Abilities {
    fn default() -> Self {
        Self {
            flying: false,
            may_fly: false,
            flying_speed: 0.05,
        }
    }
}

/// Final attribute values (modifiers other than sprinting already applied).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attributes {
    pub movement_speed: f64,
    pub jump_strength: f64,
    pub step_height: f64,
    pub gravity: f64,
    pub sneaking_speed: f64,
    pub water_movement_efficiency: f64,
    pub movement_efficiency: f64,
    pub friction_modifier: f64,
    pub air_drag_modifier: f64,
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            movement_speed: f64::from(0.1f32),
            jump_strength: f64::from(0.42f32),
            step_height: 0.6,
            gravity: 0.08,
            sneaking_speed: 0.3,
            water_movement_efficiency: 0.0,
            movement_efficiency: 0.0,
            friction_modifier: 1.0,
            air_drag_modifier: 1.0,
        }
    }
}

/// The `use_effects` of the item being used; slows and optionally forbids sprinting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemUse {
    pub can_sprint: bool,
    pub speed_multiplier: f32,
}

impl Default for ItemUse {
    fn default() -> Self {
        Self {
            can_sprint: false,
            speed_multiplier: 0.2,
        }
    }
}

/// Amplifier is the vanilla zero-based level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Effects {
    pub jump_boost: Option<u8>,
    pub levitation: Option<u8>,
    pub slow_falling: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pose {
    Standing,
    Crouching,
    Swimming,
}

impl Pose {
    pub fn height(self) -> f32 {
        match self {
            Pose::Standing => 1.8,
            Pose::Crouching => 1.5,
            Pose::Swimming => 0.6,
        }
    }

    pub fn eye_height(self) -> f32 {
        match self {
            Pose::Standing => 1.62,
            Pose::Crouching => 1.27,
            Pose::Swimming => 0.4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    None,
    StatusOnly,
    Rot,
    Pos,
    PosRot,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementOutput {
    pub kind: MoveKind,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    /// Set when the held keys changed since the last sent input packet.
    pub input: Option<PlayerInput>,
    /// `Some(true)` for start-sprinting, `Some(false)` for stop-sprinting.
    pub sprint_command: Option<bool>,
}
