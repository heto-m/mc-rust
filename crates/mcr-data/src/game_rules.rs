//! Game rules a new 26.3 world can be created with: id, category, value type and default.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Player,
    Mobs,
    Spawning,
    Drops,
    Updates,
    Chat,
    Misc,
}

impl Category {
    pub const ALL: [Self; 7] = [
        Self::Player,
        Self::Mobs,
        Self::Spawning,
        Self::Drops,
        Self::Updates,
        Self::Chat,
        Self::Misc,
    ];

    pub fn lang_key(self) -> &'static str {
        match self {
            Self::Player => "gamerule.category.minecraft.player",
            Self::Mobs => "gamerule.category.minecraft.mobs",
            Self::Spawning => "gamerule.category.minecraft.spawning",
            Self::Drops => "gamerule.category.minecraft.drops",
            Self::Updates => "gamerule.category.minecraft.updates",
            Self::Chat => "gamerule.category.minecraft.chat",
            Self::Misc => "gamerule.category.minecraft.misc",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Bool(bool),
    Int { default: i32, min: i32, max: i32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameRule {
    pub id: &'static str,
    pub category: Category,
    pub kind: Kind,
}

impl GameRule {
    pub fn lang_key(&self) -> String {
        format!("gamerule.minecraft.{}", self.id)
    }

    pub fn default_value(&self) -> String {
        match self.kind {
            Kind::Bool(b) => b.to_string(),
            Kind::Int { default, .. } => default.to_string(),
        }
    }

    /// The value in the command's syntax, or `None` when it is not valid for this rule.
    pub fn normalize(&self, value: &str) -> Option<String> {
        let value = value.trim();
        match self.kind {
            Kind::Bool(_) => match value {
                "true" | "1" => Some("true".into()),
                "false" | "0" => Some("false".into()),
                _ => None,
            },
            Kind::Int { min, max, .. } => value
                .parse::<i32>()
                .ok()
                .filter(|v| (min..=max).contains(v))
                .map(|v| v.to_string()),
        }
    }
}

const fn b(id: &'static str, category: Category, default: bool) -> GameRule {
    GameRule {
        id,
        category,
        kind: Kind::Bool(default),
    }
}

const fn i(id: &'static str, category: Category, default: i32, min: i32, max: i32) -> GameRule {
    GameRule {
        id,
        category,
        kind: Kind::Int { default, min, max },
    }
}

use Category::{Chat, Drops, Misc, Mobs, Player, Spawning, Updates};

const MAX: i32 = i32::MAX;

pub const GAME_RULES_26_3: &[GameRule] = &[
    b("advance_time", Updates, true),
    b("advance_weather", Updates, true),
    b("allow_entering_nether_using_portals", Misc, true),
    b("block_drops", Drops, true),
    b("block_explosion_drop_decay", Drops, true),
    b("command_blocks_work", Misc, true),
    b("command_block_output", Chat, true),
    b("drowning_damage", Player, true),
    b("elytra_movement_check", Player, true),
    b("ender_pearls_vanish_on_death", Player, true),
    b("entity_drops", Drops, true),
    b("fall_damage", Player, true),
    b("fire_damage", Player, true),
    i("fire_spread_radius_around_player", Updates, 128, -1, MAX),
    b("forgive_dead_players", Mobs, true),
    b("freeze_damage", Player, true),
    b("global_sound_events", Misc, true),
    b("immediate_respawn", Player, false),
    b("keep_inventory", Player, false),
    b("lava_source_conversion", Updates, false),
    b("limited_crafting", Player, false),
    b("locator_bar", Player, true),
    b("log_admin_commands", Chat, true),
    i("max_block_modifications", Misc, 32768, 1, MAX),
    i("max_command_forks", Misc, 65536, 0, MAX),
    i("max_command_sequence_length", Misc, 65536, 0, MAX),
    i("max_entity_cramming", Mobs, 24, 0, MAX),
    i("max_minecart_speed", Misc, 8, 1, 1000),
    i("max_snow_accumulation_height", Updates, 1, 0, 8),
    b("mob_drops", Drops, true),
    b("mob_explosion_drop_decay", Drops, true),
    b("mob_griefing", Mobs, true),
    b("natural_health_regeneration", Player, true),
    b("player_movement_check", Player, true),
    i("players_nether_portal_creative_delay", Player, 0, 0, MAX),
    i("players_nether_portal_default_delay", Player, 80, 0, MAX),
    i("players_sleeping_percentage", Player, 100, 0, MAX),
    b("projectiles_can_break_blocks", Drops, true),
    b("pvp", Player, true),
    b("raids", Mobs, true),
    i("random_tick_speed", Updates, 3, 0, MAX),
    b("reduced_debug_info", Misc, false),
    i("respawn_radius", Player, 10, 0, MAX),
    b("send_command_feedback", Chat, true),
    b("show_advancement_messages", Chat, true),
    b("show_death_messages", Chat, true),
    b("spawner_blocks_work", Misc, true),
    b("spawn_mobs", Spawning, true),
    b("spawn_monsters", Spawning, true),
    b("spawn_patrols", Spawning, true),
    b("spawn_phantoms", Spawning, true),
    b("spawn_wandering_traders", Spawning, true),
    b("spawn_wardens", Spawning, true),
    b("spectators_generate_chunks", Player, true),
    b("spread_vines", Updates, true),
    b("tnt_explodes", Misc, true),
    b("tnt_explosion_drop_decay", Drops, false),
    b("universal_anger", Mobs, false),
    b("water_source_conversion", Updates, true),
];

/// The rule for `id`, with or without the `minecraft:` namespace.
pub fn game_rule(id: &str) -> Option<&'static GameRule> {
    let id = id.strip_prefix("minecraft:").unwrap_or(id);
    GAME_RULES_26_3.iter().find(|r| r.id == id)
}

/// `(id, value)` pairs reduced to known rules with valid values that differ from the default, last one winning.
pub fn changed_rules<'a>(
    pairs: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for (id, value) in pairs.into_iter().take(GAME_RULES_26_3.len() * 4) {
        let Some(rule) = game_rule(id) else { continue };
        let Some(value) = rule.normalize(value) else {
            continue;
        };
        out.retain(|(i, _)| *i != rule.id);
        if value != rule.default_value() {
            out.push((rule.id, value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        let keep = game_rule("minecraft:keep_inventory").unwrap();
        assert_eq!(keep.normalize("1").as_deref(), Some("true"));
        assert_eq!(keep.normalize("yes"), None);
        let snow = game_rule("max_snow_accumulation_height").unwrap();
        assert_eq!(snow.normalize("8").as_deref(), Some("8"));
        assert_eq!(snow.normalize("9"), None);
        assert_eq!(snow.normalize("-1"), None);
        assert!(game_rule("doDaylightCycle").is_none());
        let changed = changed_rules([
            ("keep_inventory", "true"),
            ("random_tick_speed", "3"),
            ("bogus", "1"),
            ("pvp", "maybe"),
            ("advance_time", "false"),
            ("advance_time", "true"),
        ]);
        assert_eq!(changed, vec![("keep_inventory", "true".to_owned())]);
    }

    #[test]
    fn ids_are_unique_and_defaults_valid() {
        for (n, rule) in GAME_RULES_26_3.iter().enumerate() {
            assert!(
                GAME_RULES_26_3[..n].iter().all(|r| r.id != rule.id),
                "{}",
                rule.id
            );
            assert!(
                rule.normalize(&rule.default_value()).is_some(),
                "{}",
                rule.id
            );
        }
    }

    /// Cross-checks the table with the data generator's command tree when the reports were generated.
    #[test]
    fn matches_generated_command_tree() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.cache/versions/26.3/generated/reports/commands.json");
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let tree: serde_json::Value = serde_json::from_str(&text).expect("commands.json");
        let children = tree["children"]["gamerule"]["children"]
            .as_object()
            .expect("gamerule command");
        let mut generated: Vec<&str> = children
            .keys()
            .map(|k| k.strip_prefix("minecraft:").unwrap_or(k))
            .collect();
        generated.sort_unstable();
        generated.dedup();
        let mut ours: Vec<&str> = GAME_RULES_26_3.iter().map(|r| r.id).collect();
        ours.sort_unstable();
        assert_eq!(ours, generated);
    }
}
