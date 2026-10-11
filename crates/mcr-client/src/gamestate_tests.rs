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

fn heavy_entry(id: i32, items: usize) -> RecipeBookEntry {
    let mut e = entry(id);
    e.contents.display = RecipeDisplay::Stonecutter {
        input: SlotDisplay::Composite(vec![SlotDisplay::Item(1); items]),
        result: SlotDisplay::Item(1),
        crafting_station: SlotDisplay::Item(1),
    };
    e
}

#[test]
fn oversized_recipe_displays_are_dropped() {
    let mut g = GameplayState::default();
    g.recipe_book.add(RecipeBookAdd {
        entries: vec![heavy_entry(1, MAX_RECIPE_WEIGHT), entry(2)],
        replace: false,
    });
    assert!(!g.recipe_book.entries.contains_key(&1));
    assert!(g.recipe_book.entries.contains_key(&2));
}

#[test]
fn recipe_book_weight_is_bounded_and_tracked() {
    let mut g = GameplayState::default();
    let per = MAX_RECIPE_WEIGHT - 8;
    let count = MAX_RECIPE_BOOK_WEIGHT / per + 16;
    for batch in 0..count / 64 + 1 {
        let ids = (batch * 64..(batch + 1) * 64).map(|i| i as i32);
        g.recipe_book.add(RecipeBookAdd {
            entries: ids.map(|i| heavy_entry(i, per)).collect(),
            replace: false,
        });
        assert!(g.recipe_book.weight() <= MAX_RECIPE_BOOK_WEIGHT);
    }
    let ids: Vec<i32> = g.recipe_book.entries.keys().copied().collect();
    g.recipe_book.remove(ids);
    assert_eq!(g.recipe_book.weight(), 0);
    g.recipe_book.add(RecipeBookAdd {
        entries: vec![entry(1), entry(1)],
        replace: false,
    });
    assert_eq!(g.recipe_book.weight(), 3);
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

fn toasting(id: &str, show_toast: bool) -> PositionedAdvancement {
    use mcr_protocol::play::clientbound::ui::{AdvancementFrame, DisplayInfo};
    use mcr_protocol::types::Component;
    use mcr_protocol::types::item::ItemStackTemplate;
    PositionedAdvancement {
        id: Identifier::new("minecraft", id).unwrap(),
        advancement: Advancement {
            parent: None,
            display: Some(DisplayInfo {
                title: Component::text(id),
                description: Component::text(""),
                icon: ItemStackTemplate::default(),
                frame: AdvancementFrame::Goal,
                background: None,
                show_toast,
                hidden: false,
            }),
            requirements: vec![vec!["c".into()]],
            sends_telemetry_event: false,
        },
        x: 0.0,
        y: 0.0,
    }
}

fn done(id: &str) -> Pairs<Identifier, Pairs<String, Option<i64>>> {
    Pairs(vec![(
        Identifier::new("minecraft", id).unwrap(),
        Pairs(vec![("c".to_owned(), Some(1))]),
    )])
}

fn update(
    reset: bool,
    added: Vec<PositionedAdvancement>,
    progress: Pairs<Identifier, Pairs<String, Option<i64>>>,
) -> UpdateAdvancements {
    UpdateAdvancements {
        reset,
        added,
        removed: vec![],
        progress,
        show_advancements: true,
    }
}

#[test]
fn newly_completed_advancements_toast_but_the_initial_sync_does_not() {
    let mut g = GameplayState::default();
    let sync = update(true, vec![toasting("a", true)], done("a"));
    assert!(g.advancements.apply(sync).is_empty());
    let silent = update(false, vec![toasting("b", false)], done("b"));
    assert!(g.advancements.apply(silent).is_empty());
    let fresh = update(false, vec![toasting("c", true)], done("c"));
    assert_eq!(g.advancements.apply(fresh).len(), 1);
    assert!(
        g.advancements
            .apply(update(false, vec![], done("c")))
            .is_empty()
    );
}

#[test]
fn only_notifying_recipes_toast() {
    let mut g = GameplayState::default();
    let mut quiet = entry(1);
    let mut loud = entry(2);
    loud.flags = RecipeBookEntry::FLAG_NOTIFICATION;
    quiet.flags = RecipeBookEntry::FLAG_HIGHLIGHT;
    let toasts = g.recipe_book.add(RecipeBookAdd {
        entries: vec![quiet, loud],
        replace: false,
    });
    assert_eq!(toasts.len(), 1);
}

#[test]
fn advancement_definitions_keep_the_servers_order() {
    let mut g = GameplayState::default();
    let apply = |g: &mut GameplayState, ids: &[usize], removed: &[usize]| {
        g.advancements.apply(UpdateAdvancements {
            reset: false,
            added: ids.iter().map(|&i| advancement(i)).collect(),
            removed: removed
                .iter()
                .map(|i| Identifier::parse(&format!("test:a{i}")).unwrap())
                .collect(),
            progress: Pairs(vec![]),
            show_advancements: true,
        });
    };
    apply(&mut g, &[3, 1, 2], &[]);
    apply(&mut g, &[0, 1], &[3]);
    let ids: Vec<&str> = g
        .advancements
        .ordered_defs()
        .map(|d| d.id.as_str())
        .collect();
    assert_eq!(ids, ["test:a1", "test:a2", "test:a0"]);
    assert_eq!(g.advancements.revision, 2);
}

#[test]
fn ghost_recipes_carry_a_fresh_revision() {
    let mut g = GameplayState::default();
    let display = entry(1).contents.display;
    g.set_ghost_recipe(0, display.clone());
    let first = g.ghost_recipe.as_ref().unwrap().revision;
    g.set_ghost_recipe(0, display);
    assert!(g.ghost_recipe.as_ref().unwrap().revision > first);
}
