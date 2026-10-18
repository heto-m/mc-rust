use std::sync::OnceLock;

mod generated {
    pub mod v26_3;
}
pub mod game_rules;
pub mod tables;
mod version;

pub use tables::{Aabb, DecodeError};
pub use version::{
    BlockId, BlockInfo, BlockStateInfo, Direction, EntityTypeId, EntityTypeInfo, FluidId,
    FluidStateInfo, ItemId, ItemInfo, ProtocolState, RegistryView, StateId, StateProperties,
    ToolInfo, ToolRuleInfo, Version,
};

/// Vanilla's experimental feature data packs in 26.3 (Experiments on Create New World).
pub const FEATURE_PACKS_26_3: &[&str] = &[
    "minecart_improvements",
    "redstone_experiments",
    "trade_rebalance",
];

pub fn v26_3() -> &'static Version {
    static VERSION: OnceLock<Version> = OnceLock::new();
    VERSION.get_or_init(|| {
        Version::from_blob(generated::v26_3::BLOB).expect("generated v26_3 blob is valid")
    })
}

/// Creative tab contents dumped by `tools/extractor/run-creative.sh`: `tab` header lines followed by
/// `p`/`s` lines (parent-tab and search-tab stacks) carrying required features and network-encoded stacks.
pub fn creative_tabs_26_3() -> &'static str {
    generated::v26_3::CREATIVE_TABS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_default_state_and_air() {
        let v = v26_3();
        let stone = v.block_by_name("minecraft:stone").unwrap();
        let info = v.block_info(stone).unwrap();
        assert_eq!(
            v.state_block_name(info.default_state),
            Some("minecraft:stone")
        );
        assert_eq!(info.default_state, 1);
        let air = v.block_by_name("minecraft:air").unwrap();
        let air_state = v.block_info(air).unwrap().default_state;
        assert!(v.block_state(air_state).unwrap().is_air());
        assert!(!v.block_state(info.default_state).unwrap().is_air());
        assert!(v.collision_shape(air_state).is_empty());
    }

    #[test]
    fn glowstone_emits_light() {
        let v = v26_3();
        let id = v.block_by_name("minecraft:glowstone").unwrap();
        let state = v.block_info(id).unwrap().default_state;
        assert_eq!(v.light_emission(state), 15);
        assert_eq!(v.block_state(state).unwrap().light_emission, 15);
    }

    #[test]
    fn water_dims_sky_light_by_one_per_block() {
        let v = v26_3();
        for name in [
            "minecraft:water",
            "minecraft:kelp_plant",
            "minecraft:seagrass",
        ] {
            let id = v.block_by_name(name).unwrap();
            assert_eq!(
                v.light_block(v.block_info(id).unwrap().default_state),
                1,
                "{name}"
            );
        }
    }

    #[test]
    fn bottom_slab_collision() {
        let v = v26_3();
        let slab = v.block_by_name("minecraft:stone_slab").unwrap();
        let (first, end) = v.block_info(slab).unwrap().states;
        let bottom = (first..end)
            .find(|&s| {
                v.state_properties(s)
                    .any(|(k, val)| k == "type" && val == "bottom")
            })
            .unwrap();
        assert_eq!(
            v.collision_shape(bottom),
            &[Aabb {
                min: [0.0, 0.0, 0.0],
                max: [1.0, 0.5, 1.0]
            }]
        );
    }

    #[test]
    fn state_properties_decode() {
        let v = v26_3();
        let id = v.block_by_name("minecraft:acacia_button").unwrap();
        let (first, _) = v.block_info(id).unwrap().states;
        let props: Vec<_> = v.state_properties(first).collect();
        assert_eq!(
            props,
            [("face", "floor"), ("facing", "north"), ("powered", "true")]
        );
    }

    #[test]
    fn packet_ids() {
        let v = v26_3();
        assert_eq!(
            v.packet_id(
                ProtocolState::Login,
                Direction::Clientbound,
                "minecraft:login_finished"
            ),
            Some(2)
        );
        assert_eq!(
            v.packet_name(ProtocolState::Login, Direction::Clientbound, 2),
            Some("minecraft:login_finished")
        );
        assert_eq!(v.protocol(), 777);
        assert_eq!(
            v.packet_id(
                ProtocolState::Play,
                Direction::Clientbound,
                "level_chunk_with_light"
            ),
            v.packet_id(
                ProtocolState::Play,
                Direction::Clientbound,
                "minecraft:level_chunk_with_light"
            )
        );
        for state in [ProtocolState::Configuration, ProtocolState::Play] {
            for dir in [Direction::Clientbound, Direction::Serverbound] {
                let mut id = 0;
                while let Some(name) = v.packet_name(state, dir, id) {
                    assert_eq!(v.packet_id(state, dir, name), Some(id), "{name}");
                    id += 1;
                }
                assert!(id > 5);
            }
        }
    }

    #[test]
    fn registries_round_trip() {
        let v = v26_3();
        for (id, name) in v.blocks().iter() {
            assert_eq!(v.block_by_name(name), Some(id));
        }
        let sword = v.item_by_name("minecraft:diamond_sword").unwrap();
        let info = v.item_info(sword).unwrap();
        assert_eq!(info.max_stack_size, 1);
        assert!(info.max_damage > 0);
        let stone_item = v.item_by_name("minecraft:stone").unwrap();
        assert_eq!(
            v.item_info(stone_item).unwrap().block,
            v.block_by_name("minecraft:stone")
        );
        let player = v.entity_type_by_name("minecraft:player").unwrap();
        let e = v.entity_type_info(player).unwrap();
        assert_eq!((e.width, e.height), (0.6, 1.8));
        let water = v.fluid_by_name("minecraft:water").unwrap();
        assert!(v.fluid_is_source(water));
    }

    #[test]
    fn water_state_has_fluid() {
        let v = v26_3();
        let id = v.block_by_name("minecraft:water").unwrap();
        let state = v.block_info(id).unwrap().default_state;
        let fluid = v.block_state(state).unwrap().fluid.unwrap();
        assert!(fluid.source);
        assert_eq!(fluid.amount, 8);
    }

    #[test]
    fn blob_round_trips() {
        let tables = tables::Tables::decode(generated::v26_3::BLOB).unwrap();
        assert_eq!(tables.encode(), generated::v26_3::BLOB);
    }
}
