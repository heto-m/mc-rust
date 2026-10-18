//! Mounts with a custom inventory screen: horses, llamas, camels and nautiluses.

use crate::state::ClientState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountFamily {
    Horse,
    Llama,
    Nautilus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountScreen {
    pub family: MountFamily,
    pub columns: usize,
    pub saddle: bool,
    pub armor: bool,
}

fn family_of(name: &str) -> Option<MountFamily> {
    Some(match name {
        "horse" | "donkey" | "mule" | "skeleton_horse" | "zombie_horse" | "camel"
        | "camel_husk" => MountFamily::Horse,
        "llama" | "trader_llama" => MountFamily::Llama,
        "nautilus" | "zombie_nautilus" => MountFamily::Nautilus,
        _ => return None,
    })
}

impl ClientState {
    pub fn entity_type_short_name(&self, entity_type: i32) -> Option<&str> {
        let name = self
            .data
            .registry("minecraft:entity_type")?
            .name_of(u32::try_from(entity_type).ok()?)?;
        Some(name.strip_prefix("minecraft:").unwrap_or(name))
    }

    fn in_entity_tag(&self, tag: &str, entity_type: i32, default: bool) -> bool {
        match self.registries.tag("minecraft:entity_type", tag) {
            Some(ids) => ids.contains(&entity_type),
            None => default,
        }
    }

    /// Vanilla `isServerControlledInventory`: riding a mount whose inventory the server owns.
    pub fn riding_custom_inventory(&self) -> bool {
        self.player
            .vehicle
            .and_then(|id| self.entities.get(id))
            .and_then(|e| self.entity_type_short_name(e.kind))
            .is_some_and(|n| family_of(n).is_some())
    }

    /// The screen a `mount_screen_open` packet asks for, from the open container's mount info.
    pub fn mount_screen(&self) -> Option<MountScreen> {
        let info = self.inventory.open.as_ref()?.mount?;
        let name = self.entity_type_short_name(info.entity_type)?;
        let family = family_of(name)?;
        let (saddle, armor) = match family {
            MountFamily::Nautilus => (true, true),
            MountFamily::Llama => (false, true),
            MountFamily::Horse => (
                self.in_entity_tag("minecraft:can_equip_saddle", info.entity_type, true),
                self.in_entity_tag(
                    "minecraft:can_wear_horse_armor",
                    info.entity_type,
                    name == "horse",
                ),
            ),
        };
        Some(MountScreen {
            family,
            columns: info.columns,
            saddle,
            armor,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use mcr_protocol::dispatch::PlayClientbound as P;
    use mcr_protocol::play::clientbound::ui::MountScreenOpen;
    use uuid::Uuid;

    use super::*;
    use crate::config::ClientConfig;
    use crate::entity::Entity;
    use crate::state::Outbox;

    fn state_with(entity: &str) -> ClientState {
        let mut s = ClientState::new(&ClientConfig::default(), Uuid::nil());
        let kind = s.data.entity_type_by_name(entity).unwrap() as i32;
        assert!(
            s.entities
                .insert(Entity::new(9, Uuid::nil(), kind, [0.0, 64.0, 0.0]))
        );
        s
    }

    fn open(s: &mut ClientState, columns: i32) {
        let mut out = Outbox::default();
        s.handle_play(
            P::MountScreenOpen(MountScreenOpen {
                container_id: 3,
                inventory_columns: columns,
                entity_id: 9,
            }),
            &mut out,
            Instant::now(),
        )
        .unwrap();
    }

    #[test]
    fn mount_open_builds_the_horse_screen() {
        let mut s = state_with("minecraft:horse");
        open(&mut s, 0);
        let screen = s.mount_screen().unwrap();
        assert_eq!(screen.family, MountFamily::Horse);
        assert_eq!(screen.columns, 0);
        assert!(screen.saddle && screen.armor);
    }

    #[test]
    fn mount_open_builds_llama_and_nautilus_screens() {
        let mut s = state_with("minecraft:llama");
        open(&mut s, 3);
        let llama = s.mount_screen().unwrap();
        assert_eq!((llama.family, llama.columns), (MountFamily::Llama, 3));
        assert!(!llama.saddle && llama.armor);

        let mut s = state_with("minecraft:nautilus");
        open(&mut s, 0);
        assert_eq!(s.mount_screen().unwrap().family, MountFamily::Nautilus);
    }

    #[test]
    fn mount_open_for_a_cow_yields_no_screen() {
        let mut s = state_with("minecraft:cow");
        open(&mut s, 0);
        assert_eq!(s.mount_screen(), None);
    }

    #[test]
    fn riding_a_horse_routes_the_inventory_key_to_the_server() {
        let mut s = state_with("minecraft:donkey");
        assert!(!s.riding_custom_inventory());
        s.player.vehicle = Some(9);
        assert!(s.riding_custom_inventory());
        let mut s = state_with("minecraft:oak_boat");
        s.player.vehicle = Some(9);
        assert!(!s.riding_custom_inventory());
    }
}
