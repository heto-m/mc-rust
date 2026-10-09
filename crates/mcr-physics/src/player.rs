use crate::aabb::Aabb;
use crate::collide::{Colliders, CollisionCtx};
use crate::data::{PhysicsData, StateInfo, flag};
use crate::types::{Abilities, Attributes, Effects, MoveKind, MovementOutput, PlayerInput, Pose};
use crate::world::CollisionWorld;

pub(crate) const SPRINT_WINDOW: i32 = 7;
pub(crate) const SPRINT_MODIFIER: f64 = 0.30000001192092896;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct FluidState {
    pub water_height: f64,
    pub lava_height: f64,
    pub eye_in_water: bool,
}

#[derive(Debug, Clone)]
pub struct PlayerPhysics {
    pub(crate) data: &'static PhysicsData,
    pub pos: [f64; 3],
    pub vel: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub vertical_collision: bool,
    pub minor_horizontal_collision: bool,
    pub fall_distance: f64,
    pub pose: Pose,
    pub sprinting: bool,
    pub swimming: bool,
    pub crouching: bool,
    pub spectator: bool,
    pub abilities: Abilities,
    pub attributes: Attributes,
    pub effects: Effects,
    pub food_level: i32,
    pub leather_boots: bool,
    pub fast_lava: bool,
    pub(crate) fluid: FluidState,
    pub(crate) was_touching_water: bool,
    pub(crate) was_eye_in_water: bool,
    pub(crate) no_jump_delay: i32,
    pub(crate) jump_trigger_time: i32,
    pub(crate) sprint_trigger_time: i32,
    pub(crate) keys: PlayerInput,
    pub(crate) move_vec: [f32; 2],
    pub(crate) stuck: [f64; 3],
    pub(crate) support: Option<[i32; 3]>,
    pub(crate) on_ground_no_blocks: bool,
    pub(crate) in_powder_snow: bool,
    pub(crate) was_in_powder_snow: bool,
    pub(crate) jumping: bool,
    pub(crate) vertical_collision_below: bool,
    pub(crate) move_from: Option<[f64; 3]>,
    pub(crate) sent: SentState,
    pub(crate) colliders: Colliders,
    pub(crate) step_scratch: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SentState {
    pub input: PlayerInput,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub reminder: i32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub sprinting: bool,
}

impl PlayerPhysics {
    pub fn new(pos: [f64; 3]) -> Self {
        Self::with_data(PhysicsData::v26_3(), pos)
    }

    pub fn with_data(data: &'static PhysicsData, pos: [f64; 3]) -> Self {
        Self {
            data,
            pos,
            vel: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            horizontal_collision: false,
            vertical_collision: false,
            minor_horizontal_collision: false,
            fall_distance: 0.0,
            pose: Pose::Standing,
            sprinting: false,
            swimming: false,
            crouching: false,
            spectator: false,
            abilities: Abilities::default(),
            attributes: Attributes::default(),
            effects: Effects::default(),
            food_level: 20,
            leather_boots: false,
            fast_lava: false,
            fluid: FluidState::default(),
            was_touching_water: false,
            was_eye_in_water: false,
            no_jump_delay: 0,
            jump_trigger_time: 0,
            sprint_trigger_time: 0,
            keys: PlayerInput::default(),
            move_vec: [0.0; 2],
            stuck: [0.0; 3],
            support: None,
            on_ground_no_blocks: false,
            in_powder_snow: false,
            was_in_powder_snow: false,
            jumping: false,
            vertical_collision_below: false,
            move_from: None,
            sent: SentState {
                input: PlayerInput::default(),
                pos,
                yaw: 0.0,
                pitch: 0.0,
                reminder: 0,
                on_ground: false,
                horizontal_collision: false,
                sprinting: false,
            },
            colliders: Colliders::default(),
            step_scratch: Vec::new(),
        }
    }

    /// Server teleport or respawn: also resets what the position-packet logic compares against.
    pub fn snap_to(&mut self, pos: [f64; 3], vel: [f64; 3], yaw: f32, pitch: f32) {
        self.pos = pos;
        self.vel = vel;
        self.yaw = yaw;
        self.pitch = pitch;
        self.sent.pos = pos;
        self.sent.yaw = yaw;
        self.sent.pitch = pitch;
        self.sent.reminder = 0;
    }

    pub fn bounding_box(&self) -> Aabb {
        self.bounding_box_for(self.pose, self.pos)
    }

    pub(crate) fn bounding_box_for(&self, pose: Pose, p: [f64; 3]) -> Aabb {
        let w = f64::from(0.6f32 / 2.0);
        let h = f64::from(pose.height());
        Aabb::new([p[0] - w, p[1], p[2] - w], [p[0] + w, p[1] + h, p[2] + w])
    }

    pub fn eye_height(&self) -> f32 {
        self.pose.eye_height()
    }

    pub(crate) fn eye_y(&self) -> f64 {
        self.pos[1] + f64::from(self.eye_height())
    }

    pub(crate) fn block_pos(&self) -> [i32; 3] {
        [
            self.pos[0].floor() as i32,
            self.pos[1].floor() as i32,
            self.pos[2].floor() as i32,
        ]
    }

    pub(crate) fn max_up_step(&self) -> f32 {
        self.attributes.step_height as f32
    }

    pub(crate) fn ctx(&self) -> CollisionCtx {
        CollisionCtx {
            y: self.pos[1],
            fall_distance: self.fall_distance,
            powder_walk: self.leather_boots,
            descending: self.keys.sneak,
        }
    }

    pub(crate) fn state_at<W: CollisionWorld + ?Sized>(
        &self,
        w: &W,
        p: [i32; 3],
    ) -> &'static StateInfo {
        self.data.state(w.block_state(p[0], p[1], p[2]))
    }

    pub(crate) fn is_in_water(&self) -> bool {
        self.was_touching_water
    }

    pub(crate) fn is_in_lava(&self) -> bool {
        self.fluid.lava_height > 0.0
    }

    pub(crate) fn is_under_water(&self) -> bool {
        self.was_eye_in_water && self.was_touching_water
    }

    pub(crate) fn is_visually_crawling(&self) -> bool {
        self.pose == Pose::Swimming && !self.was_touching_water
    }

    pub(crate) fn is_moving_slowly(&self) -> bool {
        self.crouching || self.is_visually_crawling()
    }

    pub(crate) fn fluid_jump_threshold(&self) -> f64 {
        if self.eye_height() < 0.4 { 0.0 } else { 0.4 }
    }

    pub(crate) fn effective_gravity(&self) -> f64 {
        let falling = self.vel[1] <= 0.0;
        if falling && self.effects.slow_falling {
            self.attributes.gravity.min(0.01)
        } else {
            self.attributes.gravity
        }
    }

    pub(crate) fn speed(&self) -> f32 {
        let mut v = self.attributes.movement_speed;
        if self.sprinting {
            v *= 1.0 + SPRINT_MODIFIER;
        }
        v.clamp(0.0, 1024.0) as f32
    }

    pub(crate) fn flying_speed(&self) -> f32 {
        if self.abilities.flying {
            if self.sprinting {
                self.abilities.flying_speed * 2.0
            } else {
                self.abilities.flying_speed
            }
        } else if self.sprinting {
            0.025999999
        } else {
            0.02
        }
    }

    pub(crate) fn on_ground_pos(
        &self,
        offset: f32,
        w: &(impl CollisionWorld + ?Sized),
    ) -> [i32; 3] {
        if let Some(s) = self.support {
            if offset <= 1.0e-5 {
                return s;
            }
            let st = self.state_at(w, s);
            let keep = (offset <= 0.5 && st.has(flag::FENCE))
                || st.has(flag::WALL)
                || st.has(flag::FENCE_GATE);
            if keep {
                s
            } else {
                [s[0], (self.pos[1] - f64::from(offset)).floor() as i32, s[2]]
            }
        } else {
            [
                self.pos[0].floor() as i32,
                (self.pos[1] - f64::from(offset)).floor() as i32,
                self.pos[2].floor() as i32,
            ]
        }
    }

    pub(crate) fn block_below_affecting_movement(
        &self,
        w: &(impl CollisionWorld + ?Sized),
    ) -> [i32; 3] {
        self.on_ground_pos(0.500001, w)
    }

    pub(crate) fn build_output(&mut self, input: &PlayerInput) -> MovementOutput {
        let s = &mut self.sent;
        let input_changed = !s.input.keys_equal(input);
        let mut out_input = None;
        if input_changed {
            out_input = Some(*input);
            s.input = *input;
        }
        let mut sprint_command = None;
        if self.sprinting != s.sprinting {
            sprint_command = Some(self.sprinting);
            s.sprinting = self.sprinting;
        }
        let d = [
            self.pos[0] - s.pos[0],
            self.pos[1] - s.pos[1],
            self.pos[2] - s.pos[2],
        ];
        let dyaw = self.yaw - s.yaw;
        let dpitch = self.pitch - s.pitch;
        s.reminder += 1;
        let limit = 2.0e-4f64;
        let moved = d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > limit * limit || s.reminder >= 20;
        let rot = dyaw != 0.0 || dpitch != 0.0;
        let kind = if moved && rot {
            MoveKind::PosRot
        } else if moved {
            MoveKind::Pos
        } else if rot {
            MoveKind::Rot
        } else if s.on_ground != self.on_ground
            || s.horizontal_collision != self.horizontal_collision
        {
            MoveKind::StatusOnly
        } else {
            MoveKind::None
        };
        if moved {
            s.pos = self.pos;
            s.reminder = 0;
        }
        if rot {
            s.yaw = self.yaw;
            s.pitch = self.pitch;
        }
        s.on_ground = self.on_ground;
        s.horizontal_collision = self.horizontal_collision;
        MovementOutput {
            kind,
            pos: self.pos,
            yaw: self.yaw,
            pitch: self.pitch,
            on_ground: self.on_ground,
            horizontal_collision: self.horizontal_collision,
            input: out_input,
            sprint_command,
        }
    }
}
