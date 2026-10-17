pub const PLAYER: &str = "Soak";
pub const CYCLE_SECS: u32 = 60;
/// Sent once the client joined: a player killed by the void or a mob respawns on its own.
pub const SETUP: [&str; 1] = ["gamerule immediate_respawn true"];

const MOBS: [&str; 14] = [
    "zombie{NoAI:1b,CustomNameVisible:1b,CustomName:\"z{n}\"}",
    "armor_stand{ShowArms:1b,equipment:{head:{id:\"minecraft:diamond_helmet\",count:1}}}",
    "item{Item:{id:\"minecraft:apple\",count:3},PickupDelay:32767}",
    "text_display{text:\"wave {n}\",billboard:\"center\"}",
    "block_display{block_state:\"minecraft:oak_log\"}",
    "item_display{item:{id:\"minecraft:diamond\",count:1}}",
    "pig{NoAI:1b}",
    "sheep{NoAI:1b,Color:3b}",
    "villager{NoAI:1b,CustomName:\"v{n}\"}",
    "experience_orb{Value:5s}",
    "arrow{NoGravity:1b}",
    "falling_block{BlockState:\"minecraft:sand\",NoGravity:1b}",
    "chest_minecart{NoGravity:1b}",
    "allay{NoAI:1b}",
];

const PLACES: [(i32, i32); 10] = [
    (4000, 0),
    (0, 6000),
    (-8000, 2000),
    (12000, -12000),
    (-3000, -9000),
    (20000, 5000),
    (-15000, 15000),
    (7000, 18000),
    (-20000, -4000),
    (2500, -20000),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disconnect {
    Kick,
    Transfer,
}

pub fn commands(cycle: u32, second: u32, wave: u32) -> Vec<String> {
    let p = PLAYER;
    let n = cycle;
    match second {
        0 => {
            let mut out = Vec::with_capacity(wave as usize + 2);
            for i in 0..wave {
                let mob = MOBS[(i as usize + n as usize) % MOBS.len()].replace("{n}", &n.to_string());
                let (dx, dz) = ((i % 32) as i32 - 16, (i / 32 % 32) as i32 - 16);
                let dy = 2 + (i / 1024) as i32;
                out.push(format!(
                    "execute at {p} run summon minecraft:{} ~{dx} ~{dy} ~{dz} {}",
                    mob.split('{').next().unwrap_or_default(),
                    mob.find('{').map_or("", |at| &mob[at..]),
                ));
            }
            out
        }
        2 => (0..40)
            .flat_map(|k| {
                [
                    format!("say soak chat {n}-{k} {}", "x".repeat((k % 7 * 20) as usize)),
                    format!(
                        "tellraw {p} {{text:\"tell {n}-{k}\",color:\"aqua\",hover_event:{{action:\"show_text\",value:\"hover {k}\"}}}}"
                    ),
                ]
            })
            .collect(),
        4 => vec![
            format!("title {p} times 2 20 2"),
            format!("title {p} title \"Title {n}\""),
            format!("title {p} subtitle {{text:\"Sub {n}\",color:\"gold\"}}"),
            format!("title {p} actionbar \"Action {n}\""),
        ],
        6 => {
            let mut out = vec![
                format!("bossbar add soak:b{n} \"Bar {n}\""),
                format!("bossbar set soak:b{n} players {p}"),
                format!("bossbar set soak:b{n} value {}", n % 100),
                format!("bossbar set soak:b{n} color red"),
            ];
            if n > 0 {
                out.push(format!("bossbar remove soak:b{}", n - 1));
            }
            out
        }
        8 => {
            let mut out = vec![
                format!("scoreboard objectives add soak_o{n} dummy \"Objective {n}\""),
                format!("scoreboard objectives setdisplay sidebar soak_o{n}"),
                format!("team add soak_t{n}"),
                format!("team modify soak_t{n} prefix \"[{n}] \""),
            ];
            for k in 0..15 {
                out.push(format!("scoreboard players set p{n}_{k} soak_o{n} {k}"));
                out.push(format!("team join soak_t{n} p{n}_{k}"));
            }
            if n > 0 {
                out.push(format!("scoreboard objectives remove soak_o{}", n - 1));
                out.push(format!("team remove soak_t{}", n - 1));
                out.push("scoreboard players reset *".into());
            }
            out
        }
        10 => vec![
            format!(
                "give {p} minecraft:diamond_sword[custom_name=\"Blade {n}\",enchantments={{\"minecraft:sharpness\":{}}},lore=[\"lore {n}\"]]",
                n % 5 + 1
            ),
            format!(
                "give {p} minecraft:leather_chestplate[dyed_color={}]",
                n * 7919 % 0xFF_FFFF
            ),
            format!(
                "give {p} minecraft:written_book[written_book_content={{title:\"Book {n}\",author:\"soak\",pages:[\"page {n}\"]}}]"
            ),
            format!(
                "give {p} minecraft:firework_rocket[fireworks={{explosions:[{{shape:\"star\",colors:[I;{}]}}]}}]",
                n * 104_729 % 0xFF_FFFF
            ),
            format!(
                "give {p} minecraft:white_banner[banner_patterns=[{{pattern:\"minecraft:cross\",color:\"blue\"}}]]"
            ),
            format!("give {p} minecraft:potion[potion_contents={{potion:\"minecraft:healing\"}}]"),
            format!("give {p} minecraft:shield"),
            format!("effect give {p} minecraft:speed 30 1"),
            format!("effect give {p} minecraft:night_vision 30 0"),
        ],
        12 => {
            let mut out = Vec::new();
            for k in -6..=6 {
                if k % 2 == 0 {
                    out.push(format!(
                        "execute at {p} run setblock ~4 ~1 ~{k} minecraft:oak_sign{{front_text:{{messages:[\"sign {n}\",\"{k}\",\"\",\"\"]}}}}"
                    ));
                } else {
                    out.push(format!(
                        "execute at {p} run setblock ~4 ~1 ~{k} minecraft:white_banner{{patterns:[{{pattern:\"minecraft:stripe_top\",color:\"red\"}}]}}"
                    ));
                }
            }
            out
        }
        14 => vec![
            format!("execute at {p} run particle minecraft:flame ~ ~1 ~ 2 2 2 0.01 500"),
            format!("execute at {p} run particle minecraft:happy_villager ~ ~1 ~ 3 3 3 0 300"),
            format!("execute at {p} run playsound minecraft:entity.generic.explode master {p}"),
            format!("execute at {p} run playsound minecraft:music_disc.cat record {p}"),
            format!("execute at {p} run playsound minecraft:block.note_block.harp master {p}"),
        ],
        16 => match n % 3 {
            0 => vec!["weather rain".into(), format!("time set {}", n * 1700 % 24000)],
            1 => vec!["weather thunder".into(), format!("time set {}", n * 1700 % 24000)],
            _ => vec!["weather clear".into(), format!("time set {}", n * 1700 % 24000)],
        },
        20 => vec!["kill @e[type=!minecraft:player]".into()],
        22 => vec![
            format!("clear {p}"),
            format!("effect clear {p}"),
            format!("stopsound {p}"),
            format!("execute at {p} run fill ~4 ~1 ~-6 ~4 ~1 ~6 minecraft:air"),
        ],
        30 => {
            let (x, z) = PLACES[n as usize % PLACES.len()];
            match n % 4 {
                1 => vec![format!("execute in minecraft:the_nether run tp {p} {} 90 {}", x / 8, z / 8)],
                3 => vec![format!("execute in minecraft:the_end run tp {p} 0 90 0")],
                _ => vec![format!("execute in minecraft:overworld run tp {p} {x} 160 {z}")],
            }
        }
        40 if n % 4 == 2 => vec!["reload".into()],
        45 => vec![format!("execute at {p} run summon minecraft:zombie ~3 ~ ~3 {{NoAI:1b}}")],
        50 => vec!["kill @e[type=!minecraft:player]".into()],
        _ => Vec::new(),
    }
}

pub fn disconnect_command(kind: Disconnect, port: u16) -> String {
    match kind {
        Disconnect::Kick => format!("kick {PLAYER} soak reconnect"),
        Disconnect::Transfer => format!("transfer 127.0.0.1 {port} {PLAYER}"),
    }
}

/// Evenly spaced disconnects, none in the warm-up or the last two cycles; every other one is a transfer.
pub fn disconnect_cycles(count: u32, cycles: u32, warmup_cycles: u32) -> Vec<(u32, Disconnect)> {
    let first = warmup_cycles.max(1);
    let last = cycles.saturating_sub(2);
    if count == 0 || last <= first {
        return Vec::new();
    }
    let span = last - first;
    (0..count)
        .map(|i| {
            let cycle = first + span * (2 * i + 1) / (2 * count);
            let kind = if i % 2 == 0 {
                Disconnect::Kick
            } else {
                Disconnect::Transfer
            };
            (cycle, kind)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wave_summons_the_requested_count_around_the_player() {
        let cmds = commands(3, 0, 800);
        assert_eq!(cmds.len(), 800);
        assert!(
            cmds.iter()
                .all(|c| c.starts_with("execute at Soak run summon minecraft:"))
        );
        assert!(
            cmds.iter()
                .any(|c| c.contains("text_display") && c.contains("wave 3"))
        );
    }

    #[test]
    fn every_cycle_kills_what_it_summoned_and_removes_the_last_objective() {
        let all: Vec<String> = (0..CYCLE_SECS).flat_map(|s| commands(5, s, 10)).collect();
        assert!(all.iter().any(|c| c == "kill @e[type=!minecraft:player]"));
        assert!(
            all.iter()
                .any(|c| c == "scoreboard objectives remove soak_o4")
        );
        assert!(all.iter().any(|c| c == "bossbar remove soak:b4"));
        assert!(all.iter().any(|c| c == "team remove soak_t4"));
        assert!(all.iter().any(|c| c.starts_with("clear Soak")));
    }

    #[test]
    fn teleports_cycle_through_dimensions() {
        let tp = |n| commands(n, 30, 0).join(" ");
        assert!(tp(0).contains("in minecraft:overworld"));
        assert!(tp(1).contains("in minecraft:the_nether"));
        assert!(tp(3).contains("in minecraft:the_end"));
        assert!(commands(2, 40, 0).contains(&"reload".to_owned()));
    }

    #[test]
    fn disconnects_are_spread_after_warmup() {
        let d = disconnect_cycles(3, 20, 5);
        assert_eq!(d.len(), 3);
        assert!(d.iter().all(|&(c, _)| (5..18).contains(&c)));
        assert!(d.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(d[1].1, Disconnect::Transfer);
        assert!(disconnect_cycles(3, 4, 5).is_empty());
    }
}
