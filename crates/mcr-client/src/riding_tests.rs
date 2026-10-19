use mcr_physics::{MountMode, PlayerInput};
use mcr_protocol::dispatch::PlayClientbound as P;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::clientbound::entity::{
    EntityPositionSync, LpVec3, PositionPath, SetEntityMotion,
};
use mcr_protocol::types::component::EquipmentSlot;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::{ItemStack, Vec3d};
use mcr_world::{BlockState, ChunkColumn, ChunkPos};
use uuid::Uuid;

use crate::client::Client;
use crate::entity::{AttributeState, Entity};
use crate::events::ClientEvent;
use crate::inventory::HOTBAR_START;
use crate::riding::Sim;
use crate::state::Outbox;

const PLAYER: i32 = 7;
const VEHICLE: i32 = 40;

fn state_of(c: &Client, name: &str) -> u32 {
    let b = c.state.data.block_by_name(name).unwrap();
    c.state.data.block_info(b).unwrap().default_state
}

fn client(floor: &str, fill: Option<&str>) -> Client {
    let mut c = Client::for_test();
    c.state.player.entity_id = PLAYER;
    let cfg = *c.state.chunks.config();
    c.state
        .chunks
        .insert(ChunkPos::new(0, 0), ChunkColumn::empty(&cfg));
    let floor = state_of(&c, floor);
    let fill = fill.map(|f| state_of(&c, f));
    for x in 0..16 {
        for z in 0..16 {
            c.state.chunks.set_block(x, 59, z, BlockState(floor));
            if let Some(f) = fill {
                for y in 60..64 {
                    c.state.chunks.set_block(x, y, z, BlockState(f));
                }
            }
        }
    }
    c
}

fn item(c: &Client, name: &str) -> ItemStack {
    ItemStack::new(c.state.data.item_by_name(name).unwrap() as i32, 1)
}

fn mount(c: &mut Client, kind: &str, pos: [f64; 3], yaw: f32) {
    let kind = c.state.data.entity_type_by_name(kind).unwrap() as i32;
    let mut e = Entity::new(VEHICLE, Uuid::from_u128(40), kind, pos);
    e.yaw = yaw;
    e.target_yaw = yaw;
    e.passengers = vec![PLAYER];
    let saddle = item(c, "minecraft:saddle");
    e.set_equipment(EquipmentSlot::Saddle, saddle);
    e.data.insert(18, EntityDataValue::Byte(2 | 4));
    c.state.entities.insert(e);
    c.state.player.vehicle = Some(VEHICLE);
}

fn attribute(c: &mut Client, name: &str, value: f64) {
    let id = c
        .state
        .data
        .registry("minecraft:attribute")
        .and_then(|r| r.id_of(name))
        .unwrap() as i32;
    let e = c.state.entities.get_mut(VEHICLE).unwrap();
    e.attributes.insert(id, AttributeState::new(value, vec![]));
}

fn keys(yaw: f32, pitch: f32) -> PlayerInput {
    PlayerInput {
        yaw,
        pitch,
        ..PlayerInput::default()
    }
}

fn vehicle_moves(sent: &[S<'_>]) -> Vec<[f64; 3]> {
    sent.iter()
        .filter_map(|p| match p {
            S::MoveVehicle(m) => Some([m.position.x, m.position.y, m.position.z]),
            _ => None,
        })
        .collect()
}

#[test]
fn a_saddled_nautilus_is_steered_and_sends_vehicle_moves() {
    let mut c = client("minecraft:stone", Some("minecraft:water"));
    mount(&mut c, "minecraft:nautilus", [8.5, 61.0, 8.5], 0.0);
    let forward = PlayerInput {
        forward: true,
        ..keys(0.0, 0.0)
    };
    for _ in 0..20 {
        c.ride_tick(&forward, VEHICLE).unwrap();
    }
    assert_eq!(c.state.controlled_vehicle, Some(VEHICLE));
    assert!(matches!(
        c.ride.as_ref().map(|r| &r.sim),
        Some(Sim::Mount(m)) if m.mode == MountMode::Nautilus
    ));
    let moves = vehicle_moves(&c.take_sent());
    assert_eq!(moves.len(), 20);
    let last = moves.last().unwrap();
    assert!(last[2] > 9.0, "swam forward along +z: {last:?}");
    assert!(
        (last[1] - 61.0).abs() < 0.2,
        "no sinking in water: {last:?}"
    );

    let down = PlayerInput {
        forward: true,
        ..keys(0.0, 60.0)
    };
    for _ in 0..10 {
        c.ride_tick(&down, VEHICLE).unwrap();
    }
    let y = c.state.entities.get(VEHICLE).unwrap().pos[1];
    assert!(y < 60.9, "looking down dives: {y}");
}

#[test]
fn a_ridden_pig_steps_a_full_block() {
    let mut c = client("minecraft:stone", None);
    let stone = state_of(&c, "minecraft:stone");
    for x in 10..16 {
        for z in 0..16 {
            c.state.chunks.set_block(x, 60, z, BlockState(stone));
        }
    }
    mount(&mut c, "minecraft:pig", [8.5, 60.0, 8.5], -90.0);
    attribute(&mut c, "minecraft:step_height", 0.6);
    attribute(&mut c, "minecraft:movement_speed", 0.25);
    let stick = item(&c, "minecraft:carrot_on_a_stick");
    c.state.inventory.player.slots[HOTBAR_START] = stick;
    for _ in 0..60 {
        c.ride_tick(&keys(-90.0, 0.0), VEHICLE).unwrap();
        if c.state.entities.get(VEHICLE).unwrap().pos[0] > 11.0 {
            break;
        }
    }
    let Some(Sim::Mount(m)) = c.ride.as_ref().map(|r| &r.sim) else {
        panic!("pig not simulated");
    };
    assert_eq!(m.body.attributes.step_height, 1.0);
    let pos = c.state.entities.get(VEHICLE).unwrap().pos;
    assert!(
        pos[0] > 11.0 && pos[1] >= 61.0,
        "stepped onto the ledge: {pos:?}"
    );
}

#[test]
fn turning_a_boat_turns_the_rider_and_clamps_the_view() {
    let mut c = client("minecraft:stone", Some("minecraft:water"));
    mount(&mut c, "minecraft:oak_boat", [8.5, 63.5, 8.5], 0.0);
    let left = PlayerInput {
        left: true,
        ..keys(0.0, 0.0)
    };
    c.ride_tick(&left, VEHICLE).unwrap();
    let boat_yaw = c.state.entities.get(VEHICLE).unwrap().yaw;
    assert_eq!(boat_yaw, -1.0);
    assert_eq!(c.view.0, -1.0);
    assert_eq!(c.state.player.physics.yaw, -1.0);
    let turned: f32 = c
        .drain_events()
        .filter_map(|e| match e {
            ClientEvent::ViewTurned { yaw } => Some(yaw),
            _ => None,
        })
        .sum();
    assert_eq!(turned, -1.0);
    let sent = c.take_sent();
    assert!(
        sent.iter()
            .any(|p| matches!(p, S::MovePlayerRot(r) if r.y_rot == -1.0))
    );

    c.ride_tick(&keys(150.0, 0.0), VEHICLE).unwrap();
    let boat_yaw = c.state.entities.get(VEHICLE).unwrap().yaw;
    assert!((c.view.0 - (boat_yaw + 105.0)).abs() < 1e-3, "{}", c.view.0);
    assert_eq!(c.boat_view_clamp(), Some((boat_yaw, 105.0)));
}

#[test]
fn position_sync_and_motion_for_the_driven_vehicle() {
    let mut c = client("minecraft:stone", Some("minecraft:water"));
    mount(&mut c, "minecraft:oak_boat", [8.5, 63.5, 8.5], 0.0);
    c.ride_tick(&keys(0.0, 0.0), VEHICLE).unwrap();
    let before = c.state.entities.get(VEHICLE).unwrap().pos;
    let mut out = Outbox::default();
    c.state
        .handle_play(
            P::EntityPositionSync(EntityPositionSync {
                entity_id: VEHICLE,
                position: PositionPath::Linear(Vec3d {
                    x: 2.0,
                    y: 63.5,
                    z: 2.0,
                }),
                y_rot: 90.0,
                x_rot: 0.0,
                on_ground: false,
            }),
            &mut out,
            std::time::Instant::now(),
        )
        .unwrap();
    let e = c.state.entities.get(VEHICLE).unwrap();
    assert_eq!(
        e.pos, before,
        "a position sync never moves the driven vehicle"
    );
    assert_eq!(e.target_pos, before);

    c.state
        .handle_play(
            P::SetEntityMotion(SetEntityMotion {
                entity_id: VEHICLE,
                movement: LpVec3(Vec3d {
                    x: 1.0,
                    y: 0.0,
                    z: 0.0,
                }),
            }),
            &mut out,
            std::time::Instant::now(),
        )
        .unwrap();
    c.state.apply_entity_event(VEHICLE, 72);
    c.ride_tick(&keys(0.0, 0.0), VEHICLE).unwrap();
    let after = c.state.entities.get(VEHICLE).unwrap().pos;
    assert!(
        after[0] - before[0] > 0.5,
        "server motion pushes: {after:?}"
    );
    assert!(after[1] > before[1] + 0.3, "bubble column lifts: {after:?}");
}

#[test]
fn camel_jump_bar_waits_for_the_dash_cooldown_and_sprint_sticks() {
    let mut c = client("minecraft:stone", None);
    mount(&mut c, "minecraft:camel", [8.5, 60.0, 8.5], 0.0);
    attribute(&mut c, "minecraft:movement_speed", 0.09);
    let idle = keys(0.0, 0.0);
    let jump = PlayerInput { jump: true, ..idle };
    for _ in 0..5 {
        c.ride_tick(&idle, VEHICLE).unwrap();
    }
    for _ in 0..12 {
        c.ride_tick(&jump, VEHICLE).unwrap();
    }
    c.ride_tick(&idle, VEHICLE).unwrap();
    let jumps = |sent: &[S<'_>]| {
        sent.iter()
            .filter(|p| matches!(p, S::PlayerCommand(pc) if pc.action == mcr_protocol::play::serverbound::PlayerCommandKind::StartRidingJump))
            .count()
    };
    assert_eq!(jumps(&c.take_sent()), 1);
    for _ in 0..12 {
        c.ride_tick(&jump, VEHICLE).unwrap();
    }
    c.ride_tick(&idle, VEHICLE).unwrap();
    assert_eq!(jumps(&c.take_sent()), 0, "no dash while cooling down");

    let sprint = PlayerInput {
        forward: true,
        sprint: true,
        ..idle
    };
    let walk = PlayerInput {
        forward: true,
        ..idle
    };
    for _ in 0..60 {
        c.ride_tick(&idle, VEHICLE).unwrap();
    }
    c.ride_tick(&sprint, VEHICLE).unwrap();
    c.ride_tick(&walk, VEHICLE).unwrap();
    assert!(
        c.ride.as_ref().unwrap().sprinting,
        "sprint outlasts the key"
    );
    c.ride_tick(&idle, VEHICLE).unwrap();
    assert!(!c.ride.as_ref().unwrap().sprinting);
}

#[test]
fn blindness_refuses_and_stops_camel_sprint() {
    let mut c = client("minecraft:stone", None);
    mount(&mut c, "minecraft:camel", [8.5, 60.0, 8.5], 0.0);
    let idle = keys(0.0, 0.0);
    let sprint = PlayerInput {
        forward: true,
        sprint: true,
        ..idle
    };
    for _ in 0..5 {
        c.ride_tick(&idle, VEHICLE).unwrap();
    }
    c.state.player.physics.effects.blindness = true;
    c.ride_tick(&sprint, VEHICLE).unwrap();
    assert!(
        !c.ride.as_ref().unwrap().sprinting,
        "blind riders cannot sprint"
    );
    c.state.player.physics.effects.blindness = false;
    c.ride_tick(&idle, VEHICLE).unwrap();
    c.ride_tick(&sprint, VEHICLE).unwrap();
    assert!(c.ride.as_ref().unwrap().sprinting);
    c.state.player.physics.effects.blindness = true;
    c.ride_tick(&sprint, VEHICLE).unwrap();
    assert!(
        !c.ride.as_ref().unwrap().sprinting,
        "blindness stops the sprint"
    );
}

#[test]
fn an_untamed_nautilus_with_a_saddle_is_not_steered() {
    let mut c = client("minecraft:stone", Some("minecraft:water"));
    mount(&mut c, "minecraft:nautilus", [8.5, 61.0, 8.5], 0.0);
    c.state
        .entities
        .get_mut(VEHICLE)
        .unwrap()
        .data
        .insert(18, EntityDataValue::Byte(0));
    c.ride_tick(&keys(0.0, 0.0), VEHICLE).unwrap();
    assert_eq!(c.state.controlled_vehicle, None);
    assert!(vehicle_moves(&c.take_sent()).is_empty());
}
