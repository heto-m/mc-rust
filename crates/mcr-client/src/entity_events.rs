//! The state side of `entity_event` packets: what vanilla's `handleEntityEvent` changes besides effects.

use mcr_protocol::types::entity_data::EntityDataValue;

use crate::state::ClientState;

pub const EVENT_FISHING_ROD_REEL_IN: i8 = 31;
pub const EVENT_REDUCED_DEBUG_INFO: i8 = 22;
pub const EVENT_FULL_DEBUG_INFO: i8 = 23;
pub const EVENT_SWAP_HANDS: i8 = 55;
pub const EVENT_RAVAGER_ROARED: i8 = 69;
pub const EVENT_TNT_PRIME: i8 = 70;
pub const EVENT_BUBBLE_DOWN: i8 = 71;
pub const EVENT_BUBBLE_UP: i8 = 72;

const HOOKED_ENTITY: u8 = 8;
const MAINHAND: usize = 0;
const OFFHAND: usize = 5;
const ROAR_RANGE: f64 = 4.0;

impl ClientState {
    pub(crate) fn apply_entity_event(&mut self, entity_id: i32, event: i8) {
        let local = entity_id == self.player.entity_id;
        match event {
            EVENT_REDUCED_DEBUG_INFO | EVENT_FULL_DEBUG_INFO if local => {
                self.server.reduced_debug_info = event == EVENT_REDUCED_DEBUG_INFO;
            }
            EVENT_SWAP_HANDS if !local => {
                if let Some(e) = self.entities.get_mut(entity_id) {
                    e.equipment.swap(MAINHAND, OFFHAND);
                }
            }
            EVENT_RAVAGER_ROARED => self.ravager_roar(entity_id),
            EVENT_TNT_PRIME => self.prime_minecart(entity_id),
            EVENT_FISHING_ROD_REEL_IN => self.fishing_pull(entity_id),
            EVENT_BUBBLE_DOWN | EVENT_BUBBLE_UP if self.controlled_vehicle == Some(entity_id) => {
                self.vehicle_impulse.bubble_down = Some(event == EVENT_BUBBLE_DOWN);
            }
            _ => {}
        }
    }

    fn type_name(&self, kind: i32) -> Option<&'static str> {
        mcr_data::v26_3().entity_type_name(u32::try_from(kind).ok()?)
    }

    fn prime_minecart(&mut self, id: i32) {
        let Some(kind) = self.entities.get(id).map(|e| e.kind) else {
            return;
        };
        if self.type_name(kind) == Some("minecraft:tnt_minecart")
            && let Some(e) = self.entities.get_mut(id)
        {
            e.fuse = crate::entity::MINECART_TNT_FUSE;
        }
    }

    /// Vanilla knocks back the entities the client simulates itself, which is only the local player here.
    fn ravager_roar(&mut self, ravager: i32) {
        let Some(e) = self.entities.get(ravager) else {
            return;
        };
        if self.type_name(e.kind) != Some("minecraft:ravager") || self.player.dead {
            return;
        }
        let (w, h) = mcr_data::v26_3()
            .entity_type_info(e.kind as u32)
            .map_or((1.95, 2.2), |i| (f64::from(i.width), f64::from(i.height)));
        let min = [
            e.pos[0] - w / 2.0 - ROAR_RANGE,
            e.pos[1] - ROAR_RANGE,
            e.pos[2] - w / 2.0 - ROAR_RANGE,
        ];
        let max = [
            e.pos[0] + w / 2.0 + ROAR_RANGE,
            e.pos[1] + h + ROAR_RANGE,
            e.pos[2] + w / 2.0 + ROAR_RANGE,
        ];
        let b = self.player.physics.bounding_box();
        if (0..3).any(|i| b.max[i] <= min[i] || b.min[i] >= max[i]) {
            return;
        }
        let p = self.player.physics.pos;
        let (dx, dz) = (p[0] - e.pos[0], p[2] - e.pos[2]);
        let dd = (dx * dx + dz * dz).max(0.001);
        let vel = &mut self.player.physics.vel;
        vel[0] += dx / dd * 4.0;
        vel[1] += 0.2;
        vel[2] += dz / dd * 4.0;
    }

    /// A reeled-in rod pulls its hooked entity toward the angler; the client moves only itself.
    fn fishing_pull(&mut self, hook: i32) {
        let Some(h) = self.entities.get(hook) else {
            return;
        };
        if self.type_name(h.kind) != Some("minecraft:fishing_bobber") {
            return;
        }
        let hooked = match h.data.get(&HOOKED_ENTITY) {
            Some(EntityDataValue::Int(id)) if *id > 0 => *id - 1,
            _ => return,
        };
        if hooked != self.player.entity_id {
            return;
        }
        let owner = h.object_data;
        let owner_pos = if owner == self.player.entity_id {
            Some(self.player.physics.pos)
        } else {
            self.entities.get(owner).map(|o| o.pos)
        };
        let Some(o) = owner_pos else { return };
        let vel = &mut self.player.physics.vel;
        for i in 0..3 {
            vel[i] += (o[i] - h.pos[i]) * 0.1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use mcr_protocol::dispatch::PlayClientbound as P;
    use mcr_protocol::play::clientbound::entity::EntityEvent;
    use mcr_protocol::types::ItemStack;
    use uuid::Uuid;

    use super::*;
    use crate::config::ClientConfig;
    use crate::entity::Entity;
    use crate::state::Outbox;

    const PLAYER: i32 = 7;

    fn state() -> ClientState {
        let mut s = ClientState::new(&ClientConfig::default(), Uuid::nil());
        s.player.entity_id = PLAYER;
        s
    }

    fn kind(name: &str) -> i32 {
        mcr_data::v26_3().entity_type_by_name(name).unwrap() as i32
    }

    fn event(s: &mut ClientState, entity_id: i32, event_id: i8) {
        let mut out = Outbox::default();
        let packet = P::EntityEvent(EntityEvent {
            entity_id,
            event_id,
        });
        s.handle_play(packet, &mut out, Instant::now()).unwrap();
    }

    #[test]
    fn swap_hands_swaps_another_entitys_hands() {
        let mut s = state();
        let mut e = Entity::new(3, Uuid::nil(), kind("minecraft:zombie"), [0.0; 3]);
        e.equipment[MAINHAND] = ItemStack::new(1, 1);
        s.entities.insert(e);
        event(&mut s, 3, EVENT_SWAP_HANDS);
        let e = s.entities.get(3).unwrap();
        assert!(e.equipment[MAINHAND].is_empty());
        assert_eq!(e.equipment[OFFHAND].item, 1);
    }

    #[test]
    fn ravager_roar_knocks_the_local_player_away_only_in_range() {
        let mut s = state();
        s.player.physics.pos = [2.0, 0.0, 0.0];
        s.entities.insert(Entity::new(
            3,
            Uuid::nil(),
            kind("minecraft:ravager"),
            [0.0; 3],
        ));
        event(&mut s, 3, EVENT_RAVAGER_ROARED);
        let v = s.player.physics.vel;
        assert!((v[0] - 2.0).abs() < 1e-9 && (v[1] - 0.2).abs() < 1e-9 && v[2] == 0.0);
        let mut far = state();
        far.player.physics.pos = [20.0, 0.0, 0.0];
        far.entities.insert(Entity::new(
            3,
            Uuid::nil(),
            kind("minecraft:ravager"),
            [0.0; 3],
        ));
        event(&mut far, 3, EVENT_RAVAGER_ROARED);
        assert_eq!(far.player.physics.vel, [0.0; 3]);
    }

    #[test]
    fn reeling_in_pulls_a_hooked_local_player_toward_the_angler() {
        let mut s = state();
        s.player.physics.pos = [10.0, 0.0, 0.0];
        s.entities.insert(Entity::new(
            2,
            Uuid::nil(),
            kind("minecraft:player"),
            [0.0; 3],
        ));
        let mut hook = Entity::new(3, Uuid::nil(), kind("minecraft:fishing_bobber"), [10.0; 3]);
        hook.object_data = 2;
        hook.data
            .insert(HOOKED_ENTITY, EntityDataValue::Int(PLAYER + 1));
        s.entities.insert(hook);
        event(&mut s, 3, EVENT_FISHING_ROD_REEL_IN);
        assert_eq!(s.player.physics.vel, [-1.0; 3]);
    }

    #[test]
    fn prime_event_lights_only_tnt_minecarts() {
        let mut s = state();
        s.entities.insert(Entity::new(
            3,
            Uuid::nil(),
            kind("minecraft:tnt_minecart"),
            [0.0; 3],
        ));
        s.entities.insert(Entity::new(
            4,
            Uuid::nil(),
            kind("minecraft:minecart"),
            [0.0; 3],
        ));
        event(&mut s, 3, EVENT_TNT_PRIME);
        event(&mut s, 4, EVENT_TNT_PRIME);
        assert_eq!(s.entities.get(3).unwrap().fuse, 80);
        assert_eq!(s.entities.get(4).unwrap().fuse, -1);
        assert_eq!(s.entities.get(3).unwrap().fuse_remaining(0.25), Some(80.75));
        assert_eq!(s.entities.get(4).unwrap().fuse_remaining(0.25), None);
    }

    #[test]
    fn debug_info_events_toggle_reduced_debug() {
        let mut s = state();
        event(&mut s, PLAYER, EVENT_REDUCED_DEBUG_INFO);
        assert!(s.server.reduced_debug_info);
        event(&mut s, PLAYER, EVENT_FULL_DEBUG_INFO);
        assert!(!s.server.reduced_debug_info);
    }
}
