//! Brigadier's parse over the client command tree: argument ranges, syntax errors, the suggestion
//! context at the cursor and smart usage, as the vanilla client's `CommandSuggestions` uses them.
//!
//! Argument types are read the way their vanilla parsers consume input. Checks that need registry or
//! world data (unknown ids, missing players) are left to the server.

use mcr_protocol::play::clientbound::ui::{ArgumentParser, NodeKind, StringKind};
use mcr_protocol::types::text::{Arg, Color, Component, NamedColor, Style};

use crate::commands::CommandTree;

const MAX_PARSE_WORK: usize = 4096;
const MAX_DEPTH: usize = 64;
const MAX_ERRORS: usize = 32;
const MAX_USAGE_LINES: usize = 64;
const CONTEXT_AMOUNT: usize = 10;
const LITERAL_INCORRECT: &str = "argument.literal.incorrect";

pub const LITERAL_COLOR: u32 = 0xAA_AAAA;
pub const UNPARSED_COLOR: u32 = 0xFF_5555;
/// AQUA, YELLOW, GREEN, LIGHT_PURPLE and GOLD, cycled over the arguments.
pub const ARGUMENT_COLORS: [u32; 5] = [0x55_FFFF, 0xFF_FF55, 0x55_FF55, 0xFF_55FF, 0xFF_AA00];

/// A Brigadier `CommandSyntaxException`: a translation key, its arguments and, when the error carries
/// input context, the byte position it points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub key: &'static str,
    pub args: Vec<String>,
    pub cursor: Option<usize>,
}

impl SyntaxError {
    fn at(key: &'static str, args: Vec<String>, cursor: usize) -> Self {
        Self {
            key,
            args,
            cursor: Some(cursor),
        }
    }

    pub fn is_literal(&self) -> bool {
        self.key == LITERAL_INCORRECT
    }

    /// `CommandSuggestions#getExceptionMessage`: the message, plus the position and the ten characters
    /// before it when the error has context.
    pub fn message(&self, input: &str) -> Component {
        let args = self.args.iter().map(|a| Arg::Str(a.clone())).collect();
        let raw = Component::translatable(self.key, args);
        let Some(cursor) = self.cursor else {
            return raw;
        };
        let cursor = floor_boundary(input, cursor.min(input.len()));
        let before: Vec<char> = input[..cursor].chars().collect();
        let mut context = String::new();
        if before.len() > CONTEXT_AMOUNT {
            context.push_str("...");
        }
        context.extend(&before[before.len().saturating_sub(CONTEXT_AMOUNT)..]);
        context.push_str("<--[HERE]");
        Component::translatable(
            "command.context.parse_error",
            vec![
                Arg::Component(raw),
                Arg::Str(before.len().to_string()),
                Arg::Str(context),
            ],
        )
    }
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Which nodes and selectors a parse may use: vanilla parses once with the player's permissions plus
/// restricted commands, and once with no permissions to decide whether a clicked command needs a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseRules {
    pub restricted: bool,
    pub selectors: bool,
}

impl ParseRules {
    pub const NONE: Self = Self {
        restricted: false,
        selectors: false,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedNode {
    pub node: usize,
    pub start: usize,
    pub end: usize,
}

/// A `CommandContextBuilder`: the nodes parsed from `root`, and the context a redirect continued in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedContext {
    pub root: usize,
    pub start: usize,
    pub end: usize,
    pub nodes: Vec<ParsedNode>,
    /// Argument nodes in first-seen order; a later argument with the same name replaces the value.
    pub arguments: Vec<ParsedNode>,
    pub child: Option<Box<ParsedContext>>,
}

impl ParsedContext {
    fn new(root: usize, start: usize) -> Self {
        Self {
            root,
            start,
            end: start,
            nodes: Vec::new(),
            arguments: Vec::new(),
            child: None,
        }
    }

    pub fn last_child(&self) -> &ParsedContext {
        let mut ctx = self;
        while let Some(child) = &ctx.child {
            ctx = child;
        }
        ctx
    }

    /// `CommandContextBuilder#findSuggestionContext`: the node whose children complete the cursor's
    /// word, and where that word starts.
    pub fn suggestion_context(&self, cursor: usize) -> (usize, usize) {
        let mut ctx = self;
        loop {
            if ctx.end < cursor {
                if let Some(child) = &ctx.child {
                    ctx = child;
                    continue;
                }
                return match ctx.nodes.last() {
                    Some(last) => (last.node, last.end + 1),
                    None => (ctx.root, ctx.start),
                };
            }
            let mut prev = ctx.root;
            for n in &ctx.nodes {
                if n.start <= cursor && cursor <= n.end {
                    return (prev, n.start);
                }
                prev = n.node;
            }
            return (prev, ctx.start);
        }
    }
}

/// `ParseResults`: the chosen context, where the reader stopped and the errors at that point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandParse {
    pub input: String,
    pub context: ParsedContext,
    pub cursor: usize,
    pub errors: Vec<(usize, SyntaxError)>,
}

/// Vanilla `ClientPacketListener.CommandCheckResult`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCheck {
    NoIssues,
    ParseErrors,
    SignatureRequired,
    PermissionsRequired,
}

impl CommandParse {
    pub fn can_read(&self) -> bool {
        self.cursor < self.input.len()
    }

    /// Whole input consumed, no errors, ending on an executable node.
    pub fn is_valid(&self, tree: &CommandTree) -> bool {
        !self.can_read()
            && self.errors.is_empty()
            && self
                .context
                .last_child()
                .nodes
                .last()
                .and_then(|n| tree.nodes.get(n.node))
                .is_some_and(|n| n.executable)
    }

    /// Any `minecraft:message` argument in any context: such commands must be signed.
    pub fn has_message_arguments(&self, tree: &CommandTree) -> bool {
        let mut ctx = Some(&self.context);
        while let Some(c) = ctx {
            let found = c.arguments.iter().any(|a| {
                matches!(
                    tree.nodes.get(a.node).map(|n| &n.kind),
                    Some(NodeKind::Argument {
                        parser: ArgumentParser::Message,
                        ..
                    })
                )
            });
            if found {
                return true;
            }
            ctx = c.child.as_deref();
        }
        false
    }

    /// `CommandSuggestions#formatText`: colour runs over the whole input as (byte range, RGB), literal
    /// text grey, arguments of the last context in cycling colours and the unparsed rest red.
    pub fn highlight(&self) -> Vec<(std::ops::Range<usize>, u32)> {
        let len = self.input.len();
        let mut out = Vec::new();
        let mut unformatted = 0usize;
        let mut push = |range: std::ops::Range<usize>, color: u32| {
            if range.start < range.end {
                out.push((range, color));
            }
        };
        for (i, arg) in self.context.last_child().arguments.iter().enumerate() {
            let start = arg.start.max(unformatted);
            if start >= len {
                break;
            }
            let end = arg.end.min(len);
            if end > start {
                push(unformatted..start, LITERAL_COLOR);
                push(start..end, ARGUMENT_COLORS[i % ARGUMENT_COLORS.len()]);
                unformatted = end;
            }
        }
        if self.can_read() {
            let start = self.cursor.max(unformatted);
            if start < len {
                push(unformatted..start, LITERAL_COLOR);
                push(start..len, UNPARSED_COLOR);
                unformatted = len;
            }
        }
        push(unformatted..len, LITERAL_COLOR);
        out
    }

    /// `Commands#getParseException`.
    pub fn parse_exception(&self) -> Option<SyntaxError> {
        if !self.can_read() {
            return None;
        }
        if self.errors.len() == 1 {
            return Some(self.errors[0].1.clone());
        }
        let key = if self.context.start == self.context.end {
            "command.unknown.command"
        } else {
            "command.unknown.argument"
        };
        Some(SyntaxError::at(key, Vec::new(), self.cursor))
    }
}

/// Usage lines for the command box and the input position they line up with.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageInfo {
    pub lines: Vec<Component>,
    pub start: usize,
}

fn gray(text: String) -> Component {
    Component::text(text).styled(Style {
        color: Some(Color::Named(NamedColor::Gray)),
        ..Style::default()
    })
}

struct Reader<'a> {
    s: &'a str,
    pos: usize,
}

fn is_unquoted(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'+')
}

fn is_number(c: u8) -> bool {
    c.is_ascii_digit() || c == b'.' || c == b'-'
}

fn is_identifier(c: u8) -> bool {
    c.is_ascii_digit() || c.is_ascii_lowercase() || matches!(c, b'_' | b':' | b'/' | b'.' | b'-')
}

type Parsed<T> = Result<T, SyntaxError>;

impl<'a> Reader<'a> {
    fn can_read(&self) -> bool {
        self.pos < self.s.len()
    }

    fn peek(&self) -> u8 {
        self.s.as_bytes()[self.pos]
    }

    fn peek_is(&self, c: u8) -> bool {
        self.can_read() && self.peek() == c
    }

    fn at_separator(&self) -> bool {
        !self.can_read() || self.peek() == b' '
    }

    fn skip(&mut self) {
        self.pos += self.s[self.pos..].chars().next().map_or(1, char::len_utf8);
    }

    fn err(&self, key: &'static str, args: Vec<String>) -> SyntaxError {
        SyntaxError::at(key, args, self.pos)
    }

    fn read_while(&mut self, f: impl Fn(u8) -> bool) -> &'a str {
        let start = self.pos;
        while self.can_read() && f(self.peek()) {
            self.pos += 1;
        }
        &self.s[start..self.pos]
    }

    fn read_until_space(&mut self) -> &'a str {
        let start = self.pos;
        while self.can_read() && self.peek() != b' ' {
            self.skip();
        }
        &self.s[start..self.pos]
    }

    fn read_unquoted(&mut self) -> &'a str {
        self.read_while(is_unquoted)
    }

    fn read_number<T: std::str::FromStr>(
        &mut self,
        expected: &'static str,
        invalid: &'static str,
    ) -> Parsed<T> {
        let start = self.pos;
        let text = self.read_while(is_number);
        if text.is_empty() {
            return Err(self.err(expected, Vec::new()));
        }
        text.parse().map_err(|_| {
            self.pos = start;
            self.err(invalid, vec![text.to_owned()])
        })
    }

    fn read_until_quote(&mut self, quote: u8) -> Parsed<()> {
        while self.can_read() {
            let c = self.peek();
            self.skip();
            if c == b'\\' {
                if !self.can_read() {
                    break;
                }
                let e = self.peek();
                if e != quote && e != b'\\' {
                    let ch = self.s[self.pos..].chars().next().unwrap_or(' ');
                    return Err(self.err("parsing.quote.escape", vec![ch.to_string()]));
                }
                self.skip();
            } else if c == quote {
                return Ok(());
            }
        }
        Err(self.err("parsing.quote.expected.end", Vec::new()))
    }

    /// Brigadier `readString`: a quoted string, or an unquoted word.
    fn read_string(&mut self) -> Parsed<&'a str> {
        let start = self.pos;
        if self.can_read() && matches!(self.peek(), b'"' | b'\'') {
            let quote = self.peek();
            self.skip();
            self.read_until_quote(quote)?;
            return Ok(&self.s[start + 1..self.pos - 1]);
        }
        Ok(self.read_unquoted())
    }

    fn read_bool(&mut self) -> Parsed<()> {
        let start = self.pos;
        let value = self.read_string()?;
        match value {
            "" => Err(self.err("parsing.bool.expected", Vec::new())),
            "true" | "false" => Ok(()),
            other => {
                self.pos = start;
                Err(self.err("parsing.bool.invalid", vec![other.to_owned()]))
            }
        }
    }

    /// One enumerated word, as `readUnquotedString` plus a lookup.
    fn read_keyword(&mut self, valid: &[&str], key: &'static str) -> Parsed<()> {
        let word = self.read_unquoted();
        if valid.contains(&word) {
            Ok(())
        } else {
            Err(self.err(key, vec![word.to_owned()]))
        }
    }

    /// `Identifier.read`: identifier characters, then a namespace and path check.
    fn read_identifier(&mut self) -> Parsed<()> {
        let start = self.pos;
        let raw = self.read_while(is_identifier);
        let valid = match raw.split_once(':') {
            Some((namespace, path)) => !namespace.contains('/') && !path.contains(':'),
            None => true,
        };
        if valid {
            Ok(())
        } else {
            self.pos = start;
            Err(self.err("argument.id.invalid", Vec::new()))
        }
    }

    /// One bracketed or quoted value, nested brackets and quotes included; unterminated input is left
    /// for the server to reject.
    fn skip_balanced(&mut self) -> Parsed<()> {
        let mut depth = 0usize;
        while self.can_read() {
            match self.peek() {
                q @ (b'"' | b'\'') => {
                    self.skip();
                    self.read_until_quote(q)?;
                    if depth == 0 {
                        return Ok(());
                    }
                    continue;
                }
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.skip();
                        return Ok(());
                    }
                }
                b' ' if depth == 0 => return Ok(()),
                _ => {}
            }
            self.skip();
        }
        Ok(())
    }

    /// An SNBT value: compound, list, quoted string or bare word.
    fn read_snbt(&mut self) -> Parsed<()> {
        if !self.can_read() {
            return Err(self.err("argument.nbt.expected.value", Vec::new()));
        }
        match self.peek() {
            b'{' | b'[' | b'"' | b'\'' => self.skip_balanced(),
            _ => {
                self.read_until_space();
                Ok(())
            }
        }
    }

    /// An inline SNBT value or a registry id (`ResourceOrIdArgument`).
    fn read_id_or_inline(&mut self) -> Parsed<()> {
        if self.can_read() && matches!(self.peek(), b'{' | b'[' | b'"' | b'\'') {
            self.skip_balanced()
        } else {
            self.read_identifier()
        }
    }

    /// An item or block id with optional `[...]` and `{...}` parts.
    fn read_stack(&mut self, tags: bool, wildcard: bool) -> Parsed<()> {
        if tags && self.peek_is(b'#') {
            self.skip();
        }
        if wildcard && self.peek_is(b'*') {
            self.skip();
        } else {
            self.read_identifier()?;
        }
        if self.peek_is(b'[') {
            self.skip_balanced()?;
        }
        if self.peek_is(b'{') {
            self.skip_balanced()?;
        }
        Ok(())
    }

    fn read_coordinate(&mut self, int: bool) -> Parsed<()> {
        if self.peek_is(b'^') {
            return Err(self.err("argument.pos.mixed", Vec::new()));
        }
        if !self.can_read() {
            let key = if int {
                "argument.pos.missing.int"
            } else {
                "argument.pos.missing.double"
            };
            return Err(self.err(key, Vec::new()));
        }
        let relative = self.peek() == b'~';
        if relative {
            self.skip();
        }
        if self.at_separator() {
            return Ok(());
        }
        if int && !relative {
            self.read_number::<i32>("parsing.int.expected", "parsing.int.invalid")?;
        } else {
            self.read_number::<f64>("parsing.double.expected", "parsing.double.invalid")?;
        }
        Ok(())
    }

    fn read_local_coordinate(&mut self, start: usize) -> Parsed<()> {
        if !self.can_read() {
            return Err(self.err("argument.pos.missing.double", Vec::new()));
        }
        if self.peek() != b'^' {
            self.pos = start;
            return Err(self.err("argument.pos.mixed", Vec::new()));
        }
        self.skip();
        if !self.at_separator() {
            self.read_number::<f64>("parsing.double.expected", "parsing.double.invalid")?;
        }
        Ok(())
    }

    /// `WorldCoordinates`/`LocalCoordinates`: `count` coordinates separated by single spaces.
    fn read_coordinates(
        &mut self,
        count: usize,
        int: bool,
        local: bool,
        incomplete: &'static str,
    ) -> Parsed<()> {
        let start = self.pos;
        let local = local && self.peek_is(b'^');
        for i in 0..count {
            if i > 0 {
                if !self.peek_is(b' ') {
                    self.pos = start;
                    return Err(self.err(incomplete, Vec::new()));
                }
                self.skip();
            }
            if local {
                self.read_local_coordinate(start)?;
            } else {
                self.read_coordinate(int)?;
            }
        }
        Ok(())
    }

    /// `EntitySelectorParser#parse`, returning (max results above one, includes non-players,
    /// self selector).
    fn read_selector(&mut self, allowed: bool) -> Parsed<(bool, bool, bool)> {
        if self.peek_is(b'@') {
            if !allowed {
                return Err(self.err("argument.entity.selector.not_allowed", Vec::new()));
            }
            self.skip();
            if !self.can_read() {
                return Err(self.err("argument.entity.selector.missing", Vec::new()));
            }
            let start = self.pos;
            let kind = self.s[self.pos..].chars().next().unwrap_or(' ');
            self.skip();
            let (mut many, mut entities, own) = match kind {
                'a' => (true, false, false),
                'e' => (true, true, false),
                'n' => (false, true, false),
                'p' | 'r' => (false, false, false),
                's' => (false, true, true),
                other => {
                    self.pos = start;
                    return Err(self.err(
                        "argument.entity.selector.unknown",
                        vec![format!("@{other}")],
                    ));
                }
            };
            if self.peek_is(b'[') {
                let open = self.pos;
                self.skip_options()?;
                let options = &self.s[open..self.pos];
                if options.contains("limit=1,") || options.contains("limit=1]") {
                    many = false;
                }
                if options.contains("type=player") || options.contains("type=minecraft:player") {
                    entities = false;
                }
            }
            return Ok((many, entities, own));
        }
        let start = self.pos;
        let name = self.read_string()?;
        if is_uuid(name) {
            return Ok((false, true, false));
        }
        if name.is_empty() || name.chars().count() > 16 {
            self.pos = start;
            return Err(self.err("argument.entity.invalid", Vec::new()));
        }
        Ok((false, false, false))
    }

    fn skip_options(&mut self) -> Parsed<()> {
        let mut depth = 0usize;
        while self.can_read() {
            match self.peek() {
                q @ (b'"' | b'\'') => {
                    self.skip();
                    self.read_until_quote(q)?;
                    continue;
                }
                b'[' | b'{' => depth += 1,
                b']' | b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        self.skip();
                        return Ok(());
                    }
                }
                _ => {}
            }
            self.skip();
        }
        Err(self.err("argument.entity.options.unterminated", Vec::new()))
    }

    fn read_range(&mut self, ints: bool) -> Parsed<()> {
        let start = self.pos;
        let text = self.read_while(is_number);
        if text.is_empty() || text == ".." {
            self.pos = start;
            return Err(self.err("argument.range.empty", Vec::new()));
        }
        if ints && text.replace("..", "").contains('.') {
            self.pos = start;
            return Err(self.err("argument.range.ints", Vec::new()));
        }
        Ok(())
    }
}

fn is_uuid(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    parts.len() == 5
        && text.len() <= 36
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn java_float(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e7 {
        format!("{v:.1}")
    } else {
        v.to_string()
    }
}

const GAME_MODES: [&str; 4] = ["survival", "creative", "adventure", "spectator"];
const ANCHORS: [&str; 2] = ["feet", "eyes"];
const OPERATIONS: [&str; 9] = ["=", "+=", "-=", "*=", "/=", "%=", "<", ">", "><"];
const MIRRORS: [&str; 3] = ["none", "left_right", "front_back"];
const ROTATIONS: [&str; 4] = ["none", "clockwise_90", "180", "counterclockwise_90"];
const HEIGHTMAPS: [&str; 4] = [
    "world_surface",
    "motion_blocking",
    "motion_blocking_no_leaves",
    "ocean_floor",
];
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

fn parse_argument(parser: &ArgumentParser, r: &mut Reader<'_>, rules: ParseRules) -> Parsed<()> {
    use ArgumentParser as P;
    let start = r.pos;
    macro_rules! bounded {
        ($ty:ty, $expected:literal, $invalid:literal, $low:literal, $big:literal, $min:expr, $max:expr, $fmt:expr) => {{
            let v: $ty = r.read_number($expected, $invalid)?;
            if let Some(min) = $min
                && v < min
            {
                r.pos = start;
                return Err(r.err($low, vec![$fmt(min), $fmt(v)]));
            }
            if let Some(max) = $max
                && v > max
            {
                r.pos = start;
                return Err(r.err($big, vec![$fmt(max), $fmt(v)]));
            }
            Ok(())
        }};
    }
    match parser {
        P::Bool => r.read_bool(),
        P::Integer { min, max } => bounded!(
            i32,
            "parsing.int.expected",
            "parsing.int.invalid",
            "argument.integer.low",
            "argument.integer.big",
            *min,
            *max,
            |v: i32| v.to_string()
        ),
        P::Long { min, max } => bounded!(
            i64,
            "parsing.long.expected",
            "parsing.long.invalid",
            "argument.long.low",
            "argument.long.big",
            *min,
            *max,
            |v: i64| v.to_string()
        ),
        P::Float { min, max } => bounded!(
            f32,
            "parsing.float.expected",
            "parsing.float.invalid",
            "argument.float.low",
            "argument.float.big",
            *min,
            *max,
            |v: f32| java_float(f64::from(v))
        ),
        P::Double { min, max } => bounded!(
            f64,
            "parsing.double.expected",
            "parsing.double.invalid",
            "argument.double.low",
            "argument.double.big",
            *min,
            *max,
            java_float
        ),
        P::String(StringKind::SingleWord) => {
            r.read_unquoted();
            Ok(())
        }
        P::String(StringKind::QuotablePhrase) => r.read_string().map(|_| ()),
        P::String(StringKind::GreedyPhrase) | P::Message => {
            r.pos = r.s.len();
            Ok(())
        }
        P::Entity {
            single,
            players_only,
        } => {
            let (many, entities, own) = r.read_selector(rules.selectors)?;
            if many && *single {
                let key = if *players_only {
                    "argument.player.toomany"
                } else {
                    "argument.entity.toomany"
                };
                return Err(SyntaxError::at(key, Vec::new(), 0));
            }
            if entities && *players_only && !own {
                return Err(SyntaxError::at("argument.player.entities", Vec::new(), 0));
            }
            Ok(())
        }
        P::GameProfile => {
            if r.peek_is(b'@') {
                let (_, entities, _) = r.read_selector(rules.selectors)?;
                if entities {
                    return Err(r.err("argument.player.entities", Vec::new()));
                }
            } else {
                r.read_until_space();
            }
            Ok(())
        }
        P::ScoreHolder { .. } => {
            if r.peek_is(b'@') {
                r.read_selector(rules.selectors)?;
            } else {
                r.read_until_space();
            }
            Ok(())
        }
        P::BlockPos => r.read_coordinates(3, true, true, "argument.pos3d.incomplete"),
        P::Vec3 => r.read_coordinates(3, false, true, "argument.pos3d.incomplete"),
        P::ColumnPos => r.read_coordinates(2, true, false, "argument.pos2d.incomplete"),
        P::Vec2 => r.read_coordinates(2, false, false, "argument.pos2d.incomplete"),
        P::Rotation => r.read_coordinates(2, false, false, "argument.rotation.incomplete"),
        P::Angle => {
            if !r.can_read() {
                return Err(r.err("argument.angle.incomplete", Vec::new()));
            }
            if r.peek() == b'~' {
                r.skip();
            }
            if !r.at_separator() {
                r.read_number::<f32>("parsing.float.expected", "parsing.float.invalid")?;
            }
            Ok(())
        }
        P::BlockState | P::Particle => r.read_stack(false, false),
        P::BlockPredicate => r.read_stack(true, false),
        P::ItemStack => r.read_stack(false, false),
        P::ItemPredicate => r.read_stack(true, true),
        P::TeamColor => r.read_keyword(&COLORS, "argument.color.invalid"),
        P::HexColor => {
            let text = r.read_unquoted();
            if matches!(text.len(), 3 | 6) && text.bytes().all(|b| b.is_ascii_hexdigit()) {
                Ok(())
            } else {
                Err(r.err("argument.hexcolor.invalid", vec![text.to_owned()]))
            }
        }
        P::Component | P::Style | P::NbtTag => r.read_snbt(),
        P::NbtCompoundTag => {
            if !r.peek_is(b'{') {
                return Err(r.err("argument.nbt.expected.compound", Vec::new()));
            }
            r.skip_balanced()
        }
        P::NbtPath => {
            while r.can_read() && r.peek() != b' ' {
                r.skip_balanced()?;
            }
            Ok(())
        }
        P::Objective | P::Team | P::ItemSlot | P::ItemSlots => {
            r.read_unquoted();
            Ok(())
        }
        P::ObjectiveCriteria | P::ResourceSelector(_) | P::SwingAnimation => {
            r.read_until_space();
            Ok(())
        }
        P::Operation => {
            let op = r.read_until_space();
            if OPERATIONS.contains(&op) {
                Ok(())
            } else {
                r.pos = start;
                Err(r.err("arguments.operation.invalid", Vec::new()))
            }
        }
        P::ScoreboardSlot => {
            let name = r.read_unquoted();
            let valid = matches!(name, "list" | "sidebar" | "below_name")
                || name
                    .strip_prefix("sidebar.team.")
                    .is_some_and(|c| COLORS[..16].contains(&c));
            if valid {
                Ok(())
            } else {
                Err(r.err(
                    "argument.scoreboardDisplaySlot.invalid",
                    vec![name.to_owned()],
                ))
            }
        }
        P::Swizzle => {
            let mut seen = 0u8;
            while r.can_read() && r.peek() != b' ' {
                let bit = match r.peek() {
                    b'x' => 1,
                    b'y' => 2,
                    b'z' => 4,
                    _ => return Err(r.err("arguments.swizzle.invalid", Vec::new())),
                };
                if seen & bit != 0 {
                    return Err(r.err("arguments.swizzle.invalid", Vec::new()));
                }
                seen |= bit;
                r.skip();
            }
            Ok(())
        }
        P::ResourceLocation | P::Dimension | P::Resource(_) | P::ResourceKey(_) => {
            r.read_identifier()
        }
        P::Function | P::ResourceOrTag(_) | P::ResourceOrTagKey(_) => {
            if r.peek_is(b'#') {
                r.skip();
            }
            r.read_identifier()
        }
        P::LootTable | P::LootPredicate | P::LootModifier | P::Dialog => r.read_id_or_inline(),
        P::ContextFloatProvider | P::ContextIntProvider | P::SlotSource | P::Feature => {
            if r.can_read() && matches!(r.peek(), b'{' | b'[' | b'"' | b'\'') {
                r.skip_balanced()
            } else {
                r.read_until_space();
                Ok(())
            }
        }
        P::EntityAnchor => r.read_keyword(&ANCHORS, "argument.anchor.invalid"),
        P::IntRange => r.read_range(true),
        P::FloatRange => r.read_range(false),
        P::Gamemode => r.read_keyword(&GAME_MODES, "argument.gamemode.invalid"),
        P::Time { min } => {
            let value: f32 = r.read_number("parsing.float.expected", "parsing.float.invalid")?;
            let scale = match r.read_unquoted() {
                "" | "t" => 1.0,
                "s" => 20.0,
                "d" => 24000.0,
                _ => return Err(r.err("argument.time.invalid_unit", Vec::new())),
            };
            let ticks = (value * scale).round() as i32;
            if ticks < *min {
                return Err(r.err(
                    "argument.time.tick_count_too_low",
                    vec![min.to_string(), ticks.to_string()],
                ));
            }
            Ok(())
        }
        P::TemplateMirror => r.read_keyword(&MIRRORS, "argument.enum.invalid"),
        P::TemplateRotation => r.read_keyword(&ROTATIONS, "argument.enum.invalid"),
        P::Heightmap => r.read_keyword(&HEIGHTMAPS, "argument.enum.invalid"),
        P::Uuid => {
            let text = r.read_while(|b| b.is_ascii_hexdigit() || b == b'-');
            if is_uuid(text) {
                Ok(())
            } else {
                Err(r.err("argument.uuid.invalid", Vec::new()))
            }
        }
    }
}

impl CommandTree {
    pub(crate) fn own_children(&self, node: usize) -> impl Iterator<Item = usize> + '_ {
        let len = self.nodes.len();
        self.nodes
            .get(node)
            .map(|n| n.children.as_slice())
            .unwrap_or_default()
            .iter()
            .filter_map(move |&c| usize::try_from(c).ok().filter(|&c| c < len))
    }

    fn redirect_of(&self, node: usize) -> Option<usize> {
        self.nodes[node]
            .redirect
            .and_then(|r| usize::try_from(r).ok())
            .filter(|&r| r < self.nodes.len())
    }

    /// `CommandNode#getRelevantNodes`: the literal matching the next word, else every argument child.
    fn relevant(&self, node: usize, input: &str, pos: usize) -> Vec<usize> {
        let word_end = input[pos..].find(' ').map_or(input.len(), |i| pos + i);
        let word = &input[pos..word_end];
        let literal = self
            .own_children(node)
            .filter(|&c| matches!(&self.nodes[c].kind, NodeKind::Literal(l) if l == word));
        if let Some(l) = literal.last() {
            return vec![l];
        }
        self.own_children(node)
            .filter(|&c| matches!(self.nodes[c].kind, NodeKind::Argument { .. }))
            .collect()
    }

    fn parse_node(&self, node: usize, input: &str, pos: usize, rules: ParseRules) -> Parsed<usize> {
        match &self.nodes[node].kind {
            NodeKind::Literal(l) => {
                let end = pos + l.len();
                let fits = input[pos..].starts_with(l.as_str())
                    && (end == input.len() || input.as_bytes()[end] == b' ');
                if fits {
                    Ok(end)
                } else {
                    Err(SyntaxError::at(LITERAL_INCORRECT, vec![l.clone()], pos))
                }
            }
            NodeKind::Argument { parser, .. } => {
                let mut r = Reader { s: input, pos };
                parse_argument(parser, &mut r, rules)?;
                Ok(r.pos)
            }
            NodeKind::Root => Err(SyntaxError::at(LITERAL_INCORRECT, Vec::new(), pos)),
        }
    }

    /// Brigadier's `CommandDispatcher#parse` of `input` from byte `start` (past an optional `/`).
    pub fn parse_from(&self, input: &str, start: usize, rules: ParseRules) -> CommandParse {
        let start = floor_boundary(input, start.min(input.len()));
        let ctx = ParsedContext::new(self.root, start);
        let mut work = MAX_PARSE_WORK;
        let (context, cursor, errors) = if self.nodes.is_empty() {
            (ctx, start, Vec::new())
        } else {
            self.parse_nodes(input, self.root, start, &ctx, rules, &mut work, 0)
        };
        CommandParse {
            input: input.to_owned(),
            context,
            cursor,
            errors,
        }
    }

    /// Parses chat-box text: a leading `/` is skipped like `CommandSuggestions#updateCommandInfo`.
    pub fn parse(&self, input: &str, rules: ParseRules) -> CommandParse {
        self.parse_from(input, usize::from(input.starts_with('/')), rules)
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_nodes(
        &self,
        input: &str,
        node: usize,
        pos: usize,
        ctx: &ParsedContext,
        rules: ParseRules,
        work: &mut usize,
        depth: usize,
    ) -> (ParsedContext, usize, Vec<(usize, SyntaxError)>) {
        let mut errors = Vec::new();
        let mut potentials = Vec::new();
        let children = if depth < MAX_DEPTH {
            self.relevant(node, input, pos)
        } else {
            Vec::new()
        };
        for child in children {
            if self.nodes[child].restricted && !rules.restricted {
                continue;
            }
            if *work == 0 {
                break;
            }
            *work -= 1;
            let end = self.parse_node(child, input, pos, rules).and_then(|end| {
                match input.as_bytes().get(end) {
                    Some(&c) if c != b' ' => Err(SyntaxError::at(
                        "command.expected.separator",
                        Vec::new(),
                        end,
                    )),
                    _ => Ok(end),
                }
            });
            let end = match end {
                Ok(end) => end,
                Err(e) => {
                    if errors.len() < MAX_ERRORS {
                        errors.push((child, e));
                    }
                    continue;
                }
            };
            let mut c = ctx.clone();
            let parsed = ParsedNode {
                node: child,
                start: pos,
                end,
            };
            c.nodes.push(parsed);
            c.end = c.end.max(end);
            if let NodeKind::Argument { name, .. } = &self.nodes[child].kind {
                let same = c.arguments.iter().position(|a| {
                    matches!(&self.nodes[a.node].kind, NodeKind::Argument { name: n, .. } if n == name)
                });
                match same {
                    Some(i) => c.arguments[i] = parsed,
                    None => c.arguments.push(parsed),
                }
            }
            let redirect = self.redirect_of(child);
            let need = if redirect.is_some() { 1 } else { 2 };
            if end + need <= input.len() {
                let next = end + 1;
                match redirect {
                    Some(target) => {
                        let sub = ParsedContext::new(target, next);
                        let (sub, cursor, errs) =
                            self.parse_nodes(input, target, next, &sub, rules, work, depth + 1);
                        c.child = Some(Box::new(sub));
                        potentials.push((c, cursor, errs));
                    }
                    None => {
                        potentials.push(self.parse_nodes(
                            input,
                            child,
                            next,
                            &c,
                            rules,
                            work,
                            depth + 1,
                        ));
                    }
                }
            } else {
                potentials.push((c, end, Vec::new()));
            }
        }
        if potentials.is_empty() {
            return (ctx.clone(), pos, errors);
        }
        let len = input.len();
        potentials.sort_by_key(|(_, cursor, errs)| (*cursor < len, !errs.is_empty()));
        potentials.swap_remove(0)
    }

    fn usage_text(&self, node: usize) -> String {
        match &self.nodes[node].kind {
            NodeKind::Literal(l) => l.clone(),
            NodeKind::Argument { name, .. } => format!("<{name}>"),
            NodeKind::Root => String::new(),
        }
    }

    fn smart_usage_of(&self, node: usize, optional: bool, deep: bool) -> String {
        let own = self.usage_text(node);
        let this = if optional { format!("[{own}]") } else { own };
        if deep {
            return this;
        }
        let child_optional = self.nodes[node].executable;
        if let Some(target) = self.redirect_of(node) {
            let redirect = if target == self.root {
                "...".to_owned()
            } else {
                format!("-> {}", self.usage_text(target))
            };
            return format!("{this} {redirect}");
        }
        let children: Vec<usize> = self.own_children(node).collect();
        match children.len() {
            0 => this,
            1 => format!(
                "{this} {}",
                self.smart_usage_of(children[0], child_optional, child_optional)
            ),
            _ => {
                let mut texts: Vec<String> = Vec::new();
                for &c in &children {
                    let t = self.usage_text(c);
                    if !texts.contains(&t) {
                        texts.push(t);
                    }
                }
                if texts.len() == 1 {
                    let usage = &texts[0];
                    if child_optional {
                        format!("{this} [{usage}]")
                    } else {
                        format!("{this} {usage}")
                    }
                } else {
                    let all: Vec<String> = children.iter().map(|&c| self.usage_text(c)).collect();
                    let (open, close) = if child_optional {
                        ('[', ']')
                    } else {
                        ('(', ')')
                    };
                    format!("{this} {open}{}{close}", all.join("|"))
                }
            }
        }
    }

    /// `CommandDispatcher#getSmartUsage` for the argument children of `node`, as
    /// `CommandSuggestions#fillNodeUsage` lists them.
    pub fn smart_usage(&self, node: usize) -> Vec<String> {
        if node >= self.nodes.len() {
            return Vec::new();
        }
        let optional = self.nodes[node].executable;
        self.own_children(node)
            .filter(|&c| matches!(self.nodes[c].kind, NodeKind::Argument { .. }))
            .take(MAX_USAGE_LINES)
            .map(|c| self.smart_usage_of(c, optional, false))
            .collect()
    }

    /// `CommandSuggestions#updateUsageInfo`: errors when the cursor ends the input and nothing
    /// completes it, otherwise the usage of the node before the cursor.
    pub fn usage_info(
        &self,
        parse: &CommandParse,
        cursor: usize,
        no_suggestions: bool,
    ) -> UsageInfo {
        let mut lines = Vec::new();
        let input = parse.input.as_str();
        let mut trailing = false;
        if cursor == input.len() {
            if no_suggestions && !parse.errors.is_empty() {
                let mut literals = 0;
                for (_, e) in &parse.errors {
                    if e.is_literal() {
                        literals += 1;
                    } else if lines.len() < MAX_USAGE_LINES {
                        lines.push(e.message(input));
                    }
                }
                if literals > 0 {
                    let e = SyntaxError::at("command.unknown.argument", Vec::new(), parse.cursor);
                    lines.push(e.message(input));
                }
            } else if parse.can_read() {
                trailing = true;
            }
        }
        let (parent, start) = parse.context.suggestion_context(cursor);
        if lines.is_empty() {
            let usage = self.smart_usage(parent);
            if usage.is_empty()
                && trailing
                && let Some(e) = parse.parse_exception()
            {
                lines.push(e.message(input));
            }
            lines.extend(usage.into_iter().map(gray));
        }
        UsageInfo { lines, start }
    }

    /// `ClientPacketListener#verifyCommand` for a command clicked in chat, a book or a dialog.
    pub fn check(&self, command: &str, selectors: bool) -> CommandCheck {
        let full = self.parse_from(
            command,
            0,
            ParseRules {
                restricted: true,
                selectors,
            },
        );
        if !full.is_valid(self) {
            return CommandCheck::ParseErrors;
        }
        if full.has_message_arguments(self) {
            return CommandCheck::SignatureRequired;
        }
        if !self.parse_from(command, 0, ParseRules::NONE).is_valid(self) {
            return CommandCheck::PermissionsRequired;
        }
        CommandCheck::NoIssues
    }
}

#[cfg(test)]
#[path = "brigadier_tests.rs"]
mod tests;
