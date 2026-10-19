use std::time::Instant;

use mcr_buf::Angle;
use mcr_protocol::dispatch::PlayClientbound as P;
use mcr_protocol::play::clientbound::entity::{
    AddEntity, DeltaStep, EntityPositionSync, LpVec3, MoveEntityPos, PositionMoveRotation,
    PositionPath, PositionStep, Relatives, SetEntityData, SetEntityMotion, TeleportEntity,
    VecDelta,
};
use mcr_protocol::play::clientbound::player::{EntityAnchor, LookAtEntity, PlayerLookAt};
use mcr_protocol::play::clientbound::world::BundleDelimiter;
use mcr_protocol::types::Vec3d;
use mcr_protocol::types::entity_data::{EntityDataEntry, EntityDataValue, EntityDataValues, Pose};
use uuid::Uuid;

use crate::client::Client;
use crate::config::ClientConfig;
use crate::entity::encode_delta;
use crate::state::{ClientState, Outbox};

const PLAYER: i32 = 7;

fn state() -> ClientState {
    let mut s = ClientState::new(&ClientConfig::default(), Uuid::nil());
    s.player.entity_id = PLAYER;
    s
}

fn handle(s: &mut ClientState, packet: P<'_>) {
    let mut out = Outbox::default();
    s.handle_play(packet, &mut out, Instant::now()).unwrap();
}

fn kind(s: &ClientState, name: &str) -> i32 {
    s.data
        .entity_type_by_name(&format!("minecraft:{name}"))
        .unwrap() as i32
}

fn v(x: f64, y: f64, z: f64) -> Vec3d {
    Vec3d { x, y, z }
}

fn add(id: i32, entity_type: i32, pos: [f64; 3], vel: [f64; 3]) -> P<'static> {
    P::AddEntity(AddEntity {
        entity_id: id,
        uuid: Uuid::from_u128(id as u128),
        entity_type,
        x: pos[0],
        y: pos[1],
        z: pos[2],
        velocity: LpVec3(v(vel[0], vel[1], vel[2])),
        x_rot: Angle(0),
        y_rot: Angle(0),
        y_head_rot: Angle(0),
        data: 0,
    })
}

fn data(id: i32, entries: Vec<(u8, EntityDataValue)>) -> P<'static> {
    P::SetEntityData(SetEntityData {
        entity_id: id,
        values: EntityDataValues {
            entries: entries
                .into_iter()
                .map(|(index, value)| EntityDataEntry { index, value })
                .collect(),
        },
    })
}

/// A zombie (stepped over 3 ticks) with its health synced, standing at `pos`.
fn zombie(s: &mut ClientState, id: i32, pos: [f64; 3]) {
    let zombie = kind(s, "zombie");
    handle(s, add(id, zombie, pos, [0.0; 3]));
    handle(s, data(id, vec![(9, EntityDataValue::Float(20.0))]));
}

fn delta(from: f64, to: f64) -> i16 {
    (encode_delta(to) - encode_delta(from)) as i16
}

fn pos(s: &ClientState, id: i32) -> [f64; 3] {
    s.entities.get(id).unwrap().pos
}

fn close(a: [f64; 3], b: [f64; 3]) {
    assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-6), "{a:?} vs {b:?}");
}

#[test]
fn projectiles_keep_flying_between_syncs() {
    let mut s = state();
    for (i, name) in [
        "snowball",
        "arrow",
        "trident",
        "fireball",
        "small_fireball",
        "dragon_fireball",
        "wither_skull",
        "wind_charge",
        "breeze_wind_charge",
        "llama_spit",
        "firework_rocket",
        "eye_of_ender",
        "shulker_bullet",
    ]
    .into_iter()
    .enumerate()
    {
        let id = 100 + i as i32;
        let t = kind(&s, name);
        handle(&mut s, add(id, t, [0.5, 100.0, 0.5], [0.0, 0.0, 0.5]));
        let mut last = pos(&s, id);
        for tick in 0..19 {
            s.tick();
            let now = pos(&s, id);
            assert!(now[2] > last[2] + 0.1, "{name} stalls at tick {tick}");
            last = now;
        }
    }
}

#[test]
fn thrown_projectile_matches_vanilla_order_and_resyncs() {
    let mut s = state();
    let snowball = kind(&s, "snowball");
    handle(&mut s, add(1, snowball, [0.5, 100.0, 0.5], [0.0, 0.0, 1.0]));
    s.tick();
    let drag = f64::from(0.99f32);
    close(pos(&s, 1), [0.5, 100.0 - 0.03 * drag, 0.5 + drag]);
    s.tick();
    let vy = (-0.03 * drag - 0.03) * drag;
    close(
        pos(&s, 1),
        [0.5, 100.0 - 0.03 * drag + vy, 0.5 + drag + drag * drag],
    );

    handle(
        &mut s,
        P::EntityPositionSync(EntityPositionSync {
            entity_id: 1,
            position: PositionPath::Linear(v(0.5, 99.0, 3.0)),
            y_rot: 0.0,
            x_rot: 0.0,
            on_ground: false,
        }),
    );
    assert_eq!(pos(&s, 1), [0.5, 99.0, 3.0], "a sync snaps without lerp");
    s.tick();
    assert!(pos(&s, 1)[2] > 3.5, "and the flight goes on");
}

#[test]
fn hostile_projectile_motion_stays_bounded() {
    let mut s = state();
    let arrow = kind(&s, "arrow");
    handle(&mut s, add(1, arrow, [0.5, 100.0, 0.5], [0.0; 3]));
    handle(
        &mut s,
        P::SetEntityMotion(SetEntityMotion {
            entity_id: 1,
            movement: LpVec3(v(5.0, 0.0, 0.0)),
        }),
    );
    assert_eq!(
        s.entities.get(1).unwrap().velocity,
        [5.0, 0.0, 0.0],
        "no ±3.9 clamp"
    );
    handle(
        &mut s,
        P::SetEntityMotion(SetEntityMotion {
            entity_id: 1,
            movement: LpVec3(v(1.0e9, 0.0, 0.0)),
        }),
    );
    s.tick();
    let p = pos(&s, 1);
    assert!(p[0] < 20.0, "simulation clamps the speed: {p:?}");
}

#[test]
fn living_entities_step_over_their_update_interval() {
    let mut s = state();
    zombie(&mut s, 1, [0.0, 64.0, 0.0]);
    handle(
        &mut s,
        P::MoveEntityPos(MoveEntityPos {
            entity_id: 1,
            delta: VecDelta::Linear {
                xa: delta(0.0, 3.0),
                ya: 0,
                za: 0,
            },
            on_ground: true,
        }),
    );
    let mut xs = Vec::new();
    for _ in 0..4 {
        s.tick();
        xs.push(pos(&s, 1)[0]);
    }
    for (x, want) in xs.iter().zip([1.0, 2.0, 3.0, 3.0]) {
        assert!((x - want).abs() < 1e-6, "{xs:?}");
    }
    assert!(!s.entities.get(1).unwrap().is_interpolating());

    let player = kind(&s, "player");
    handle(&mut s, add(2, player, [0.0, 64.0, 0.0], [0.0; 3]));
    handle(&mut s, data(2, vec![(9, EntityDataValue::Float(20.0))]));
    handle(
        &mut s,
        P::MoveEntityPos(MoveEntityPos {
            entity_id: 2,
            delta: VecDelta::Linear {
                xa: delta(0.0, 2.0),
                ya: 0,
                za: 0,
            },
            on_ground: true,
        }),
    );
    s.tick();
    assert!(
        (pos(&s, 2)[0] - 1.0).abs() < 1e-6,
        "players step over two ticks"
    );
    s.tick();
    assert!((pos(&s, 2)[0] - 2.0).abs() < 1e-6);
}

#[test]
fn stepped_paths_replay_every_queued_step() {
    let mut s = state();
    zombie(&mut s, 1, [0.0, 64.0, 0.0]);
    let steps = vec![
        DeltaStep {
            ticks: 1,
            xa: delta(0.0, 1.0),
            ya: 0,
            za: 0,
        },
        DeltaStep {
            ticks: 2,
            xa: 0,
            ya: 0,
            za: delta(0.0, 2.0),
        },
    ];
    handle(
        &mut s,
        P::MoveEntityPos(MoveEntityPos {
            entity_id: 1,
            delta: VecDelta::Stepped(steps),
            on_ground: true,
        }),
    );
    assert_eq!(s.entities.get(1).unwrap().target_pos, [1.0, 64.0, 2.0]);
    let mut path = Vec::new();
    for _ in 0..4 {
        s.tick();
        path.push(pos(&s, 1));
    }
    close(path[0], [1.0, 64.0, 0.0]);
    close(path[1], [1.0, 64.0, 1.0]);
    close(path[2], [1.0, 64.0, 2.0]);
    close(path[3], [1.0, 64.0, 2.0]);

    // A second update queued behind a running one is played after it, speeding up to catch up.
    zombie(&mut s, 2, [0.0, 64.0, 0.0]);
    for x in [3.0, 6.0] {
        let from = s.entities.get(2).unwrap().target_pos[0];
        handle(
            &mut s,
            P::MoveEntityPos(MoveEntityPos {
                entity_id: 2,
                delta: VecDelta::Linear {
                    xa: delta(from, x),
                    ya: 0,
                    za: 0,
                },
                on_ground: true,
            }),
        );
    }
    let mut xs = Vec::new();
    for _ in 0..8 {
        s.tick();
        xs.push(pos(&s, 2)[0]);
    }
    assert!(xs.windows(2).all(|w| w[1] >= w[0]), "{xs:?}");
    assert!((xs[0] - 1.0).abs() < 1e-6, "{xs:?}");
    assert_eq!(*xs.last().unwrap(), 6.0, "{xs:?}");
    assert!(xs[4] > 4.0, "the queue speeds playback up: {xs:?}");
}

#[test]
fn position_sync_paths_and_hostile_steps() {
    let mut s = state();
    zombie(&mut s, 1, [0.0, 64.0, 0.0]);
    let step = |x: f64, tick_offset: i32| PositionStep {
        position: v(x, 64.0, 0.0),
        tick_offset,
    };
    handle(
        &mut s,
        P::EntityPositionSync(EntityPositionSync {
            entity_id: 1,
            position: PositionPath::Stepped(vec![
                step(1.0, i32::MAX),
                step(f64::NAN, 1),
                step(2.0, -5),
            ]),
            y_rot: 0.0,
            x_rot: 0.0,
            on_ground: true,
        }),
    );
    for _ in 0..1300 {
        s.tick();
    }
    let p = pos(&s, 1);
    assert!(p.iter().all(|c| c.is_finite()));
    assert_eq!(p, [2.0, 64.0, 0.0]);

    let many: Vec<_> = (1..=200).map(|i| step(f64::from(i) * 0.01, 1)).collect();
    handle(
        &mut s,
        P::EntityPositionSync(EntityPositionSync {
            entity_id: 1,
            position: PositionPath::Stepped(many),
            y_rot: 0.0,
            x_rot: 0.0,
            on_ground: true,
        }),
    );
    for _ in 0..100 {
        s.tick();
    }
    assert_eq!(
        pos(&s, 1),
        [2.0, 64.0, 0.0],
        "a long path still ends at its end"
    );
}

#[test]
fn bundles_apply_in_one_piece() {
    let mut c = Client::for_test();
    let ids = mcr_protocol::PacketIds::v26_3();
    let frame = |p: &P<'_>| {
        let mut out = Vec::new();
        let id = p.encode_into(ids, &mut out);
        (id, out)
    };
    let delimiter = frame(&P::BundleDelimiter(BundleDelimiter));
    let pig = kind(&c.state, "pig");
    let spawn = frame(&add(5, pig, [1.0, 64.0, 1.0], [0.0; 3]));
    let health = frame(&data(5, vec![(9, EntityDataValue::Float(10.0))]));
    c.dispatch(delimiter.0, &delimiter.1).unwrap();
    c.dispatch(spawn.0, &spawn.1).unwrap();
    c.dispatch(health.0, &health.1).unwrap();
    assert!(
        c.state.entities.get(5).is_none(),
        "nothing applies mid-bundle"
    );
    c.dispatch(delimiter.0, &delimiter.1).unwrap();
    let e = c.state.entities.get(5).expect("the bundle applied");
    assert!(c.state.is_living(e));
    assert!(!c.state.unhandled.contains_key("bundle_delimiter"));

    let spawn2 = frame(&add(6, pig, [1.0, 64.0, 1.0], [0.0; 3]));
    c.dispatch(spawn2.0, &spawn2.1).unwrap();
    assert!(
        c.state.entities.get(6).is_some(),
        "unbundled packets apply at once"
    );
}

fn teleport(id: i32, change: PositionMoveRotation, relatives: u32) -> P<'static> {
    P::TeleportEntity(TeleportEntity {
        entity_id: id,
        change,
        relatives: Relatives(relatives),
        on_ground: false,
    })
}

#[test]
fn entity_teleports_honour_delta_flags() {
    let mut s = state();
    let arrow = kind(&s, "arrow");
    handle(&mut s, add(1, arrow, [0.0, 100.0, 0.0], [0.5, 0.0, 0.0]));
    handle(
        &mut s,
        teleport(
            1,
            PositionMoveRotation {
                position: v(1.0, 0.0, 0.0),
                delta_movement: v(0.25, 1.0, 0.0),
                y_rot: 0.0,
                x_rot: 0.0,
            },
            Relatives::X | Relatives::Y | Relatives::Z | Relatives::DELTA_X,
        ),
    );
    let e = s.entities.get(1).unwrap();
    assert_eq!(e.pos, [1.0, 100.0, 0.0]);
    assert_eq!(e.velocity, [0.75, 1.0, 0.0], "x adds, y replaces");

    handle(
        &mut s,
        teleport(
            1,
            PositionMoveRotation {
                position: v(0.0, 0.0, 0.0),
                delta_movement: v(0.0, 0.0, 0.0),
                y_rot: 90.0,
                x_rot: 0.0,
            },
            Relatives::X
                | Relatives::Y
                | Relatives::Z
                | Relatives::DELTA_X
                | Relatives::DELTA_Y
                | Relatives::DELTA_Z
                | Relatives::ROTATE_DELTA,
        ),
    );
    let vel = s.entities.get(1).unwrap().velocity;
    assert!(
        vel[0].abs() < 1e-6 && (vel[2] - 0.75).abs() < 1e-6 && (vel[1] - 1.0).abs() < 1e-6,
        "turning by 90° turns the motion: {vel:?}"
    );
}

fn look_at(id: i32, anchor: EntityAnchor) -> P<'static> {
    P::PlayerLookAt(PlayerLookAt {
        from_anchor: EntityAnchor::Feet,
        x: 0.0,
        y: 64.0,
        z: 10.0,
        entity: Some(LookAtEntity {
            entity_id: id,
            anchor,
        }),
    })
}

#[test]
fn look_at_uses_the_target_eye_and_falls_back_to_the_position() {
    let mut s = state();
    s.player.physics.pos = [0.0, 64.0, 0.0];
    let player = kind(&s, "player");
    handle(&mut s, add(2, player, [0.0, 64.0, 5.0], [0.0; 3]));
    handle(
        &mut s,
        data(
            2,
            vec![
                (9, EntityDataValue::Float(20.0)),
                (6, EntityDataValue::Pose(Pose::Crouching)),
            ],
        ),
    );
    handle(&mut s, look_at(2, EntityAnchor::Eyes));
    let want = -(1.27f64.atan2(5.0)).to_degrees() as f32;
    assert!(
        (s.player.physics.pitch - want).abs() < 1e-3,
        "{}",
        s.player.physics.pitch
    );

    let pig = kind(&s, "pig");
    handle(&mut s, add(3, pig, [0.0, 64.0, 5.0], [0.0; 3]));
    handle(&mut s, look_at(3, EntityAnchor::Eyes));
    let eye = f64::from(s.data.entity_type_info(pig as u32).unwrap().eye_height);
    let want = -(eye.atan2(5.0)).to_degrees() as f32;
    assert!((s.player.physics.pitch - want).abs() < 1e-3);

    s.player.physics.pitch = 30.0;
    handle(&mut s, look_at(99, EntityAnchor::Eyes));
    assert!(
        s.player.physics.pitch.abs() < 1e-3,
        "unknown entity: the packet position"
    );
    assert!(s.player.physics.yaw.abs() < 1e-3);
}
