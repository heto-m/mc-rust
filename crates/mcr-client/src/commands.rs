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

/// (node, byte position, message arguments found so far as (node, start)).
type ParseState = (usize, usize, Vec<(usize, usize)>);

impl CommandTree {
    /// Vanilla's `SignableCommand.of`: the `minecraft:message` arguments, as (name, text), of the first
    /// parse that consumes the whole command (no leading slash) and ends on an executable node.
    pub fn signable_arguments(&self, command: &str) -> Vec<(String, String)> {
        if self.nodes.is_empty() || command.is_empty() {
            return Vec::new();
        }
        let mut stack: Vec<ParseState> = vec![(self.root, 0, Vec::new())];
        let mut steps = 0;
        while let Some((node, pos, found)) = stack.pop() {
            steps += 1;
            if steps > MAX_PARSE_STEPS {
                break;
            }
            let children = self.children(node, 0);
            let literal = children.iter().copied().find(|&c| {
                matches!(&self.nodes[c].kind, NodeKind::Literal(l) if literal_end(command, pos, l).is_some())
            });
            let candidates: Vec<usize> = match literal {
                Some(c) => vec![c],
                None => children
                    .into_iter()
                    .filter(|&c| matches!(self.nodes[c].kind, NodeKind::Argument { .. }))
                    .collect(),
            };
            let mut next = Vec::new();
            for child in candidates {
                let Some(end) = node_end(&self.nodes[child].kind, command, pos) else {
                    continue;
                };
                let mut found = found.clone();
                if matches!(
                    self.nodes[child].kind,
                    NodeKind::Argument {
                        parser: ArgumentParser::Message,
                        ..
                    }
                ) && found.len() < MAX_SIGNED_ARGUMENTS
                {
                    found.push((child, pos));
                }
                if end == command.len() {
                    if self.nodes[child].executable {
                        return found
                            .into_iter()
                            .filter_map(|(n, start)| match &self.nodes[n].kind {
                                NodeKind::Argument { name, .. } => {
                                    Some((name.clone(), command[start..].to_owned()))
                                }
                                _ => None,
                            })
                            .collect();
                    }
                } else if command.as_bytes()[end] == b' ' && end + 1 < command.len() {
                    next.push((child, end + 1, found));
                }
            }
            for state in next.into_iter().rev() {
                if stack.len() < MAX_STATES {
                    stack.push(state);
                }
            }
        }
        Vec::new()
    }
}

const MAX_PARSE_STEPS: usize = 4096;
const MAX_SIGNED_ARGUMENTS: usize = 8;

fn literal_end(command: &str, pos: usize, literal: &str) -> Option<usize> {
    let end = pos + literal.len();
    (command[pos..].starts_with(literal)
        && (end == command.len() || command.as_bytes()[end] == b' '))
        .then_some(end)
}

fn token_end(command: &str, pos: usize) -> usize {
    command[pos..].find(' ').map_or(command.len(), |i| pos + i)
}

fn tokens_end(command: &str, pos: usize, count: usize) -> Option<usize> {
    let mut end = token_end(command, pos);
    for _ in 1..count {
        if end + 1 >= command.len() {
            return None;
        }
        end = token_end(command, end + 1);
    }
    Some(end)
}

/// End of a quoted string starting at `pos`, past its closing quote.
fn quoted_end(command: &str, pos: usize) -> Option<usize> {
    let bytes = command.as_bytes();
    let quote = bytes[pos];
    let mut i = pos + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// End of an entity selector such as `@a[distance=..5, limit=1]`, brackets and quotes included.
fn selector_end(command: &str, pos: usize) -> Option<usize> {
    let bytes = command.as_bytes();
    let open = pos + 2;
    if bytes.get(open) != Some(&b'[') {
        return Some(token_end(command, pos));
    }
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => i = quoted_end(command, i)?,
            b']' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

fn node_end(kind: &NodeKind, command: &str, pos: usize) -> Option<usize> {
    use ArgumentParser as P;
    if pos >= command.len() {
        return None;
    }
    let parser = match kind {
        NodeKind::Root => return None,
        NodeKind::Literal(l) => return literal_end(command, pos, l),
        NodeKind::Argument { parser, .. } => parser,
    };
    let first = command.as_bytes()[pos];
    match parser {
        P::Message
        | P::String(StringKind::GreedyPhrase)
        | P::Component
        | P::Style
        | P::NbtCompoundTag
        | P::NbtTag
        | P::Dialog => Some(command.len()),
        P::String(StringKind::QuotablePhrase) if first == b'"' || first == b'\'' => {
            quoted_end(command, pos)
        }
        P::Entity { .. } | P::GameProfile | P::ScoreHolder { .. } if first == b'@' => {
            selector_end(command, pos)
        }
        P::Vec3 | P::BlockPos => tokens_end(command, pos, 3),
        P::Vec2 | P::ColumnPos | P::Rotation => tokens_end(command, pos, 2),
        _ => Some(token_end(command, pos)),
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

    fn arg(
        name: &str,
        parser: ArgumentParser,
        children: Vec<i32>,
        executable: bool,
    ) -> CommandNode {
        CommandNode {
            executable,
            restricted: false,
            children,
            redirect: None,
            kind: NodeKind::Argument {
                name: name.into(),
                parser,
                suggestions: None,
            },
        }
    }

    /// The shape of 26.3's `msg`, `say`, `execute`, `time`, `tellraw` and `teleport` nodes.
    fn signing_tree() -> CommandTree {
        let players = || ArgumentParser::Entity {
            single: false,
            players_only: true,
        };
        let entities = || ArgumentParser::Entity {
            single: false,
            players_only: false,
        };
        let mut nodes = vec![
            CommandNode {
                executable: false,
                restricted: false,
                children: vec![1, 4, 6, 11, 14, 17],
                redirect: None,
                kind: NodeKind::Root,
            },
            lit("msg", vec![2], false),
            arg("targets", players(), vec![3], false),
            arg("message", ArgumentParser::Message, vec![], true),
            lit("say", vec![5], false),
            arg("message", ArgumentParser::Message, vec![], true),
            lit("execute", vec![7, 9], false),
            lit("as", vec![8], false),
            arg("targets", entities(), vec![], false),
            lit("run", vec![], false),
            arg("unused", ArgumentParser::Bool, vec![], true),
            lit("time", vec![12], false),
            lit("query", vec![13], false),
            lit("daytime", vec![], true),
            lit("tellraw", vec![15], false),
            arg("targets", players(), vec![16], false),
            arg("message", ArgumentParser::Component, vec![], true),
            lit("teleport", vec![18], false),
            arg("location", ArgumentParser::Vec3, vec![19], true),
            arg("reason", ArgumentParser::Message, vec![], true),
        ];
        nodes[8].redirect = Some(6);
        nodes[9].redirect = Some(0);
        CommandTree::from_packet(&Commands {
            nodes,
            root_index: 0,
        })
    }

    fn signed(command: &str) -> Vec<(String, String)> {
        signing_tree().signable_arguments(command)
    }

    fn pair(name: &str, value: &str) -> Vec<(String, String)> {
        vec![(name.to_owned(), value.to_owned())]
    }

    #[test]
    fn message_arguments_are_signable() {
        assert_eq!(
            signed("msg Alex hello there"),
            pair("message", "hello there")
        );
        assert_eq!(signed("say  two spaces"), pair("message", " two spaces"));
        assert_eq!(
            signed("msg @a[distance=..5, limit=1] hi"),
            pair("message", "hi")
        );
        assert_eq!(
            signed("msg @a[name=\"x ] y\"] quoted"),
            pair("message", "quoted")
        );
        assert_eq!(
            signed("teleport 1 ~2 ^3 because"),
            pair("reason", "because")
        );
    }

    #[test]
    fn redirects_reach_signable_arguments() {
        assert_eq!(
            signed("execute as @a[distance=..5, limit=1] run say hi"),
            pair("message", "hi")
        );
        assert_eq!(
            signed("execute as Alex as Steve run msg Alex yo"),
            pair("message", "yo")
        );
    }

    #[test]
    fn commands_without_message_arguments_stay_plain() {
        for command in [
            "time query daytime",
            "tellraw @a {\"text\":\"hi\"}",
            "teleport 1 2 3",
            "msg Alex",
            "msg Alex ",
            "say",
            "nope hi",
            "time query",
            "",
        ] {
            assert!(signed(command).is_empty(), "{command:?}");
        }
        assert!(
            CommandTree::default()
                .signable_arguments("say hi")
                .is_empty()
        );
    }

    #[test]
    fn hostile_trees_and_inputs_stay_bounded() {
        let mut nodes = vec![CommandNode {
            executable: false,
            restricted: false,
            children: vec![1],
            redirect: None,
            kind: NodeKind::Root,
        }];
        nodes.push(CommandNode {
            redirect: Some(0),
            ..lit("a", vec![], false)
        });
        let tree = CommandTree::from_packet(&Commands {
            nodes,
            root_index: 0,
        });
        let long = vec!["a"; 5000].join(" ");
        assert!(tree.signable_arguments(&long).is_empty());
        let wide: Vec<CommandNode> = std::iter::once(CommandNode {
            executable: false,
            restricted: false,
            children: (1..200).collect(),
            redirect: None,
            kind: NodeKind::Root,
        })
        .chain((1..200).map(|_| {
            arg(
                "w",
                ArgumentParser::String(StringKind::SingleWord),
                (1..200).collect(),
                false,
            )
        }))
        .collect();
        let tree = CommandTree::from_packet(&Commands {
            nodes: wide,
            root_index: 0,
        });
        assert!(tree.signable_arguments(&vec!["x"; 50].join(" ")).is_empty());
        assert!(signed("msg @a[unclosed hi").is_empty());
        assert!(signed("say é ü 😀").len() == 1);
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
