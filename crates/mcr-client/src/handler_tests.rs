use std::time::Instant;

use mcr_buf::Identifier;
use mcr_nbt::{Compound, NetworkNbt, Value};
use mcr_protocol::configuration::RegistryData;
use mcr_protocol::dispatch::PlayClientbound as P;
use mcr_protocol::play::clientbound::entity::{DamageEvent, TakeItemEntity};
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::entity_data::EntityDataValue;
use mcr_protocol::types::registry::RegistryEntry;
use uuid::Uuid;

use crate::config::ClientConfig;
use crate::entity::Entity;
use crate::events::{ClientEvent, DamageEffects};
use crate::pickup::PICKUP_TICKS;
use crate::state::{ClientState, Outbox};

const PLAYER: i32 = 7;
const ITEM: i32 = 40;

fn state() -> ClientState {
    let mut s = ClientState::new(&ClientConfig::default(), Uuid::nil());
    s.player.entity_id = PLAYER;
    s
}

fn handle(s: &mut ClientState, packet: P<'_>) -> Vec<ClientEvent> {
    let mut out = Outbox::default();
    s.handle_play(packet, &mut out, Instant::now()).unwrap();
    out.events.into()
}

fn damage_types(s: &mut ClientState, entries: &[(&str, Option<&str>)]) {
    let entries = entries
        .iter()
        .map(|(id, effects)| RegistryEntry {
            id: Identifier::parse(id).unwrap(),
            data: effects.map(|e| {
                let mut c = Compound::new();
                c.insert("effects", e);
                NetworkNbt(Value::Compound(c))
            }),
        })
        .collect();
    s.registries.apply_registry_data(&RegistryData {
        registry: Identifier::parse("minecraft:damage_type").unwrap(),
        entries,
    });
}

fn damage(s: &mut ClientState, entity_id: i32, source_type: i32) -> Option<DamageEffects> {
    let packet = P::DamageEvent(DamageEvent {
        entity_id,
        source_type,
        source_cause_id: -1,
        source_direct_id: -1,
        source_position: None,
    });
    handle(s, packet).into_iter().find_map(|e| match e {
        ClientEvent::EntityDamaged {
            entity_id: id,
            effects,
            ..
        } if id == entity_id => Some(effects),
        _ => None,
    })
}

#[test]
fn damage_events_pick_the_hurt_sound_of_their_damage_type() {
    let mut s = state();
    damage_types(
        &mut s,
        &[
            ("minecraft:generic", None),
            ("minecraft:lava", None),
            ("minecraft:in_fire", None),
            ("minecraft:on_fire", None),
            ("minecraft:drown", None),
            ("minecraft:sweet_berry_bush", None),
            ("minecraft:freeze", None),
            ("minecraft:fall", None),
            ("custom:frostbite", Some("freezing")),
        ],
    );
    let expected = [
        "entity.player.hurt",
        "entity.player.hurt_on_fire",
        "entity.player.hurt_on_fire",
        "entity.player.hurt_on_fire",
        "entity.player.hurt_drown",
        "entity.player.hurt_sweet_berry_bush",
        "entity.player.hurt_freeze",
        "entity.player.hurt",
        "entity.player.hurt_freeze",
    ];
    for (id, sound) in expected.iter().enumerate() {
        let effects = damage(&mut s, PLAYER, id as i32).unwrap();
        assert_eq!(effects.player_hurt_sound(), *sound, "damage type {id}");
    }
    assert_eq!(s.player.damage_cooldown, 20);
    assert_eq!(damage(&mut s, PLAYER, 999), Some(DamageEffects::Hurt));
}

fn dropped_item(s: &mut ClientState, count: i32) {
    let kind = s.data.entity_type_by_name("minecraft:item").unwrap() as i32;
    let mut e = Entity::new(ITEM, Uuid::nil(), kind, [3.0, 64.0, 0.0]);
    let stone = s.data.item_by_name("minecraft:stone").unwrap() as i32;
    e.data
        .insert(8, EntityDataValue::ItemStack(ItemStack::new(stone, count)));
    assert!(s.entities.insert(e));
}

fn take(s: &mut ClientState, amount: i32) -> Vec<ClientEvent> {
    handle(
        s,
        P::TakeItemEntity(TakeItemEntity {
            item_id: ITEM,
            player_id: PLAYER,
            amount,
        }),
    )
}

#[test]
fn taken_item_flies_to_the_collector_and_is_removed() {
    let mut s = state();
    s.player
        .physics
        .snap_to([0.0, 64.0, 0.0], [0.0; 3], 0.0, 0.0);
    dropped_item(&mut s, 1);
    let events = take(&mut s, 1);
    assert!(events.contains(&ClientEvent::EntityRemoved(ITEM)));
    assert!(s.entities.get(ITEM).is_none());
    assert_eq!(s.pickups.len(), 1);

    let target_y = 64.0 + f64::from(s.player.physics.eye_height()) / 2.0;
    let near = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
    assert!(near(s.pickups[0].position(0.0), [3.0, 64.0, 0.0]));
    for tick in 1..PICKUP_TICKS {
        s.tick();
        let t = (f64::from(tick) / f64::from(PICKUP_TICKS)).powi(2);
        let want = [3.0 - 3.0 * t, 64.0 + (target_y - 64.0) * t, 0.0];
        assert!(near(s.pickups[0].position(0.0), want), "tick {tick}");
    }
    let end = s.pickups[0].position(1.0);
    assert!(near(end, [0.0, target_y, 0.0]));
    s.tick();
    assert!(s.pickups.is_empty());
}

#[test]
fn partly_taken_stack_stays_with_the_rest() {
    let mut s = state();
    dropped_item(&mut s, 5);
    let events = take(&mut s, 2);
    assert!(!events.contains(&ClientEvent::EntityRemoved(ITEM)));
    let left = match s.entities.get(ITEM).and_then(|e| e.data.get(&8)) {
        Some(EntityDataValue::ItemStack(stack)) => stack.count,
        _ => 0,
    };
    assert_eq!(left, 3);
    assert_eq!(s.pickups.len(), 1);
}

#[test]
fn win_game_event_shows_the_credits() {
    use mcr_protocol::play::clientbound::world::GameEvent;
    let mut s = state();
    let events = handle(
        &mut s,
        P::GameEvent(GameEvent {
            event: GameEvent::WIN_GAME,
            param: 1.0,
        }),
    );
    assert!(matches!(events.as_slice(), [ClientEvent::ShowCredits]));
}

fn respawn(s: &ClientState, data_to_keep: u8) -> P<'static> {
    use mcr_protocol::play::clientbound::player::{CommonPlayerSpawnInfo, Respawn};
    use mcr_protocol::types::game::GameType;
    P::Respawn(Respawn {
        spawn_info: CommonPlayerSpawnInfo {
            dimension_type: 0,
            dimension: s.dimension.name.clone(),
            seed: 0,
            game_type: GameType::Survival,
            previous_game_type: None,
            is_debug: false,
            is_flat: false,
            last_death_location: None,
            portal_cooldown: 0,
            sea_level: 63,
        },
        data_to_keep,
    })
}

#[test]
fn immediate_respawn_survives_every_respawn() {
    use mcr_protocol::play::clientbound::world::GameEvent;
    let mut s = state();
    handle(
        &mut s,
        P::GameEvent(GameEvent {
            event: 11,
            param: 1.0,
        }),
    );
    for _ in 0..3 {
        let packet = respawn(&s, 0);
        handle(&mut s, packet);
        assert!(!s.player.show_death_screen);
    }
}

#[test]
fn respawn_with_kept_attributes_keeps_their_modifiers() {
    use mcr_protocol::play::clientbound::player::Respawn;
    use mcr_protocol::types::component::{AttributeModifier, AttributeOperation};
    let mut s = state();
    let speed = s
        .data
        .registry("minecraft:attribute")
        .and_then(|r| r.id_of("minecraft:movement_speed"))
        .unwrap() as i32;
    s.player.attributes.insert(
        speed,
        crate::entity::AttributeState {
            base: 0.1,
            modifiers: vec![AttributeModifier {
                id: Identifier::parse("minecraft:effect.speed").unwrap(),
                amount: 1.0,
                operation: AttributeOperation::AddMultipliedTotal,
            }],
            range: None,
        },
    );
    let packet = respawn(&s, Respawn::KEEP_ATTRIBUTE_MODIFIERS);
    handle(&mut s, packet);
    assert!((s.player.physics.attributes.movement_speed - 0.2).abs() < 1e-9);
    let packet = respawn(&s, 0);
    handle(&mut s, packet);
    assert!((s.player.physics.attributes.movement_speed - 0.1).abs() < 1e-9);
}

#[test]
fn movement_effects_attributes_and_fast_lava_reach_physics() {
    use mcr_protocol::play::clientbound::entity::{
        AttributeSnapshot, UpdateAttributes, UpdateMobEffect,
    };
    let mut s = state();
    let effect = |s: &ClientState, name: &str| {
        s.data
            .registry("minecraft:mob_effect")
            .and_then(|r| r.id_of(name))
            .unwrap() as i32
    };
    for name in [
        "minecraft:blindness",
        "minecraft:dolphins_grace",
        "minecraft:weaving",
    ] {
        let id = effect(&s, name);
        handle(
            &mut s,
            P::UpdateMobEffect(UpdateMobEffect {
                entity_id: PLAYER,
                effect: id,
                amplifier: 0,
                duration_ticks: 100,
                flags: 0,
            }),
        );
    }
    let e = s.player.physics.effects;
    assert!(e.blindness && e.dolphins_grace && e.weaving);

    let attr = |name: &str| {
        s.data
            .registry("minecraft:attribute")
            .and_then(|r| r.id_of(name))
            .unwrap() as i32
    };
    let snapshots = [
        (attr("minecraft:friction_modifier"), 0.5),
        (attr("minecraft:air_drag_modifier"), f64::NAN),
    ]
    .map(|(attribute, base)| AttributeSnapshot {
        attribute,
        base,
        modifiers: Vec::new(),
    });
    handle(
        &mut s,
        P::UpdateAttributes(UpdateAttributes {
            entity_id: PLAYER,
            attributes: mcr_buf::LengthPrefixed(snapshots.to_vec()),
        }),
    );
    let a = s.player.physics.attributes;
    assert_eq!(a.friction_modifier, 0.5);
    assert!(a.air_drag_modifier.is_finite(), "NaN is sanitised");

    let mut nether = Compound::new();
    let mut attributes = Compound::new();
    attributes.insert("minecraft:gameplay/fast_lava", 1i8);
    nether.insert("attributes", attributes);
    s.registries.apply_registry_data(&RegistryData {
        registry: Identifier::parse("minecraft:dimension_type").unwrap(),
        entries: vec![
            RegistryEntry {
                id: Identifier::parse("minecraft:overworld").unwrap(),
                data: Some(NetworkNbt(Value::Compound(Compound::new()))),
            },
            RegistryEntry {
                id: Identifier::parse("minecraft:custom").unwrap(),
                data: Some(NetworkNbt(Value::Compound(nether))),
            },
            RegistryEntry {
                id: Identifier::parse("minecraft:the_nether").unwrap(),
                data: None,
            },
        ],
    });
    for (type_id, fast) in [(0, false), (1, true), (2, true)] {
        s.dimension.type_id = type_id;
        assert_eq!(s.fast_lava(), fast, "dimension type {type_id}");
    }
}
