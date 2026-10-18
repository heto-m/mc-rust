//! Riding: client-authoritative vehicles (boats, saddled horses, pigs) and the packets that go with them.

use mcr_physics::{Attributes, Boat, Mount, MountMode, PlayerInput};
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::types::Vec3d;
use mcr_protocol::types::component::EquipmentSlot;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::game::Hand;

use crate::attach;
use crate::client::Client;
use crate::entity::Entity;
use crate::error::Result;

/// Position, yaw and pitch.
type Pose = ([f64; 3], f32, f32);

#[derive(Debug, Clone)]
pub enum Sim {
    Boat(Boat),
    Mount(Mount),
    /// Server-driven vehicle: the player just follows it.
    Passive,
}

#[derive(Debug, Clone)]
pub struct Ride {
    pub vehicle: i32,
    pub sim: Sim,
    written: [f64; 3],
    sent_input: Option<PlayerInput>,
    jump_ticks: i32,
    jump_scale: f32,
    was_jumping: bool,
    boost: Boost,
    sprint_sent: bool,
}

const BOOST_INDEX: u8 = 18;
const STRIDER_SUFFOCATING_INDEX: u8 = 19;
const CAMEL_POSE_TICK_INDEX: u8 = 20;
const MAX_BOOST_TICKS: i32 = 10_000;

/// `ItemBasedSteering`: a carrot or fungus boost whose length the server syncs.
#[derive(Debug, Clone, Copy, Default)]
struct Boost {
    seen: i32,
    boosting: bool,
    time: i32,
}

impl Boost {
    fn observe(&mut self, synced: i32) {
        if synced != self.seen {
            self.seen = synced;
            self.boosting = true;
            self.time = 0;
        }
    }

    fn total(&self) -> i32 {
        self.seen.clamp(1, MAX_BOOST_TICKS)
    }

    fn tick(&mut self) {
        if self.boosting {
            let old = self.time;
            self.time = self.time.saturating_add(1);
            if old > self.total() {
                self.boosting = false;
            }
        }
    }

    fn factor(&self) -> f32 {
        if self.boosting {
            let phase = self.time as f32 / self.total() as f32;
            1.0 + 1.15 * sin32(phase * std::f32::consts::PI)
        } else {
            1.0
        }
    }
}

fn sin32(x: f32) -> f32 {
    f64::from(x).sin() as f32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VehicleKind {
    Boat,
    Horse,
    Pig,
    Strider,
    Camel,
    HappyGhast,
    Other,
}

fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}

impl Client {
    fn vehicle_kind(&self, e: &Entity) -> VehicleKind {
        let Some(name) = u32::try_from(e.kind)
            .ok()
            .and_then(|k| self.state.data.entity_type_name(k))
        else {
            return VehicleKind::Other;
        };
        let n = short(name);
        if n.ends_with("_boat") || n.ends_with("_raft") || n == "boat" {
            VehicleKind::Boat
        } else if matches!(
            n,
            "horse" | "donkey" | "mule" | "skeleton_horse" | "zombie_horse"
        ) {
            VehicleKind::Horse
        } else if n == "pig" {
            VehicleKind::Pig
        } else if n == "strider" {
            VehicleKind::Strider
        } else if matches!(n, "camel" | "camel_husk") {
            VehicleKind::Camel
        } else if n == "happy_ghast" {
            VehicleKind::HappyGhast
        } else {
            VehicleKind::Other
        }
    }

    fn entity_attribute(&self, e: &Entity, name: &str) -> Option<f64> {
        let id = self
            .state
            .data
            .registry("minecraft:attribute")?
            .id_of(&format!("minecraft:{name}"))?;
        e.attributes.get(&(id as i32)).map(|a| a.value(None))
    }

    fn passenger_feet(&self, vehicle: i32, fallback: [f64; 3]) -> [f64; 3] {
        let Some(e) = self.state.entities.get(vehicle) else {
            return fallback;
        };
        let data = self.state.data;
        let seat = e
            .passengers
            .iter()
            .position(|&p| p == self.state.player.entity_id)
            .unwrap_or(0);
        let vehicle_scale = self.entity_attribute(e, "scale").unwrap_or(1.0) as f32;
        let player_scale = data
            .registry("minecraft:attribute")
            .and_then(|r| r.id_of("minecraft:scale"))
            .and_then(|id| self.state.player.attributes.get(&(id as i32)))
            .map_or(1.0, |a| a.value(None) as f32);
        let o = attach::offset(
            &attach::Vehicle::of(data, e, vehicle_scale),
            seat,
            &attach::Rider::player(player_scale),
        );
        [e.pos[0] + o[0], e.pos[1] + o[1], e.pos[2] + o[2]]
    }

    fn int_data(&self, e: &Entity, index: u8) -> i32 {
        match e.data.get(&index) {
            Some(EntityDataValue::Int(v)) => *v,
            _ => 0,
        }
    }

    fn holds(&self, name: &str) -> bool {
        [Hand::Main, Hand::Off]
            .iter()
            .any(|&h| self.state.is_item(&self.state.hand_stack(h), name))
    }

    /// Whether the local player steers the entity (vanilla `getControllingPassenger`).
    fn controls(&self, e: &Entity, kind: VehicleKind) -> bool {
        if e.passengers.first() != Some(&self.state.player.entity_id) {
            return false;
        }
        let saddled = !e.equipment_slot(EquipmentSlot::Saddle).is_empty();
        match kind {
            VehicleKind::Boat => true,
            VehicleKind::Horse => saddled,
            VehicleKind::Pig => saddled && self.holds("carrot_on_a_stick"),
            VehicleKind::Strider => saddled && self.holds("warped_fungus_on_a_stick"),
            VehicleKind::Camel => saddled,
            VehicleKind::HappyGhast => !e.equipment_slot(EquipmentSlot::Body).is_empty(),
            VehicleKind::Other => false,
        }
    }

    fn make_sim(&self, e: &Entity, kind: VehicleKind) -> Sim {
        let dims = u32::try_from(e.kind)
            .ok()
            .and_then(|k| self.state.data.entity_type_info(k))
            .map_or((0.9, 1.0), |i| {
                (i.width.clamp(0.1, 8.0), i.height.clamp(0.1, 8.0))
            });
        match kind {
            VehicleKind::Boat => Sim::Boat(Boat::new(e.target_pos, e.target_yaw, dims.0, dims.1)),
            VehicleKind::Horse
            | VehicleKind::Pig
            | VehicleKind::Strider
            | VehicleKind::Camel
            | VehicleKind::HappyGhast => {
                let defaults = Attributes::default();
                let attrs = Attributes {
                    jump_strength: self
                        .entity_attribute(e, "jump_strength")
                        .unwrap_or(defaults.jump_strength),
                    step_height: self.entity_attribute(e, "step_height").unwrap_or(1.0),
                    gravity: if kind == VehicleKind::HappyGhast {
                        0.0
                    } else {
                        self.entity_attribute(e, "gravity")
                            .unwrap_or(defaults.gravity)
                    },
                    ..defaults
                };
                let mode = match kind {
                    VehicleKind::Pig => MountMode::Pig,
                    VehicleKind::Strider => MountMode::Strider,
                    VehicleKind::Camel => MountMode::Camel,
                    VehicleKind::HappyGhast => MountMode::HappyGhast,
                    _ => MountMode::Horse,
                };
                Sim::Mount(
                    Mount::new(e.target_pos, e.target_yaw, dims.0, dims.1, attrs).with_mode(mode),
                )
            }
            VehicleKind::Other => Sim::Passive,
        }
    }

    /// Mounts by interacting with the entity; the server answers with `set_passengers`.
    pub fn mount(&mut self, entity_id: i32) -> Result<()> {
        self.send_play(&S::Interact(sb::Interact {
            entity_id,
            hand: Hand::Main,
            location: Vec3d::default(),
            using_secondary_action: false,
        }))
    }

    /// Leaves the vehicle: vanilla dismounts when the sneak key is pressed.
    pub fn dismount_input(&self) -> PlayerInput {
        PlayerInput {
            sneak: true,
            yaw: self.view.0,
            pitch: self.view.1,
            ..PlayerInput::default()
        }
    }

    pub fn riding(&self) -> Option<&Ride> {
        self.ride.as_ref()
    }

    pub(crate) fn ride_tick(&mut self, keys: &PlayerInput, vehicle: i32) -> Result<()> {
        self.combat.hands_busy = false;
        let Some(entity) = self.state.entities.get(vehicle).cloned() else {
            self.ride = None;
            self.state.controlled_vehicle = None;
            return self.send_ride_packets(keys, None);
        };
        let kind = self.vehicle_kind(&entity);
        let controlled = self.controls(&entity, kind);
        let fresh = !self.ride.as_ref().is_some_and(|r| r.vehicle == vehicle);
        if fresh {
            self.ride = Some(Ride {
                vehicle,
                sim: if controlled {
                    self.make_sim(&entity, kind)
                } else {
                    Sim::Passive
                },
                written: entity.target_pos,
                sent_input: None,
                jump_ticks: 0,
                jump_scale: 0.0,
                was_jumping: false,
                sprint_sent: false,
                boost: Boost {
                    seen: self.int_data(&entity, BOOST_INDEX),
                    ..Boost::default()
                },
            });
        }
        let Some(mut ride) = self.ride.take() else {
            return Ok(());
        };
        if controlled && matches!(ride.sim, Sim::Passive) {
            ride.sim = self.make_sim(&entity, kind);
        }
        if !controlled {
            ride.sim = Sim::Passive;
        }
        self.state.controlled_vehicle = (!matches!(ride.sim, Sim::Passive)).then_some(vehicle);
        let teleported = entity.target_pos != ride.written;
        let mut vehicle_pose = (entity.target_pos, entity.target_yaw, entity.target_pitch);
        let mut paddle = None;
        let mut on_ground = false;
        let mut jump_data = None;
        self.combat.hands_busy = matches!(ride.sim, Sim::Boat(_))
            && (keys.forward || keys.backward || keys.left || keys.right);
        match &mut ride.sim {
            Sim::Boat(boat) => {
                if teleported {
                    boat.snap_to(entity.target_pos, entity.target_yaw);
                }
                boat.tick(&self.state.chunks, keys);
                paddle = Some(boat.paddle);
                vehicle_pose = (boat.body.pos, boat.yaw(), 0.0);
            }
            Sim::Mount(mount) => {
                if teleported {
                    mount.snap_to(entity.target_pos, entity.target_yaw, entity.target_pitch);
                }
                let attr_speed = self
                    .entity_attribute(&entity, "movement_speed")
                    .unwrap_or(0.2) as f32;
                let mode = mount.mode;
                let steered = matches!(mode, MountMode::Pig | MountMode::Strider);
                if steered {
                    ride.boost.observe(self.int_data(&entity, BOOST_INDEX));
                    ride.boost.tick();
                }
                if !steered && mode != MountMode::HappyGhast {
                    let jump = keys.jump;
                    if ride.jump_ticks < 0 {
                        ride.jump_ticks += 1;
                        if ride.jump_ticks == 0 {
                            ride.jump_scale = 0.0;
                        }
                    }
                    if ride.was_jumping && !jump {
                        ride.jump_ticks = -10;
                        let amount = (ride.jump_scale * 100.0).floor() as i32;
                        mount.rider_jump(amount);
                        jump_data = Some(amount);
                    } else if !ride.was_jumping && jump {
                        ride.jump_ticks = 0;
                        ride.jump_scale = 0.0;
                    } else if ride.was_jumping {
                        ride.jump_ticks = ride.jump_ticks.saturating_add(1);
                        ride.jump_scale = if ride.jump_ticks < 10 {
                            ride.jump_ticks as f32 * 0.1
                        } else {
                            0.8 + 2.0 / (ride.jump_ticks - 9) as f32 * 0.1
                        };
                    }
                    ride.was_jumping = jump;
                }
                let ridden_speed = match mode {
                    MountMode::Pig => {
                        (f64::from(attr_speed) * 0.225 * f64::from(ride.boost.factor())) as f32
                    }
                    MountMode::Strider => {
                        let suffocating = matches!(
                            entity.data.get(&STRIDER_SUFFOCATING_INDEX),
                            Some(EntityDataValue::Boolean(true))
                        );
                        let base = if suffocating { 0.35f32 } else { 0.55f32 };
                        (f64::from(attr_speed) * f64::from(base) * f64::from(ride.boost.factor()))
                            as f32
                    }
                    MountMode::Camel => {
                        let sprint = keys.sprint && keys.forward;
                        attr_speed
                            + if sprint && mount.dash_cooldown() == 0 {
                                0.1
                            } else {
                                0.0
                            }
                    }
                    MountMode::HappyGhast => self
                        .entity_attribute(&entity, "flying_speed")
                        .unwrap_or(0.05) as f32,
                    MountMode::Horse => attr_speed,
                };
                if mode == MountMode::Camel {
                    mount.refuse_to_move =
                        camel_refuses_to_move(&entity, self.state.world.game_time);
                }
                mount.tick(&self.state.chunks, keys, attr_speed, ridden_speed);
                on_ground = mount.body.on_ground;
                vehicle_pose = (mount.body.pos, mount.body.yaw, mount.body.pitch);
            }
            Sim::Passive => {}
        }
        if !matches!(ride.sim, Sim::Passive)
            && let Some(e) = self.state.entities.get_mut(vehicle)
        {
            e.snap_to(vehicle_pose.0, vehicle_pose.1, vehicle_pose.2);
            e.on_ground = on_ground;
            ride.written = vehicle_pose.0;
        } else {
            ride.written = entity.target_pos;
        }
        let feet = self.passenger_feet(vehicle, vehicle_pose.0);
        let p = &mut self.state.player.physics;
        p.passenger = true;
        p.snap_to(feet, [0.0; 3], keys.yaw, keys.pitch);
        p.update_passenger_fluid(&self.state.chunks);
        let controlled_now = !matches!(ride.sim, Sim::Passive);
        let pose = controlled_now.then_some((vehicle_pose, on_ground));
        if let Some(amount) = jump_data {
            let entity_id = self.state.player.entity_id;
            self.send_play(&S::PlayerCommand(sb::PlayerCommand {
                entity_id,
                action: sb::PlayerCommandKind::StartRidingJump,
                data: amount,
            }))?;
        }
        if let Some([left, right]) = paddle {
            self.send_play(&S::PaddleBoat(sb::PaddleBoat { left, right }))?;
        }
        let sprinting = matches!(&ride.sim, Sim::Mount(m) if m.mode == MountMode::Camel)
            && keys.sprint
            && keys.forward;
        if sprinting != ride.sprint_sent {
            ride.sprint_sent = sprinting;
            let entity_id = self.state.player.entity_id;
            self.send_play(&S::PlayerCommand(sb::PlayerCommand {
                entity_id,
                action: if sprinting {
                    sb::PlayerCommandKind::StartSprinting
                } else {
                    sb::PlayerCommandKind::StopSprinting
                },
                data: 0,
            }))?;
        }
        let sent = ride.sent_input;
        let changed = sent.is_none_or(|s| !s.keys_equal(keys));
        ride.sent_input = Some(*keys);
        self.ride = Some(ride);
        if changed {
            self.send_player_input(keys)?;
        }
        self.send_ride_packets(keys, pose)?;
        Ok(())
    }

    fn send_player_input(&mut self, keys: &PlayerInput) -> Result<()> {
        self.send_play(&S::PlayerInput(sb::PlayerInput {
            forward: keys.forward,
            backward: keys.backward,
            left: keys.left,
            right: keys.right,
            jump: keys.jump,
            shift: keys.sneak,
            sprint: keys.sprint,
        }))
    }

    fn send_ride_packets(
        &mut self,
        keys: &PlayerInput,
        vehicle: Option<(Pose, bool)>,
    ) -> Result<()> {
        self.send_play(&S::MovePlayerRot(sb::MovePlayerRot {
            y_rot: keys.yaw,
            x_rot: keys.pitch,
            flags: sb::MoveFlags {
                on_ground: false,
                horizontal_collision: false,
            },
        }))?;
        if let Some((([x, y, z], yaw, pitch), on_ground)) = vehicle {
            self.send_play(&S::MoveVehicle(sb::MoveVehicle {
                position: Vec3d { x, y, z },
                y_rot: yaw,
                x_rot: pitch,
                on_ground,
            }))?;
        }
        Ok(())
    }
}

/// `Camel#refuseToMove`: sitting, or still changing pose (40 ticks down, 52 up).
fn camel_refuses_to_move(e: &Entity, game_time: i64) -> bool {
    let Some(EntityDataValue::Long(last)) = e.data.get(&CAMEL_POSE_TICK_INDEX) else {
        return false;
    };
    let sitting = *last < 0;
    let pose_time = game_time.saturating_sub(last.saturating_abs());
    sitting || pose_time < if sitting { 40 } else { 52 }
}
