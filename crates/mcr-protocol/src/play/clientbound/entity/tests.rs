use mcr_buf::{from_bytes_exact, to_bytes};

use super::*;

fn decoded_like<'a, T: Decode<'a>>(_: &T, bytes: &'a [u8]) -> T {
    from_bytes_exact(bytes).unwrap()
}

macro_rules! rt {
    ($value:expr) => {{
        let value = $value;
        let bytes = to_bytes(&value);
        assert_eq!(decoded_like(&value, &bytes), value);
        bytes
    }};
}

fn v(x: f64, y: f64, z: f64) -> Vec3d {
    Vec3d { x, y, z }
}

#[test]
fn lp_vec3_fixtures() {
    assert_eq!(rt!(LpVec3::default()), [0]);
    assert_eq!(
        rt!(LpVec3(v(1.0, -1.0, 0.0))),
        [0xf1, 0xff, 0x7f, 0xfe, 0x00, 0x03]
    );
    assert_eq!(
        rt!(LpVec3(v(5.0, 0.0, 0.0))),
        [0xf5, 0xff, 0x7f, 0xfe, 0xff, 0xff, 0x01]
    );
}

#[test]
fn lp_vec3_is_lossy_but_close() {
    let bytes = to_bytes(&LpVec3(v(0.1234, -0.5, 0.0003)));
    let back = from_bytes_exact::<LpVec3>(&bytes).unwrap().0;
    assert!((back.x - 0.1234).abs() < 1e-4);
    assert!((back.y + 0.5).abs() < 1e-4);
    assert!((back.z - 0.0003).abs() < 1e-4);
    assert_eq!(to_bytes(&LpVec3(v(1e-6, 0.0, f64::NAN))), [0]);
}

#[test]
fn add_entity_bytes() {
    let packet = AddEntity {
        entity_id: 300,
        uuid: Uuid::from_u128(1),
        entity_type: 5,
        x: 1.0,
        y: 2.0,
        z: 3.0,
        velocity: LpVec3::default(),
        x_rot: Angle(64),
        y_rot: Angle(128),
        y_head_rot: Angle(0),
        data: 7,
    };
    let bytes = rt!(packet);
    assert_eq!(&bytes[..2], [0xac, 0x02]);
    assert_eq!(&bytes[2..18], 1u128.to_be_bytes());
    assert_eq!(bytes[18], 5);
    assert_eq!(&bytes[19..27], 1.0f64.to_be_bytes());
    assert_eq!(&bytes[43..], [0, 64, 128, 0, 7]);
}

#[test]
fn move_entity_pos_linear_and_stepped() {
    let linear = MoveEntityPos {
        entity_id: 5,
        delta: VecDelta::Linear {
            xa: 1,
            ya: -1,
            za: 2,
        },
        on_ground: true,
    };
    assert_eq!(rt!(linear), [5, 1, 0, 1, 0xff, 0xff, 0, 2]);

    let stepped = MoveEntityPos {
        entity_id: 5,
        delta: VecDelta::Stepped(vec![
            DeltaStep {
                ticks: 1,
                xa: 1,
                ya: 2,
                za: 3,
            },
            DeltaStep {
                ticks: 130,
                xa: -1,
                ya: 0,
                za: 0,
            },
        ]),
        on_ground: false,
    };
    assert_eq!(
        rt!(stepped),
        [
            5, 4, 1, 0, 1, 0, 2, 0, 3, 0x82, 0x01, 0xff, 0xff, 0, 0, 0, 0
        ]
    );
}

#[test]
fn move_entity_pos_rot_and_rot() {
    let packet = MoveEntityPosRot {
        entity_id: 1,
        delta: VecDelta::Linear {
            xa: 0,
            ya: 0,
            za: 0,
        },
        y_rot: Angle(64),
        x_rot: Angle(32),
        on_ground: false,
    };
    assert_eq!(rt!(packet), [1, 0, 0, 0, 0, 0, 0, 0, 64, 32]);
    let rot = MoveEntityRot {
        entity_id: 1,
        on_ground: true,
        y_rot: Angle(64),
        x_rot: Angle(32),
    };
    assert_eq!(rt!(rot), [1, 1, 64, 32]);
}

#[test]
fn stepped_delta_rejects_hostile_count() {
    assert!(from_bytes_exact::<MoveEntityPos>(&[1, 0xfe, 0xff, 0x7f, 0, 0]).is_err());
}

#[test]
fn minecart_vehicle_and_position_sync() {
    rt!(MoveMinecartAlongTrack {
        entity_id: 9,
        steps: vec![MinecartStep {
            position: v(1.0, 2.0, 3.0),
            movement: v(0.0, 0.0, 0.5),
            y_rot: Angle(10),
            x_rot: Angle(20),
            weight: 1.5,
        }],
    });
    let bytes = rt!(MoveVehicle {
        position: v(1.0, 2.0, 3.0),
        y_rot: 90.0,
        x_rot: -10.0,
    });
    assert_eq!(bytes.len(), 32);
    let linear = EntityPositionSync {
        entity_id: 2,
        position: PositionPath::Linear(v(1.0, 2.0, 3.0)),
        y_rot: 1.0,
        x_rot: 2.0,
        on_ground: true,
    };
    let bytes = rt!(linear);
    assert_eq!(&bytes[..2], [2, 0]);
    let stepped = EntityPositionSync {
        entity_id: 2,
        position: PositionPath::Stepped(vec![PositionStep {
            position: v(4.0, 5.0, 6.0),
            tick_offset: 3,
        }]),
        y_rot: 1.0,
        x_rot: 2.0,
        on_ground: false,
    };
    assert_eq!(&rt!(stepped.clone())[..3], [2, 1, 1]);
    assert_eq!(stepped.position.end_position(), Some(v(4.0, 5.0, 6.0)));
}

#[test]
fn teleport_entity() {
    let packet = TeleportEntity {
        entity_id: 3,
        change: PositionMoveRotation {
            position: v(1.0, 2.0, 3.0),
            delta_movement: v(0.0, 0.0, 0.0),
            y_rot: 45.0,
            x_rot: 0.0,
        },
        relatives: Relatives(Relatives::X | Relatives::ROTATE_DELTA),
        on_ground: true,
    };
    let bytes = rt!(packet);
    assert_eq!(bytes.len(), 1 + 48 + 8 + 4 + 1);
    assert_eq!(&bytes[bytes.len() - 5..], [0, 0, 1, 1, 1]);
    assert!(Relatives(0b11).contains(Relatives::Y));
}

#[test]
fn damage_event_optional_ids() {
    let none = DamageEvent {
        entity_id: 1,
        source_type: 3,
        source_cause_id: -1,
        source_direct_id: 7,
        source_position: None,
    };
    assert_eq!(rt!(none), [1, 3, 0, 8, 0]);
    rt!(DamageEvent {
        entity_id: 1,
        source_type: 3,
        source_cause_id: 4,
        source_direct_id: -1,
        source_position: Some(v(1.0, 2.0, 3.0)),
    });
}

#[test]
fn small_packets() {
    assert_eq!(
        rt!(Animate {
            entity_id: 4,
            action: 2
        }),
        [4, 2]
    );
    assert_eq!(
        rt!(EntityEvent {
            entity_id: 1,
            event_id: -2
        }),
        [0, 0, 0, 1, 0xfe]
    );
    assert_eq!(
        rt!(HurtAnimation {
            entity_id: 1,
            yaw: 0.0
        }),
        [1, 0, 0, 0, 0]
    );
    assert_eq!(
        rt!(RemoveEntities {
            entity_ids: vec![1, 128]
        }),
        [2, 1, 0x80, 0x01]
    );
    assert_eq!(
        rt!(RemoveMobEffect {
            entity_id: 1,
            effect: 2
        }),
        [1, 2]
    );
    assert_eq!(
        rt!(RotateHead {
            entity_id: 1,
            y_head_rot: Angle(9)
        }),
        [1, 9]
    );
    assert_eq!(
        rt!(SetEntityLink {
            source_id: 1,
            dest_id: -1
        }),
        [0, 0, 0, 1, 0xff, 0xff, 0xff, 0xff]
    );
    assert_eq!(
        rt!(SetEntityMotion {
            entity_id: 1,
            movement: LpVec3::default()
        }),
        [1, 0]
    );
    assert_eq!(
        rt!(SetPassengers {
            vehicle: 10,
            passengers: vec![1, 2, 3]
        }),
        [10, 3, 1, 2, 3]
    );
    assert_eq!(
        rt!(TakeItemEntity {
            item_id: 1,
            player_id: 2,
            amount: 3
        }),
        [1, 2, 3]
    );
    assert_eq!(
        rt!(ProjectilePower {
            entity_id: 1,
            acceleration_power: 1.0
        })[0],
        1
    );
    assert_eq!(
        rt!(UpdateMobEffect {
            entity_id: 1,
            effect: 2,
            amplifier: 3,
            duration_ticks: 300,
            flags: UpdateMobEffect::FLAG_VISIBLE | UpdateMobEffect::FLAG_SHOW_ICON,
        }),
        [1, 2, 3, 0xac, 0x02, 6]
    );
}

#[test]
fn update_attributes() {
    let packet = UpdateAttributes {
        entity_id: 1,
        attributes: LengthPrefixed(vec![AttributeSnapshot {
            attribute: 4,
            base: 20.0,
            modifiers: vec![AttributeModifier {
                id: Identifier::parse("a:b").unwrap(),
                amount: 0.5,
                operation: AttributeOperation::AddMultipliedTotal,
            }],
        }]),
    };
    let bytes = rt!(packet);
    assert_eq!(&bytes[..3], [1, 1, 4]);
    assert_eq!(&bytes[3..11], 20.0f64.to_be_bytes());
    assert_eq!(&bytes[11..13], [1, 3]);
    assert_eq!(*bytes.last().unwrap(), 2);
}

#[test]
fn waypoints() {
    let id = Uuid::from_u128(7);
    let icon = WaypointIcon {
        style: Identifier::parse("default").unwrap(),
        color: Some([1, 2, 3]),
    };
    let packet = Waypoint {
        operation: WaypointOperation::Update,
        waypoint: TrackedWaypoint {
            id: WaypointId::Uuid(id),
            icon: icon.clone(),
            data: WaypointData::Position { x: 1, y: -1, z: 2 },
        },
    };
    let bytes = rt!(packet);
    assert_eq!(&bytes[..2], [2, 1]);
    assert_eq!(&bytes[2..18], 7u128.to_be_bytes());
    let tail = &bytes[18 + 1 + "minecraft:default".len()..];
    assert_eq!(tail, [1, 1, 2, 3, 1, 1, 0xff, 0xff, 0xff, 0xff, 0x0f, 2]);

    for data in [
        WaypointData::Empty,
        WaypointData::Chunk { x: -4, z: 4 },
        WaypointData::Azimuth { angle: 1.5 },
    ] {
        rt!(Waypoint {
            operation: WaypointOperation::Track,
            waypoint: TrackedWaypoint {
                id: WaypointId::Name("n".into()),
                icon: WaypointIcon {
                    style: icon.style.clone(),
                    color: None,
                },
                data,
            },
        });
    }
}

#[test]
fn rejects_bad_enums() {
    assert!(from_bytes_exact::<Waypoint>(&[3]).is_err());
    assert!(from_bytes_exact::<SetEquipment>(&[1, 9]).is_err());
}

#[test]
fn set_entity_data() {
    rt!(SetEntityData {
        entity_id: 1,
        values: EntityDataValues::default()
    });
}

#[test]
#[ignore = "needs ItemStack"]
fn set_equipment() {
    rt!(SetEquipment {
        entity_id: 1,
        slots: vec![
            EquipmentEntry {
                slot: EquipmentSlot::MainHand,
                item: ItemStack::default()
            },
            EquipmentEntry {
                slot: EquipmentSlot::Saddle,
                item: ItemStack::default()
            },
        ],
    });
}
