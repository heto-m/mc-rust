use mcr_buf::Identifier;
use mcr_protocol::play::clientbound::entity::{
    TrackedWaypoint, Waypoint, WaypointData, WaypointIcon, WaypointId, WaypointOperation,
};
use mcr_protocol::play::clientbound::ui::{
    Advancement, MapDecoration, MapItemData, MapPatch, Pairs, PositionedAdvancement, RecipeBookAdd,
    RecipeBookEntry, RecipeDisplay, RecipeDisplayEntry, SlotDisplay, UpdateAdvancements,
};

use crate::gamestate::*;

fn entry(id: i32) -> RecipeBookEntry {
    RecipeBookEntry {
        contents: RecipeDisplayEntry {
            id,
            display: RecipeDisplay::Stonecutter {
                input: SlotDisplay::Item(1),
                result: SlotDisplay::Item(1),
                crafting_station: SlotDisplay::Item(1),
            },
            group: None,
            category: 0,
            crafting_requirements: None,
        },
        flags: 0,
    }
}

#[test]
fn recipe_book_is_bounded_across_packets() {
    let mut g = GameplayState::default();
    for batch in 0..64 {
        g.recipe_book.add(RecipeBookAdd {
            entries: (0..4096).map(|i| entry(batch * 4096 + i)).collect(),
            replace: false,
        });
    }
    assert!(g.recipe_book.entries.len() <= MAX_RECIPES);
}

fn advancement(i: usize) -> PositionedAdvancement {
    PositionedAdvancement {
        id: Identifier::parse(&format!("test:a{i}")).unwrap(),
        advancement: Advancement {
            parent: None,
            display: None,
            requirements: vec![],
            sends_telemetry_event: false,
        },
        x: 0.0,
        y: 0.0,
    }
}

#[test]
fn advancement_progress_is_bounded_in_total() {
    let mut g = GameplayState::default();
    for batch in 0..40usize {
        let added: Vec<_> = (0..512).map(|i| advancement(batch * 512 + i)).collect();
        let progress = Pairs(
            added
                .iter()
                .map(|a| {
                    (
                        a.id.clone(),
                        Pairs((0..200).map(|c| (format!("c{c}"), Some(1))).collect()),
                    )
                })
                .collect(),
        );
        g.advancements.apply(UpdateAdvancements {
            reset: false,
            added,
            removed: vec![],
            progress,
            show_advancements: true,
        });
    }
    let a = &g.advancements;
    assert!(a.defs.len() <= MAX_ADVANCEMENTS);
    assert!(a.progress.len() <= MAX_ADVANCEMENTS);
    assert!(a.progress.values().map(|m| m.len()).sum::<usize>() <= MAX_CRITERIA);
    assert!(
        a.progress
            .values()
            .all(|m| m.len() <= MAX_CRITERIA_PER_ADVANCEMENT)
    );
}

#[test]
fn maps_stats_and_waypoints_are_bounded() {
    let mut g = GameplayState::default();
    for id in 0..5000 {
        g.apply_map(MapItemData {
            map_id: id,
            scale: 0,
            locked: false,
            decorations: Some(
                (0..1000)
                    .map(|_| MapDecoration {
                        decoration_type: 0,
                        x: 0,
                        y: 0,
                        rot: 0,
                        name: None,
                    })
                    .collect(),
            ),
            color_patch: Some(MapPatch {
                start_x: 0,
                start_y: 0,
                width: 255,
                height: 255,
                map_colors: vec![1; 255 * 255],
            }),
        });
        g.set_stat(id, id, 1);
        g.waypoints.apply(Waypoint {
            operation: WaypointOperation::Track,
            waypoint: TrackedWaypoint {
                id: WaypointId::Name(format!("w{id}")),
                icon: WaypointIcon {
                    style: Identifier::parse("minecraft:default").unwrap(),
                    color: None,
                },
                data: WaypointData::Empty,
            },
        });
    }
    for _ in 0..20 {
        for id in 0..20_000 {
            g.set_stat(id, 0, 1);
        }
    }
    assert!(g.maps.len() <= MAX_MAPS);
    assert!(
        g.maps
            .values()
            .all(|m| m.colors.len() == MAP_SIZE * MAP_SIZE)
    );
    assert!(
        g.maps
            .values()
            .all(|m| m.decorations.len() <= MAX_DECORATIONS)
    );
    assert!(g.stats.len() <= MAX_STATS);
    assert!(g.waypoints.tracked.len() <= MAX_WAYPOINTS);
}
