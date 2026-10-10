/// Block entities that have a client renderer in 26.3; everything else is drawn by terrain or not at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockEntityKind {
    Chest,
    TrappedChest,
    EnderChest,
    Sign,
    HangingSign,
    Skull,
    Banner,
    ShulkerBox,
    Bell,
    Lectern,
    EnchantingTable,
    Campfire,
    DecoratedPot,
    Conduit,
    Beacon,
    EndPortal,
    EndGateway,
    Spawner,
    TrialSpawner,
    Vault,
    Brushable,
    Piston,
    Shelf,
    CopperGolemStatue,
}

impl BlockEntityKind {
    /// Registry name of a `block_entity_type` entry (`minecraft:` prefix optional).
    pub fn from_type_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Some(match name {
            "chest" => Self::Chest,
            "trapped_chest" => Self::TrappedChest,
            "ender_chest" => Self::EnderChest,
            "sign" => Self::Sign,
            "hanging_sign" => Self::HangingSign,
            "skull" => Self::Skull,
            "banner" => Self::Banner,
            "shulker_box" => Self::ShulkerBox,
            "bell" => Self::Bell,
            "lectern" => Self::Lectern,
            "enchanting_table" => Self::EnchantingTable,
            "campfire" => Self::Campfire,
            "decorated_pot" => Self::DecoratedPot,
            "conduit" => Self::Conduit,
            "beacon" => Self::Beacon,
            "end_portal" => Self::EndPortal,
            "end_gateway" => Self::EndGateway,
            "mob_spawner" | "spawner" => Self::Spawner,
            "trial_spawner" => Self::TrialSpawner,
            "vault" => Self::Vault,
            "brushable_block" => Self::Brushable,
            "piston" => Self::Piston,
            "shelf" => Self::Shelf,
            "copper_golem_statue" => Self::CopperGolemStatue,
            _ => return None,
        })
    }

    /// Derives the kind from a block's registry name; usable when the type registry is unavailable.
    pub fn from_block_name(name: &str) -> Option<Self> {
        let name = name.strip_prefix("minecraft:").unwrap_or(name);
        Some(match name {
            "chest" => Self::Chest,
            "trapped_chest" => Self::TrappedChest,
            "ender_chest" => Self::EnderChest,
            "bell" => Self::Bell,
            "lectern" => Self::Lectern,
            "enchanting_table" => Self::EnchantingTable,
            "campfire" | "soul_campfire" => Self::Campfire,
            "decorated_pot" => Self::DecoratedPot,
            "conduit" => Self::Conduit,
            "beacon" => Self::Beacon,
            "end_portal" => Self::EndPortal,
            "end_gateway" => Self::EndGateway,
            "spawner" => Self::Spawner,
            "trial_spawner" => Self::TrialSpawner,
            "vault" => Self::Vault,
            "suspicious_sand" | "suspicious_gravel" => Self::Brushable,
            "moving_piston" => Self::Piston,
            n if n.ends_with("copper_chest") => Self::Chest,
            n if n.ends_with("_hanging_sign") => Self::HangingSign,
            n if n.ends_with("_sign") => Self::Sign,
            n if n.ends_with("_banner") => Self::Banner,
            n if n.ends_with("_skull") || n.ends_with("_head") => Self::Skull,
            n if n.ends_with("shulker_box") => Self::ShulkerBox,
            n if n.ends_with("_shelf") => Self::Shelf,
            n if n.ends_with("copper_golem_statue") => Self::CopperGolemStatue,
            _ => return None,
        })
    }

    /// Kinds whose renderer cares about game time even without NBT changes.
    pub fn animated(self) -> bool {
        matches!(
            self,
            Self::Banner
                | Self::Bell
                | Self::EnchantingTable
                | Self::Conduit
                | Self::Beacon
                | Self::EndPortal
                | Self::EndGateway
                | Self::Spawner
                | Self::TrialSpawner
                | Self::Vault
                | Self::Skull
        )
    }

    /// Distance in blocks beyond which the kind is not drawn (vanilla `getViewDistance`).
    pub fn view_distance(self, render_distance_blocks: f64) -> f64 {
        match self {
            Self::Beacon => render_distance_blocks,
            Self::EndGateway => 256.0,
            Self::Piston => 68.0,
            _ => 64.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chest_family_resolves() {
        assert_eq!(
            BlockEntityKind::from_type_name("minecraft:chest"),
            Some(BlockEntityKind::Chest)
        );
        assert_eq!(
            BlockEntityKind::from_type_name("minecraft:trapped_chest"),
            Some(BlockEntityKind::TrappedChest)
        );
        assert_eq!(
            BlockEntityKind::from_type_name("ender_chest"),
            Some(BlockEntityKind::EnderChest)
        );
        assert_eq!(BlockEntityKind::from_type_name("minecraft:furnace"), None);
    }

    #[test]
    fn block_names_resolve() {
        for (name, kind) in [
            ("minecraft:oak_wall_sign", BlockEntityKind::Sign),
            ("minecraft:oak_hanging_sign", BlockEntityKind::HangingSign),
            ("minecraft:red_wall_banner", BlockEntityKind::Banner),
            (
                "minecraft:wither_skeleton_wall_skull",
                BlockEntityKind::Skull,
            ),
            ("minecraft:player_head", BlockEntityKind::Skull),
            ("minecraft:cyan_shulker_box", BlockEntityKind::ShulkerBox),
            (
                "minecraft:waxed_oxidized_copper_chest",
                BlockEntityKind::Chest,
            ),
            ("minecraft:oak_shelf", BlockEntityKind::Shelf),
        ] {
            assert_eq!(BlockEntityKind::from_block_name(name), Some(kind), "{name}");
        }
        assert_eq!(BlockEntityKind::from_block_name("minecraft:stone"), None);
    }
}
