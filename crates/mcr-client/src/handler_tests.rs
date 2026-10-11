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
