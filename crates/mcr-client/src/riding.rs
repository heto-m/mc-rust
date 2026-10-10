//! Riding: client-authoritative vehicles (boats, saddled horses, pigs) and the packets that go with them.

use mcr_physics::{Attributes, Boat, Mount, PlayerInput};
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::types::Vec3d;
use mcr_protocol::types::component::EquipmentSlot;
use mcr_protocol::types::game::Hand;

use crate::client::Client;
use crate::entity::Entity;
use crate::error::Result;

/// Position, yaw and pitch.
type Pose = ([f64; 3], f32, f32);

#[derive(Debug, Clone)]
pub enum Sim {
    Boat(Boat),
    Mount {
        mount: Mount,
        pig: bool,
    },
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VehicleKind {
    Boat,
    Horse,
    Pig,
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
            VehicleKind::Horse | VehicleKind::Pig => {
                let defaults = Attributes::default();
                let attrs = Attributes {
                    jump_strength: self
                        .entity_attribute(e, "jump_strength")
                        .unwrap_or(defaults.jump_strength),
                    step_height: self.entity_attribute(e, "step_height").unwrap_or(1.0),
                    gravity: self
                        .entity_attribute(e, "gravity")
                        .unwrap_or(defaults.gravity),
                    ..defaults
                };
                Sim::Mount {
                    mount: Mount::new(e.target_pos, e.target_yaw, dims.0, dims.1, attrs),
                    pig: kind == VehicleKind::Pig,
                }
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
        match &mut ride.sim {
            Sim::Boat(boat) => {
                if teleported {
                    boat.snap_to(entity.target_pos, entity.target_yaw);
                }
                boat.tick(&self.state.chunks, keys);
                paddle = Some(boat.paddle);
                vehicle_pose = (boat.body.pos, boat.yaw(), 0.0);
            }
            Sim::Mount { mount, pig } => {
                if teleported {
                    mount.snap_to(entity.target_pos, entity.target_yaw, entity.target_pitch);
                }
                let speed = self
                    .entity_attribute(&entity, "movement_speed")
                    .unwrap_or(0.2) as f32;
                let mut rider = *keys;
                if *pig {
                    rider = PlayerInput {
                        forward: true,
                        backward: false,
                        left: false,
                        right: false,
                        ..rider
                    };
                } else {
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
                        ride.jump_ticks += 1;
                        ride.jump_scale = if ride.jump_ticks < 10 {
                            ride.jump_ticks as f32 * 0.1
                        } else {
                            0.8 + 2.0 / (ride.jump_ticks - 9) as f32 * 0.1
                        };
                    }
                    ride.was_jumping = jump;
                }
                let ridden_speed = if *pig { speed * 0.225 } else { speed };
                mount.tick(&self.state.chunks, &rider, ridden_speed);
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
        let seat = self
            .state
            .entities
            .get(vehicle)
            .map_or(vehicle_pose.0, |e| e.pos);
        let p = &mut self.state.player.physics;
        p.passenger = true;
        p.snap_to(
            [seat[0], seat[1] - 0.4, seat[2]],
            [0.0; 3],
            keys.yaw,
            keys.pitch,
        );
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
