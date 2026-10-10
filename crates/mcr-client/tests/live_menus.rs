//! `MCR_LIVE_ADDR=localhost:25610 MCR_LIVE_CONSOLE=<stdin file> MCR_LIVE_LOG=<server log> \
//!  cargo test -p mcr-client --test live_menus -- --ignored --nocapture`
//!
//! Every menu is opened, filled with shift-clicks and plain clicks, and the server may only send slot
//! updates for slots it computes itself (results, smelting); any other update is a prediction miss.

mod common;

use common::Live;
use mcr_buf::BlockPos;
use mcr_client::ServerUpdate;
use mcr_protocol::play::serverbound::{ContainerInput as I, Facing};
use mcr_protocol::types::game::Hand;

struct Case {
    name: &'static str,
    block: &'static str,
    give: &'static [&'static str],
    /// Container slots the server legitimately rewrites on its own.
    server_slots: &'static [i32],
    /// (slot, button, input); negative slots count back from the end of the container.
    steps: &'static [(i32, i8, I)],
}

const HOTBAR: i32 = -9;

fn at(slot: i32, n: usize) -> i16 {
    if slot < 0 {
        (n as i32 + slot) as i16
    } else {
        slot as i16
    }
}

const CASES: &[Case] = &[
    Case {
        name: "chest",
        block: "minecraft:chest",
        give: &[
            "minecraft:stone 64",
            "minecraft:iron_ingot 10",
            "minecraft:diamond_sword[damage=3]",
        ],
        server_slots: &[],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (0, 0, I::Pickup),
            (5, 0, I::Pickup),
            (1, 0, I::QuickMove),
            (2, 0, I::QuickMove),
            (0, 1, I::Pickup),
            (6, 0, I::Pickup),
        ],
    },
    Case {
        name: "shulker_box",
        block: "minecraft:white_shulker_box",
        give: &["minecraft:stone 64", "minecraft:white_shulker_box"],
        server_slots: &[],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (0, 0, I::QuickMove),
        ],
    },
    Case {
        name: "hopper",
        block: "minecraft:hopper",
        give: &["minecraft:stone 64", "minecraft:dirt 5"],
        server_slots: &[0, 1, 2, 3, 4],
        steps: &[(HOTBAR, 0, I::QuickMove), (HOTBAR + 1, 0, I::QuickMove)],
    },
    Case {
        name: "dispenser",
        block: "minecraft:dropper",
        give: &["minecraft:stone 64", "minecraft:dirt 5"],
        server_slots: &[],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (0, 0, I::QuickMove),
        ],
    },
    Case {
        name: "furnace",
        block: "minecraft:furnace",
        give: &[
            "minecraft:raw_iron 8",
            "minecraft:coal 8",
            "minecraft:stone 8",
            "minecraft:bucket",
        ],
        server_slots: &[0, 1, 2],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (HOTBAR + 3, 0, I::QuickMove),
        ],
    },
    Case {
        name: "blast_furnace",
        block: "minecraft:blast_furnace",
        give: &[
            "minecraft:raw_iron 8",
            "minecraft:coal 8",
            "minecraft:bread 8",
        ],
        server_slots: &[0, 1, 2],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
        ],
    },
    Case {
        name: "smoker",
        block: "minecraft:smoker",
        give: &[
            "minecraft:beef 8",
            "minecraft:coal 8",
            "minecraft:raw_iron 8",
        ],
        server_slots: &[0, 1, 2],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
        ],
    },
    Case {
        name: "enchanting_table",
        block: "minecraft:enchanting_table",
        give: &[
            "minecraft:iron_sword",
            "minecraft:lapis_lazuli 10",
            "minecraft:stone 4",
        ],
        server_slots: &[],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (0, 0, I::QuickMove),
            (1, 0, I::QuickMove),
        ],
    },
    Case {
        name: "brewing_stand",
        block: "minecraft:brewing_stand",
        give: &[
            "minecraft:blaze_powder 5",
            "minecraft:nether_wart 5",
            "minecraft:potion[potion_contents={potion:\"minecraft:water\"}]",
            "minecraft:glass_bottle 3",
            "minecraft:stone 3",
        ],
        server_slots: &[4],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (HOTBAR + 3, 0, I::QuickMove),
            (HOTBAR + 4, 0, I::QuickMove),
            (0, 0, I::QuickMove),
        ],
    },
    Case {
        name: "anvil",
        block: "minecraft:anvil",
        give: &[
            "minecraft:iron_sword[damage=10]",
            "minecraft:iron_sword[damage=20]",
            "minecraft:stone 4",
        ],
        server_slots: &[2],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (0, 0, I::QuickMove),
        ],
    },
    Case {
        name: "smithing_table",
        block: "minecraft:smithing_table",
        give: &[
            "minecraft:netherite_upgrade_smithing_template",
            "minecraft:diamond_sword",
            "minecraft:netherite_ingot",
            "minecraft:stone 3",
        ],
        server_slots: &[3],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (HOTBAR + 3, 0, I::QuickMove),
            (3, 0, I::Pickup),
            (8, 0, I::Pickup),
        ],
    },
    Case {
        name: "grindstone",
        block: "minecraft:grindstone",
        give: &[
            "minecraft:diamond_sword[enchantments={sharpness:3}]",
            "minecraft:iron_sword[damage=5]",
            "minecraft:stone 4",
        ],
        server_slots: &[2],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (2, 0, I::Pickup),
            (3, 0, I::Pickup),
        ],
    },
    Case {
        name: "stonecutter",
        block: "minecraft:stonecutter",
        give: &[
            "minecraft:stone 64",
            "minecraft:dirt 8",
            "minecraft:cobblestone 8",
        ],
        server_slots: &[1],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (0, 0, I::QuickMove),
        ],
    },
    Case {
        name: "loom",
        block: "minecraft:loom",
        give: &[
            "minecraft:white_banner",
            "minecraft:red_dye 4",
            "minecraft:creeper_banner_pattern",
            "minecraft:stone 3",
        ],
        server_slots: &[3],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (HOTBAR + 3, 0, I::QuickMove),
        ],
    },
    Case {
        name: "cartography_table",
        block: "minecraft:cartography_table",
        give: &[
            "minecraft:paper 8",
            "minecraft:filled_map[map_id=1]",
            "minecraft:glass_pane 3",
            "minecraft:stone 3",
        ],
        server_slots: &[2],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (HOTBAR + 3, 0, I::QuickMove),
        ],
    },
    Case {
        name: "beacon",
        block: "minecraft:beacon",
        give: &[
            "minecraft:iron_ingot",
            "minecraft:iron_ingot 5",
            "minecraft:stone 3",
        ],
        server_slots: &[],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (HOTBAR + 2, 0, I::QuickMove),
            (0, 0, I::QuickMove),
        ],
    },
    Case {
        name: "crafter",
        block: "minecraft:crafter",
        give: &["minecraft:stone 8", "minecraft:oak_planks 8"],
        server_slots: &[45],
        steps: &[
            (HOTBAR, 0, I::QuickMove),
            (HOTBAR + 1, 0, I::QuickMove),
            (0, 0, I::QuickMove),
        ],
    },
    Case {
        name: "crafting_table",
        block: "minecraft:crafting_table",
        give: &[
            "minecraft:oak_planks 8",
            "minecraft:milk_bucket",
            "minecraft:sugar 6",
            "minecraft:wheat 6",
            "minecraft:egg 3",
        ],
        server_slots: &[0],
        steps: &[
            (HOTBAR, 0, I::Pickup),
            (1, 1, I::Pickup),
            (2, 1, I::Pickup),
            (4, 1, I::Pickup),
            (5, 1, I::Pickup),
            (0, 0, I::Pickup),
            (9, 0, I::Pickup),
            (HOTBAR + 1, 0, I::Pickup),
            (1, 0, I::Pickup),
            (HOTBAR + 2, 0, I::Pickup),
            (2, 1, I::Pickup),
            (3, 1, I::Pickup),
            (HOTBAR + 3, 0, I::Pickup),
            (4, 1, I::Pickup),
            (5, 1, I::Pickup),
            (6, 1, I::Pickup),
            (HOTBAR + 4, 0, I::Pickup),
            (8, 0, I::Pickup),
            (0, 0, I::QuickMove),
        ],
    },
];

fn open_menu(live: &mut Live, case: &Case) -> usize {
    live.console("fill -4 100 -4 4 100 4 minecraft:stone");
    live.console("fill -4 101 -4 4 108 4 minecraft:air");
    live.console("tp Menu 0.5 101 0.5");
    live.console("clear Menu");
    live.idle(10);
    live.console(&format!("setblock 2 101 0 {}", case.block));
    for item in case.give {
        live.console(&format!("give Menu {item}"));
    }
    live.idle(15);
    live.client
        .use_item_on(
            Hand::Main,
            BlockPos { x: 2, y: 101, z: 0 },
            Facing::Up,
            [0.5, 1.0, 0.5],
            false,
        )
        .unwrap();
    live.until(case.name, 60, |l| l.client.state.inventory.open.is_some());
    live.idle(8);
    live.client
        .state
        .inventory
        .open
        .as_ref()
        .unwrap()
        .slots
        .len()
}

fn run_case(live: &mut Live, case: &Case) -> Vec<String> {
    let n = open_menu(live, case);
    let mut problems = Vec::new();
    let id = live.client.state.inventory.open.as_ref().unwrap().id;
    for &(slot, button, input) in case.steps {
        let slot = at(slot, n);
        live.client.state.inventory.update_log.clear();
        live.client.click_container(slot, button, input).unwrap();
        live.idle(6);
        let log: Vec<ServerUpdate> = live.client.state.inventory.update_log.clone();
        let bad: Vec<_> = log
            .iter()
            .filter(|u| u.container == id && (u.slot < 0 || !case.server_slots.contains(&u.slot)))
            .collect();
        if !bad.is_empty() {
            problems.push(format!(
                "{}: slot {slot} button {button} {input:?} -> unexpected server updates {bad:?}",
                case.name
            ));
        }
    }
    live.client.close_container().unwrap();
    live.idle(6);
    problems
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_menu_quick_move_and_results() {
    let mut live = Live::connect("Menu");
    live.console("op Menu");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console("gamemode survival Menu");
    live.idle(10);
    let only = std::env::var("MCR_CASES").ok();
    let mut problems = Vec::new();
    for case in CASES {
        if only
            .as_ref()
            .is_some_and(|o| !o.split(',').any(|c| c == case.name))
        {
            continue;
        }
        let found = run_case(&mut live, case);
        println!(
            "{}: {}",
            case.name,
            if found.is_empty() {
                "ok".into()
            } else {
                format!("{} problems", found.len())
            }
        );
        problems.extend(found);
    }
    for p in &problems {
        println!("PROBLEM {p}");
    }
    assert!(
        problems.is_empty(),
        "{} menu prediction misses",
        problems.len()
    );
}
