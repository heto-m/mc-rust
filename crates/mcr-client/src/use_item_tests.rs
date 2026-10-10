use mcr_buf::IdOr;
use mcr_protocol::types::ItemStack;
use mcr_protocol::types::component::{
    Consumable, DataComponent, DataComponentPatch, ItemUseAnimation,
};
use uuid::Uuid;

use crate::config::ClientConfig;
use crate::state::{ClientState, Cooldown};
use crate::use_item::{UseKind, seconds_to_ticks};

fn stack_with_consume(seconds: f32) -> (ClientState, ItemStack) {
    let s = ClientState::new(&ClientConfig::default(), Uuid::nil());
    let item = s.data.item_by_name("minecraft:potion").unwrap() as i32;
    let mut stack = ItemStack::new(item, 1);
    stack.components = DataComponentPatch {
        added: vec![DataComponent::Consumable(Consumable {
            consume_seconds: seconds,
            animation: ItemUseAnimation::Drink,
            sound: IdOr::Id(0),
            has_consume_particles: true,
            on_consume_effects: vec![],
        })],
        removed: vec![],
    };
    (s, stack)
}

#[test]
fn hostile_consume_durations_are_clamped() {
    for secs in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        -5.0,
        0.0,
        1.6,
        f32::MAX,
        1.0e30,
    ] {
        let ticks = seconds_to_ticks(secs);
        assert!((0..=72_000).contains(&ticks), "{secs} -> {ticks}");
        let (s, stack) = stack_with_consume(secs);
        let (kind, ticks) = s.use_kind(&stack).unwrap();
        assert_eq!(kind, UseKind::Consume);
        assert!((0..=72_000).contains(&ticks));
    }
    assert_eq!(seconds_to_ticks(1.6), 32);
}

#[test]
fn cooldown_state_survives_hostile_durations() {
    let (mut s, stack) = stack_with_consume(1.0);
    let name = s.data.item_name(stack.item as u32).unwrap().to_owned();
    s.cooldowns.insert(
        name,
        Cooldown {
            ends_at: u64::MAX,
            total: i32::MIN,
        },
    );
    assert!(s.is_on_cooldown(&stack));
    assert!(s.cooldown_progress(&stack).unwrap().is_finite());
    assert!(s.use_kind(&stack).is_none());
}
