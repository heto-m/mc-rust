//! Client-side Brigadier tree built from the `commands` packet, with local completion.

use mcr_protocol::play::clientbound::ui::{
    ArgumentParser, CommandNode, Commands, NodeKind, StringKind,
};

const MAX_NODES: usize = 1 << 16;
const MAX_STATES: usize = 256;
const MAX_TOKENS: usize = 256;
const MAX_REDIRECTS: usize = 16;
pub const MAX_LOCAL_SUGGESTIONS: usize = 1024;
const ASK_SERVER: &str = "minecraft:ask_server";
const POST_EFFECTS: &str = "minecraft:post_effects";
/// The post chains a vanilla client ships, which `/posteffect` suggests.
const VANILLA_POST_EFFECTS: [&str; 5] = [
    "minecraft:blur",
    "minecraft:creeper",
    "minecraft:entity_outline",
    "minecraft:invert",
    "minecraft:spider",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommandTree {
    nodes: Vec<CommandNode>,
    root: usize,
}

/// What the tree knows about the word being completed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Completion {
    /// Character index in the input where the completed word starts.
    pub start: usize,
    pub local: Vec<String>,
    pub usage: Vec<String>,
    /// Some candidate argument takes its suggestions from the server.
    pub ask_server: bool,
}

/// Plain-chat completions: the server's custom entries that continue the word before the cursor
/// (character indices), with the index where that word starts.
pub fn complete_chat(entries: &[String], text: &str, cursor: usize) -> (usize, Vec<String>) {
    let before: Vec<char> = text.chars().take(cursor).collect();
    let start = before.iter().rposition(|c| *c == ' ').map_or(0, |i| i + 1);
    let prefix: String = before[start..].iter().collect::<String>().to_lowercase();
    let mut out: Vec<String> = entries
        .iter()
        .filter(|e| e.to_lowercase().starts_with(&prefix))
        .take(256)
        .cloned()
        .collect();
    out.sort_by_key(|e| e.to_lowercase());
    (start, out)
}

/// Names the tree cannot know by itself.
pub trait SuggestSource {
    fn players(&self) -> Vec<String>;
    fn registry(&self, id: &str) -> Vec<String>;
}

impl CommandTree {
    pub fn from_packet(packet: &Commands) -> Self {
        let n = packet.nodes.len();
        let root = usize::try_from(packet.root_index).unwrap_or(usize::MAX);
        if n == 0 || n > MAX_NODES || root >= n {
            return Self::default();
        }
        Self {
            nodes: packet.nodes.clone(),
            root,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    fn children(&self, node: usize, depth: usize) -> Vec<usize> {
        let Some(n) = self.nodes.get(node) else {
            return Vec::new();
        };
        let mut out: Vec<usize> = n
            .children
            .iter()
            .filter_map(|&c| usize::try_from(c).ok().filter(|&c| c < self.nodes.len()))
            .collect();
        if out.is_empty()
            && depth < MAX_REDIRECTS
            && let Some(r) = n.redirect.and_then(|r| usize::try_from(r).ok())
            && r < self.nodes.len()
        {
            out = self.children(r, depth + 1);
        }
        out
    }

    /// Completes the word ending at `cursor` (a character index) in `input`, which may start with `/`.
    pub fn complete(&self, input: &str, cursor: usize, source: &dyn SuggestSource) -> Completion {
        let chars: Vec<char> = input.chars().take(cursor).collect();
        let skip = usize::from(chars.first() == Some(&'/'));
        let body = &chars[skip..];
        let word_start = body
            .iter()
            .rposition(|c| c.is_whitespace())
            .map_or(0, |i| i + 1);
        let mut out = Completion {
            start: skip + word_start,
            ..Completion::default()
        };
        if self.nodes.is_empty() {
            return out;
        }
        let words: Vec<String> = body[..word_start]
            .iter()
            .collect::<String>()
            .split_whitespace()
            .take(MAX_TOKENS)
            .map(str::to_owned)
            .collect();
        let partial: String = body[word_start..].iter().collect();

        let mut states: Vec<(usize, usize)> = vec![(self.root, 0)];
        let mut i = 0;
        while i < words.len() {
            let mut next: Vec<(usize, usize)> = Vec::new();
            for &(node, skip_words) in &states {
                if skip_words > 0 {
                    next.push((node, skip_words - 1));
                    continue;
                }
                for child in self.children(node, 0) {
                    let Some(c) = self.nodes.get(child) else {
                        continue;
                    };
                    match &c.kind {
                        NodeKind::Root => {}
                        NodeKind::Literal(l) => {
                            if *l == words[i] {
                                next.push((child, 0));
                            }
                        }
                        NodeKind::Argument { parser, .. } => match consumes(parser, &words[i..]) {
                            Consume::Words(k) if k >= 1 => next.push((child, k - 1)),
                            Consume::Rest => next.push((child, usize::MAX)),
                            _ => {}
                        },
                    }
                }
            }
            next.sort_unstable();
            next.dedup();
            next.truncate(MAX_STATES);
            states = next;
            if states.is_empty() {
                return out;
            }
            i += 1;
        }

        let lower = partial.to_lowercase();
        let mut seen = std::collections::BTreeSet::new();
        for &(node, skip_words) in &states {
            if skip_words == usize::MAX {
                continue;
            }
            if skip_words > 0 {
                if let Some(NodeKind::Argument { parser, .. }) =
                    self.nodes.get(node).map(|n| &n.kind)
                {
                    out.usage.push(usage_of(parser));
                }
                continue;
            }
            for child in self.children(node, 0) {
                let Some(c) = self.nodes.get(child) else {
                    continue;
                };
                match &c.kind {
                    NodeKind::Root => {}
                    NodeKind::Literal(l) => {
                        if l.to_lowercase().starts_with(&lower) {
                            seen.insert(l.clone());
                        }
                        out.usage.push(l.clone());
                    }
                    NodeKind::Argument {
                        name,
                        parser,
                        suggestions,
                    } => {
                        out.usage.push(format!("<{name}>"));
                        let values = match suggestions.as_ref().map(|s| s.as_str()) {
                            Some(ASK_SERVER) => {
                                out.ask_server = true;
                                Vec::new()
                            }
                            Some(POST_EFFECTS) => VANILLA_POST_EFFECTS.map(str::to_owned).to_vec(),
                            Some(_) => Vec::new(),
                            None => parser_values(parser, source),
                        };
                        for s in values {
                            if s.to_lowercase().starts_with(&lower)
                                || s.strip_prefix("minecraft:")
                                    .is_some_and(|r| r.to_lowercase().starts_with(&lower))
                            {
                                seen.insert(s);
                            }
                        }
                    }
                }
            }
        }
        out.local = seen.into_iter().take(MAX_LOCAL_SUGGESTIONS).collect();
        out.usage.sort();
        out.usage.dedup();
        out.usage.truncate(32);
        out
    }
}

enum Consume {
    Words(usize),
    Rest,
    Invalid,
}

fn consumes(parser: &ArgumentParser, words: &[String]) -> Consume {
    use ArgumentParser as P;
    let w = &words[0];
    let numeric = |ok: bool| {
        if ok {
            Consume::Words(1)
        } else {
            Consume::Invalid
        }
    };
    let multi = |n: usize| {
        if words.len() >= n {
            Consume::Words(n)
        } else {
            Consume::Words(words.len())
        }
    };
    match parser {
        P::Bool => numeric(w == "true" || w == "false"),
        P::Integer { .. } | P::Long { .. } => numeric(w.parse::<i64>().is_ok()),
        P::Float { .. } | P::Double { .. } => numeric(w.parse::<f64>().is_ok()),
        P::String(StringKind::GreedyPhrase)
        | P::Message
        | P::Component
        | P::Style
        | P::NbtCompoundTag
        | P::NbtTag
        | P::Dialog => Consume::Rest,
        P::Vec3 | P::BlockPos => multi(3),
        P::Vec2 | P::ColumnPos | P::Rotation => multi(2),
        P::Gamemode => numeric(matches!(
            w.as_str(),
            "survival" | "creative" | "adventure" | "spectator"
        )),
        _ => Consume::Words(1),
    }
}

fn usage_of(parser: &ArgumentParser) -> String {
    use ArgumentParser as P;
    match parser {
        P::Vec3 | P::BlockPos => "<x> <y> <z>".into(),
        P::Vec2 | P::ColumnPos | P::Rotation => "<x> <z>".into(),
        _ => "...".into(),
    }
}

const COLORS: [&str; 17] = [
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
    "reset",
];

fn parser_values(parser: &ArgumentParser, source: &dyn SuggestSource) -> Vec<String> {
    use ArgumentParser as P;
    let strs = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    match parser {
        P::Bool => strs(&["true", "false"]),
        P::Gamemode => strs(&["survival", "creative", "adventure", "spectator"]),
        P::TeamColor => strs(&COLORS),
        P::EntityAnchor => strs(&["feet", "eyes"]),
        P::Operation => strs(&["=", "+=", "-=", "*=", "/=", "%=", "<", ">", "><"]),
        P::Heightmap => strs(&[
            "world_surface",
            "motion_blocking",
            "motion_blocking_no_leaves",
            "ocean_floor",
        ]),
        P::TemplateMirror => strs(&["none", "left_right", "front_back"]),
        P::TemplateRotation => strs(&["none", "clockwise_90", "180", "counterclockwise_90"]),
        P::Swizzle => strs(&["x", "y", "z", "xy", "xz", "yz", "xyz"]),
        P::ScoreboardSlot => strs(&["list", "sidebar", "below_name"]),
        P::Entity { players_only, .. } => {
            let mut v = source.players();
            if !players_only {
                v.extend(strs(&["@a", "@e", "@p", "@r", "@s", "@n"]));
            } else {
                v.extend(strs(&["@a", "@p", "@r", "@s"]));
            }
            v
        }
        P::ScoreHolder { .. } => {
            let mut v = source.players();
            v.extend(strs(&["@a", "@e", "@p", "@r", "@s", "*"]));
            v
        }
        P::GameProfile => {
            let mut v = source.players();
            v.extend(strs(&["@a", "@p", "@r", "@s"]));
            v
        }
        P::Dimension => source.registry("minecraft:dimension"),
        P::ItemStack | P::ItemPredicate | P::ItemSlot => source.registry("minecraft:item"),
        P::BlockState | P::BlockPredicate => source.registry("minecraft:block"),
        P::Particle => source.registry("minecraft:particle_type"),
        P::Resource(r) | P::ResourceKey(r) | P::ResourceOrTag(r) | P::ResourceOrTagKey(r) => {
            source.registry(r.as_str())
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcr_buf::Identifier;

    struct NoSource;

    impl SuggestSource for NoSource {
        fn players(&self) -> Vec<String> {
            vec!["Alex".into()]
        }

        fn registry(&self, _id: &str) -> Vec<String> {
            Vec::new()
        }
    }

    fn lit(name: &str, children: Vec<i32>, executable: bool) -> CommandNode {
        CommandNode {
            executable,
            restricted: false,
            children,
            redirect: None,
            kind: NodeKind::Literal(name.into()),
        }
    }

    fn tree() -> CommandTree {
        let root = CommandNode {
            executable: false,
            restricted: false,
            children: vec![1, 2, 3],
            redirect: None,
            kind: NodeKind::Root,
        };
        let gamemode = lit("gamemode", vec![4], false);
        let gamerule = lit("gamerule", vec![], true);
        let give = lit("give", vec![5], false);
        let mode = CommandNode {
            executable: true,
            restricted: false,
            children: vec![],
            redirect: None,
            kind: NodeKind::Argument {
                name: "gamemode".into(),
                parser: ArgumentParser::Gamemode,
                suggestions: None,
            },
        };
        let targets = CommandNode {
            executable: true,
            restricted: false,
            children: vec![],
            redirect: None,
            kind: NodeKind::Argument {
                name: "targets".into(),
                parser: ArgumentParser::Entity {
                    single: false,
                    players_only: true,
                },
                suggestions: Some(Identifier::parse("minecraft:ask_server").unwrap()),
            },
        };
        CommandTree::from_packet(&Commands {
            nodes: vec![root, gamemode, gamerule, give, mode, targets],
            root_index: 0,
        })
    }

    #[test]
    fn completes_literals() {
        let c = tree().complete("/gam", 4, &NoSource);
        assert_eq!(c.start, 1);
        assert_eq!(c.local, ["gamemode", "gamerule"]);
    }

    #[test]
    fn completes_enumerated_arguments() {
        let c = tree().complete("/gamemode c", 11, &NoSource);
        assert_eq!(c.start, 10);
        assert_eq!(c.local, ["creative"]);
    }

    #[test]
    fn asks_server_for_server_suggestions() {
        let c = tree().complete("/give A", 7, &NoSource);
        assert!(c.ask_server);
        assert_eq!(c.start, 6);
    }

    #[test]
    fn post_effect_arguments_suggest_vanilla_chains() {
        let mut t = tree();
        let NodeKind::Argument { suggestions, .. } = &mut t.nodes[5].kind else {
            panic!("targets is an argument");
        };
        *suggestions = Some(Identifier::parse(POST_EFFECTS).unwrap());
        let c = t.complete("/give A", 7, &NoSource);
        assert!(!c.ask_server);
        let c = t.complete("/give in", 8, &NoSource);
        assert_eq!(c.local, ["minecraft:invert"]);
    }

    #[test]
    fn invalid_prefix_has_no_suggestions() {
        let c = tree().complete("/nope x", 7, &NoSource);
        assert!(c.local.is_empty() && !c.ask_server);
    }

    #[test]
    fn hostile_indices_are_ignored() {
        let mut t = tree();
        t.nodes[0].children = vec![-5, 99, 1];
        t.nodes[1].redirect = Some(1);
        let c = t.complete("/g", 2, &NoSource);
        assert_eq!(c.local, ["gamemode"]);
        let bad = CommandTree::from_packet(&Commands {
            nodes: vec![],
            root_index: 3,
        });
        assert!(bad.is_empty());
    }
}
