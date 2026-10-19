use mcr_protocol::play::clientbound::ui::{CommandNode, Commands};

use super::*;

fn lit(name: &str, children: Vec<i32>, executable: bool) -> CommandNode {
    CommandNode {
        executable,
        restricted: false,
        children,
        redirect: None,
        kind: NodeKind::Literal(name.into()),
    }
}

fn arg(name: &str, parser: ArgumentParser, children: Vec<i32>, executable: bool) -> CommandNode {
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

fn entities(single: bool, players_only: bool) -> ArgumentParser {
    ArgumentParser::Entity {
        single,
        players_only,
    }
}

/// gamemode (restricted), give, tp, say, execute as … run, trigger.
fn tree() -> CommandTree {
    let mut nodes = vec![
        CommandNode {
            executable: false,
            restricted: false,
            children: vec![1, 4, 8, 10, 12, 17],
            redirect: None,
            kind: NodeKind::Root,
        },
        lit("gamemode", vec![2], false),
        arg("gamemode", ArgumentParser::Gamemode, vec![3], true),
        arg("target", entities(false, true), vec![], true),
        lit("give", vec![5], false),
        arg("targets", entities(false, true), vec![6], false),
        arg("item", ArgumentParser::ItemStack, vec![7], true),
        arg(
            "count",
            ArgumentParser::Integer {
                min: Some(1),
                max: Some(6400),
            },
            vec![],
            true,
        ),
        lit("tp", vec![9], false),
        arg("location", ArgumentParser::Vec3, vec![], true),
        lit("say", vec![11], false),
        arg("message", ArgumentParser::Message, vec![], true),
        lit("execute", vec![13, 15], false),
        lit("as", vec![14], false),
        arg("targets", entities(false, false), vec![], false),
        lit("run", vec![], false),
        arg("unused", ArgumentParser::Bool, vec![], true),
        lit("trigger", vec![18], false),
        arg("objective", ArgumentParser::Objective, vec![19, 20], true),
        lit("add", vec![21], false),
        lit("set", vec![21], false),
        arg(
            "value",
            ArgumentParser::Integer {
                min: None,
                max: None,
            },
            vec![],
            true,
        ),
    ];
    nodes[1].restricted = true;
    nodes[4].restricted = true;
    nodes[8].restricted = true;
    nodes[14].redirect = Some(12);
    nodes[15].redirect = Some(0);
    CommandTree::from_packet(&Commands {
        nodes,
        root_index: 0,
    })
}

const ALL: ParseRules = ParseRules {
    restricted: true,
    selectors: true,
};

fn colors(input: &str) -> Vec<(&str, u32)> {
    let parse = tree().parse(input, ALL);
    parse
        .highlight()
        .into_iter()
        .map(|(r, c)| (&input[r], c))
        .collect()
}

fn usage(input: &str) -> Vec<String> {
    let t = tree();
    let parse = t.parse(input, ALL);
    let lang = |key: &str| key.to_owned();
    t.usage_info(&parse, input.len(), true)
        .lines
        .iter()
        .map(|c| plain(c, &lang))
        .collect()
}

fn plain(c: &Component, lang: &dyn Fn(&str) -> String) -> String {
    use mcr_protocol::types::text::Content;
    let mut out = match &c.content {
        Content::Text(t) => t.clone(),
        Content::Translatable { key, args, .. } => {
            let args: Vec<String> = args
                .iter()
                .map(|a| match a {
                    Arg::Str(s) => s.clone(),
                    Arg::Component(c) => plain(c, lang),
                    Arg::Number(_) => "#".into(),
                })
                .collect();
            format!("{}({})", lang(key), args.join(", "))
        }
        _ => String::new(),
    };
    for e in &c.extra {
        out.push_str(&plain(e, lang));
    }
    out
}

#[test]
fn arguments_cycle_colours_and_literals_are_grey() {
    assert_eq!(
        colors("/give @a stone 5"),
        [
            ("/give ", LITERAL_COLOR),
            ("@a", ARGUMENT_COLORS[0]),
            (" ", LITERAL_COLOR),
            ("stone", ARGUMENT_COLORS[1]),
            (" ", LITERAL_COLOR),
            ("5", ARGUMENT_COLORS[2]),
        ]
    );
}

#[test]
fn unparsed_rest_is_red() {
    assert_eq!(
        colors("/give @a stone x"),
        [
            ("/give ", LITERAL_COLOR),
            ("@a", ARGUMENT_COLORS[0]),
            (" ", LITERAL_COLOR),
            ("stone", ARGUMENT_COLORS[1]),
            (" ", LITERAL_COLOR),
            ("x", UNPARSED_COLOR),
        ]
    );
    assert_eq!(
        colors("/nope"),
        [("/", LITERAL_COLOR), ("nope", UNPARSED_COLOR)]
    );
}

#[test]
fn only_the_last_redirected_context_is_coloured() {
    assert_eq!(
        colors("/execute as @e run tp ~ ~1 ^"),
        [
            ("/execute as @e run tp ", LITERAL_COLOR),
            ("~ ~1 ^", UNPARSED_COLOR),
        ]
    );
    assert_eq!(
        colors("/execute as @e run tp ~ ~1 2"),
        [
            ("/execute as @e run tp ", LITERAL_COLOR),
            ("~ ~1 2", ARGUMENT_COLORS[0]),
        ]
    );
}

#[test]
fn usage_follows_the_node_before_the_cursor() {
    assert_eq!(usage("/give @a "), ["<item> [<count>]"]);
    assert_eq!(usage("/gamemode "), ["<gamemode> [<target>]"]);
    assert_eq!(usage("/trigger "), ["<objective> [add|set]"]);
}

#[test]
fn errors_show_with_position_and_context() {
    assert_eq!(
        usage("/give @a stone 0"),
        ["command.context.parse_error(argument.integer.low(1, 0), 15, ... @a stone <--[HERE])"]
    );
    assert_eq!(
        usage("/gamemode sideways"),
        [
            "command.context.parse_error(argument.gamemode.invalid(sideways), 18, ...e sideways<--[HERE])"
        ]
    );
    assert_eq!(
        usage("/tp 1 2"),
        ["command.context.parse_error(argument.pos3d.incomplete(), 4, /tp <--[HERE])"]
    );
}

#[test]
fn unknown_command_reports_at_the_slash() {
    assert_eq!(
        usage("/nope"),
        ["command.context.parse_error(command.unknown.command(), 1, /<--[HERE])"]
    );
}

#[test]
fn selectors_need_permission() {
    let t = tree();
    let rules = ParseRules {
        restricted: true,
        selectors: false,
    };
    let parse = t.parse("/give @a stone", rules);
    assert_eq!(parse.errors.len(), 1);
    assert_eq!(
        parse.errors[0].1.key,
        "argument.entity.selector.not_allowed"
    );
    assert!(t.parse("/give Alex stone", rules).is_valid(&t));
}

#[test]
fn single_player_arguments_reject_entity_selectors() {
    let mut t = tree();
    if let NodeKind::Argument { parser, .. } = &mut t.nodes[3].kind {
        *parser = entities(true, true);
    }
    let e = &t.parse("/gamemode creative @e", ALL).errors[0].1;
    assert_eq!(e.key, "argument.player.toomany");
    assert!(
        t.parse("/gamemode creative @e[limit=1,type=player]", ALL)
            .is_valid(&t)
    );
    assert!(t.parse("/gamemode creative @s", ALL).is_valid(&t));
}

#[test]
fn clicked_commands_are_checked_like_vanilla() {
    let t = tree();
    assert_eq!(t.check("trigger menu", false), CommandCheck::NoIssues);
    assert_eq!(t.check("trigger menu set 3", false), CommandCheck::NoIssues);
    assert_eq!(t.check("trigger", false), CommandCheck::ParseErrors);
    assert_eq!(
        t.check("trigger menu set x", true),
        CommandCheck::ParseErrors
    );
    assert_eq!(t.check("say hi", true), CommandCheck::SignatureRequired);
    assert_eq!(
        t.check("execute as @a run say hi", true),
        CommandCheck::SignatureRequired
    );
    assert_eq!(
        t.check("gamemode creative", true),
        CommandCheck::PermissionsRequired
    );
    assert_eq!(t.check("tp 1 2 3", true), CommandCheck::PermissionsRequired);
    assert_eq!(
        t.check("execute as Alex run", true),
        CommandCheck::ParseErrors
    );
}

#[test]
fn suggestion_context_points_at_the_word_start() {
    let t = tree();
    let parse = t.parse("/give @a st", ALL);
    assert_eq!(parse.context.suggestion_context(11), (5, 9));
    let parse = t.parse("/execute as @a run ", ALL);
    assert_eq!(parse.context.suggestion_context(19), (0, 19));
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
        ..lit("a", vec![], true)
    });
    let t = CommandTree::from_packet(&Commands {
        nodes,
        root_index: 0,
    });
    let long = vec!["a"; 5000].join(" ");
    let parse = t.parse(&long, ALL);
    assert!(parse.cursor < long.len());
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
            true,
        )
    }))
    .collect();
    let t = CommandTree::from_packet(&Commands {
        nodes: wide,
        root_index: 0,
    });
    let parse = t.parse(&vec!["x"; 60].join(" "), ALL);
    assert!(parse.errors.len() <= MAX_ERRORS);
    assert!(!tree().parse("/give @a[name=\"é ü", ALL).errors.is_empty());
    let _ = tree().parse("/tp ^é ^ ^", ALL).highlight();
}
