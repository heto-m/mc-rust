use mcr_buf::{Decode, DecodeError, Encode, Identifier, Result};
use mcr_nbt::{Compound, Limits, List, Value, read_network, write_network};
use uuid::Uuid;

pub trait Translate {
    fn translate(&self, key: &str) -> Option<&str>;
}

struct NoTranslations;

impl Translate for NoTranslations {
    fn translate(&self, _key: &str) -> Option<&str> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("invalid text component: {0}")]
pub struct ComponentError(pub String);

type ParseResult<T> = std::result::Result<T, ComponentError>;

fn err<T>(msg: impl Into<String>) -> ParseResult<T> {
    Err(ComponentError(msg.into()))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub content: Content,
    pub style: Style,
    pub extra: Vec<Component>,
}

impl Default for Component {
    fn default() -> Self {
        Self::text("")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    Text(String),
    Translatable {
        key: String,
        fallback: Option<String>,
        args: Vec<Arg>,
    },
    Keybind(String),
    Score {
        name: String,
        objective: String,
    },
    Selector {
        selector: String,
        separator: Option<Box<Component>>,
    },
    Nbt(Box<NbtContent>),
    Object {
        object: ObjectContent,
        fallback: Option<Box<Component>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Str(String),
    Number(Value),
    Component(Component),
}

#[derive(Debug, Clone, PartialEq)]
pub struct NbtContent {
    pub path: String,
    pub interpret: bool,
    pub plain: bool,
    pub separator: Option<Box<Component>>,
    pub source: NbtSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NbtSource {
    Entity(String),
    Block(String),
    Storage(Identifier),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectContent {
    Atlas {
        atlas: Identifier,
        sprite: Identifier,
    },
    /// `player` is the raw resolvable-profile NBT.
    Player { player: Value, hat: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedColor {
    Black,
    DarkBlue,
    DarkGreen,
    DarkAqua,
    DarkRed,
    DarkPurple,
    Gold,
    Gray,
    DarkGray,
    Blue,
    Green,
    Aqua,
    Red,
    LightPurple,
    Yellow,
    White,
}

impl NamedColor {
    const ALL: [(Self, &'static str, u32); 16] = [
        (Self::Black, "black", 0x000000),
        (Self::DarkBlue, "dark_blue", 0x0000AA),
        (Self::DarkGreen, "dark_green", 0x00AA00),
        (Self::DarkAqua, "dark_aqua", 0x00AAAA),
        (Self::DarkRed, "dark_red", 0xAA0000),
        (Self::DarkPurple, "dark_purple", 0xAA00AA),
        (Self::Gold, "gold", 0xFFAA00),
        (Self::Gray, "gray", 0xAAAAAA),
        (Self::DarkGray, "dark_gray", 0x555555),
        (Self::Blue, "blue", 0x5555FF),
        (Self::Green, "green", 0x55FF55),
        (Self::Aqua, "aqua", 0x55FFFF),
        (Self::Red, "red", 0xFF5555),
        (Self::LightPurple, "light_purple", 0xFF55FF),
        (Self::Yellow, "yellow", 0xFFFF55),
        (Self::White, "white", 0xFFFFFF),
    ];

    fn row(self) -> &'static (Self, &'static str, u32) {
        Self::ALL.iter().find(|r| r.0 == self).unwrap()
    }

    pub fn name(self) -> &'static str {
        self.row().1
    }

    pub fn rgb(self) -> u32 {
        self.row().2
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().find(|r| r.1 == name).map(|r| r.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    Named(NamedColor),
    Rgb(u32),
}

impl Color {
    pub fn rgb(self) -> u32 {
        match self {
            Self::Named(n) => n.rgb(),
            Self::Rgb(v) => v,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.strip_prefix('#') {
            Some(hex) => u32::from_str_radix(hex, 16)
                .ok()
                .filter(|&v| v <= 0xFF_FFFF && !hex.starts_with(['+', '-']))
                .map(Self::Rgb),
            None => NamedColor::from_name(s).map(Self::Named),
        }
    }

    pub fn serialize(self) -> String {
        match self {
            Self::Named(n) => n.name().to_owned(),
            Self::Rgb(v) => format!("#{v:06X}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Style {
    pub color: Option<Color>,
    pub shadow_color: Option<i32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underlined: Option<bool>,
    pub strikethrough: Option<bool>,
    pub obfuscated: Option<bool>,
    pub click_event: Option<Box<ClickEvent>>,
    pub hover_event: Option<Box<HoverEvent>>,
    pub insertion: Option<String>,
    pub font: Option<Identifier>,
}

impl Style {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClickEvent {
    OpenUrl(String),
    OpenFile(String),
    RunCommand(String),
    SuggestCommand(String),
    /// Raw dialog reference: an id string or an inline dialog compound.
    ShowDialog(Value),
    ChangePage(i32),
    CopyToClipboard(String),
    Custom {
        id: Identifier,
        payload: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum HoverEvent {
    ShowText(Box<Component>),
    /// The item template fields (`id`, `count`, `components`) without the `action` key.
    ShowItem(Compound),
    ShowEntity {
        entity: Identifier,
        uuid: Uuid,
        name: Option<Box<Component>>,
    },
}

impl Component {
    pub fn text(text: impl Into<String>) -> Self {
        Self::from_content(Content::Text(text.into()))
    }

    pub fn translatable(key: impl Into<String>, args: Vec<Arg>) -> Self {
        Self::from_content(Content::Translatable {
            key: key.into(),
            fallback: None,
            args,
        })
    }

    pub fn from_content(content: Content) -> Self {
        Self {
            content,
            style: Style::default(),
            extra: Vec::new(),
        }
    }

    #[must_use]
    pub fn styled(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    #[must_use]
    pub fn with(mut self, sibling: Component) -> Self {
        self.extra.push(sibling);
        self
    }

    pub fn to_plain_string(&self) -> String {
        self.to_plain_string_with(&NoTranslations)
    }

    /// Vanilla's `getString`: translatables resolve through `lookup`, then the fallback, then the key.
    pub fn to_plain_string_with(&self, lookup: &dyn Translate) -> String {
        let mut out = String::new();
        let mut budget = PlainBudget {
            steps: MAX_PLAIN_STEPS,
            depth: 0,
        };
        self.write_plain(lookup, &mut budget, &mut out);
        out
    }

    fn write_plain(&self, lookup: &dyn Translate, budget: &mut PlainBudget, out: &mut String) {
        if !budget.enter(out) {
            return;
        }
        match &self.content {
            Content::Text(t) => out.push_str(t),
            Content::Translatable {
                key,
                fallback,
                args,
            } => {
                let template = lookup.translate(key).or(fallback.as_deref()).unwrap_or(key);
                write_translation(template, args, lookup, budget, out);
            }
            Content::Keybind(k) => out.push_str(k),
            Content::Score { .. } | Content::Nbt(_) => {}
            Content::Selector { selector, .. } => out.push_str(selector),
            Content::Object { fallback, .. } => {
                if let Some(f) = fallback {
                    f.write_plain(lookup, budget, out);
                }
            }
        }
        for sibling in &self.extra {
            sibling.write_plain(lookup, budget, out);
        }
        budget.depth -= 1;
    }

    pub fn to_nbt(&self) -> Value {
        if let Content::Text(t) = &self.content
            && self.extra.is_empty()
            && self.style.is_empty()
        {
            return Value::String(t.clone());
        }
        let mut c = Compound::new();
        write_content(&self.content, &mut c);
        if !self.extra.is_empty() {
            let values = self.extra.iter().map(Component::to_nbt).collect();
            c.insert("extra", Value::List(list_from_values(values)));
        }
        write_style(&self.style, &mut c);
        Value::Compound(c)
    }

    pub fn from_nbt(value: &Value) -> ParseResult<Self> {
        parse_value(value, false)
    }

    /// Never fails: malformed parts degrade to empty text or unset style fields.
    pub fn from_nbt_lenient(value: &Value) -> Self {
        parse_value(value, true).unwrap_or_default()
    }
}

impl From<&str> for Component {
    fn from(s: &str) -> Self {
        Self::text(s)
    }
}

impl From<String> for Component {
    fn from(s: String) -> Self {
        Self::text(s)
    }
}

impl Encode for Component {
    fn encode(&self, out: &mut Vec<u8>) {
        let mut nbt = self.to_nbt();
        nbt.sort_vanilla_order();
        write_network(out, Some(&nbt));
    }
}

impl Decode<'_> for Component {
    fn decode(input: &mut &[u8]) -> Result<Self> {
        let value = read_network(input, Limits::UNLIMITED)?
            .ok_or_else(|| DecodeError::Other("text component is the end tag".into()))?;
        Ok(Self::from_nbt_lenient(&value))
    }
}

const MAX_PLAIN_STEPS: usize = 100_000;
const MAX_PLAIN_DEPTH: usize = 64;
const MAX_PLAIN_BYTES: usize = 1 << 20;

struct PlainBudget {
    steps: usize,
    depth: usize,
}

impl PlainBudget {
    fn enter(&mut self, out: &str) -> bool {
        if self.steps == 0 || self.depth >= MAX_PLAIN_DEPTH || out.len() >= MAX_PLAIN_BYTES {
            return false;
        }
        self.steps -= 1;
        self.depth += 1;
        true
    }

    fn charge(&mut self, bytes: usize, out: &str) -> bool {
        let cost = 1 + bytes / 16;
        if self.steps < cost || out.len() >= MAX_PLAIN_BYTES {
            return false;
        }
        self.steps -= cost;
        true
    }
}

fn write_translation(
    template: &str,
    args: &[Arg],
    lookup: &dyn Translate,
    budget: &mut PlainBudget,
    out: &mut String,
) {
    if !budget.charge(template.len(), out) {
        return;
    }
    let Some(pieces) = decompose_template(template, args.len()) else {
        out.push_str(template);
        return;
    };
    for piece in pieces {
        match piece {
            TemplatePiece::Literal(s) => out.push_str(s),
            TemplatePiece::Percent => out.push('%'),
            TemplatePiece::Arg(i) => {
                if budget.steps == 0 || out.len() >= MAX_PLAIN_BYTES {
                    return;
                }
                budget.steps -= 1;
                match &args[i] {
                    Arg::Str(s) => out.push_str(s),
                    Arg::Component(c) => c.write_plain(lookup, budget, out),
                    Arg::Number(v) => out.push_str(&number_to_string(v)),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplatePiece<'a> {
    Literal(&'a str),
    Percent,
    Arg(usize),
}

/// Vanilla's `TranslatableContents.decomposeTemplate`: only `%s`, `%n$s` and `%%` are valid, and any
/// other `%` or an argument out of range makes the whole template render as literal text (`None`).
pub fn decompose_template(template: &str, arg_count: usize) -> Option<Vec<TemplatePiece<'_>>> {
    let bytes = template.as_bytes();
    let mut pieces = Vec::new();
    let mut next = 0usize;
    let mut literal_from = 0;
    let mut i = 0;
    while let Some(off) = template[i..].find('%') {
        let at = i + off;
        let mut j = at + 1;
        let digits = bytes[j..].iter().take_while(|b| b.is_ascii_digit()).count();
        let positional = digits > 0 && bytes.get(j + digits) == Some(&b'$');
        let index = if positional {
            let n: usize = template[j..j + digits].parse().ok()?;
            j += digits + 1;
            Some(n.checked_sub(1)?)
        } else {
            None
        };
        let piece = match bytes.get(j) {
            Some(b'%') if !positional => TemplatePiece::Percent,
            Some(b's') => {
                let slot = index.unwrap_or_else(|| {
                    next += 1;
                    next - 1
                });
                if slot >= arg_count {
                    return None;
                }
                TemplatePiece::Arg(slot)
            }
            _ => return None,
        };
        if at > literal_from {
            pieces.push(TemplatePiece::Literal(&template[literal_from..at]));
        }
        pieces.push(piece);
        i = j + 1;
        literal_from = i;
    }
    if literal_from < template.len() {
        pieces.push(TemplatePiece::Literal(&template[literal_from..]));
    }
    Some(pieces)
}

/// Java's `Double.toString`: plain decimal in `[1e-3, 1e7)`, otherwise `d.dddE<exp>`.
pub fn java_double_string(v: f64) -> String {
    java_float_string(v, &format!("{:e}", v.abs()))
}

/// Java's `Float.toString`.
pub fn java_float_to_string(v: f32) -> String {
    java_float_string(f64::from(v), &format!("{:e}", v.abs()))
}

fn java_float_string(v: f64, sci: &str) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    let sign = if v.is_sign_negative() { "-" } else { "" };
    if v.is_infinite() {
        return format!("{sign}Infinity");
    }
    if v == 0.0 {
        return format!("{sign}0.0");
    }
    let plain_range = (1e-3..1e7).contains(&v.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    if !plain_range {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        return format!("{sign}{first}.{rest}E{exp}");
    }
    let point = exp + 1;
    let body = if point <= 0 {
        format!("0.{}{}", "0".repeat(point.unsigned_abs() as usize), digits)
    } else if digits.len() as i32 <= point {
        format!(
            "{}{}.0",
            digits,
            "0".repeat((point as usize) - digits.len())
        )
    } else {
        let (int, frac) = digits.split_at(point as usize);
        format!("{int}.{frac}")
    };
    format!("{sign}{body}")
}

pub fn number_to_string(v: &Value) -> String {
    match v {
        Value::Byte(n) => n.to_string(),
        Value::Short(n) => n.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Long(n) => n.to_string(),
        Value::Float(n) => java_float_to_string(*n),
        Value::Double(n) => java_double_string(*n),
        _ => String::new(),
    }
}

fn list_from_values(values: Vec<Value>) -> List {
    if values.is_empty() {
        return List::Empty;
    }
    if values.iter().all(|v| matches!(v, Value::String(_))) {
        return List::String(
            values
                .into_iter()
                .map(|v| match v {
                    Value::String(s) => s,
                    _ => unreachable!(),
                })
                .collect(),
        );
    }
    List::Compound(
        values
            .into_iter()
            .map(|v| match v {
                Value::Compound(c) => c,
                other => {
                    let mut wrapper = Compound::new();
                    wrapper.insert("", other);
                    wrapper
                }
            })
            .collect(),
    )
}

fn unwrap_element(c: &Compound) -> Option<&Value> {
    if c.len() == 1 { c.get("") } else { None }
}

fn list_values(list: &List) -> Vec<Value> {
    match list {
        List::Empty => Vec::new(),
        List::Byte(v) => v.iter().map(|&x| Value::Byte(x)).collect(),
        List::Short(v) => v.iter().map(|&x| Value::Short(x)).collect(),
        List::Int(v) => v.iter().map(|&x| Value::Int(x)).collect(),
        List::Long(v) => v.iter().map(|&x| Value::Long(x)).collect(),
        List::Float(v) => v.iter().map(|&x| Value::Float(x)).collect(),
        List::Double(v) => v.iter().map(|&x| Value::Double(x)).collect(),
        List::ByteArray(v) => v.iter().map(|x| Value::ByteArray(x.clone())).collect(),
        List::String(v) => v.iter().map(|x| Value::String(x.clone())).collect(),
        List::List(v) => v.iter().map(|x| Value::List(x.clone())).collect(),
        List::Compound(v) => v
            .iter()
            .map(|c| {
                unwrap_element(c)
                    .cloned()
                    .unwrap_or_else(|| Value::Compound(c.clone()))
            })
            .collect(),
        List::IntArray(v) => v.iter().map(|x| Value::IntArray(x.clone())).collect(),
        List::LongArray(v) => v.iter().map(|x| Value::LongArray(x.clone())).collect(),
    }
}

fn parse_children(list: &List, lenient: bool) -> ParseResult<Vec<Component>> {
    let mut out = Vec::new();
    match list {
        List::Compound(items) => {
            for c in items {
                out.push(match unwrap_element(c) {
                    Some(inner) => parse_or_default(inner, lenient)?,
                    None => parse_compound_or_default(c, lenient)?,
                });
            }
        }
        other => {
            for v in list_values(other) {
                out.push(parse_or_default(&v, lenient)?);
            }
        }
    }
    Ok(out)
}

fn parse_or_default(value: &Value, lenient: bool) -> ParseResult<Component> {
    match parse_value(value, lenient) {
        Err(_) if lenient => Ok(Component::default()),
        r => r,
    }
}

fn parse_compound_or_default(c: &Compound, lenient: bool) -> ParseResult<Component> {
    match parse_compound(c, lenient) {
        Err(_) if lenient => Ok(Component::default()),
        r => r,
    }
}

fn parse_value(value: &Value, lenient: bool) -> ParseResult<Component> {
    match value {
        Value::String(s) => Ok(Component::text(s.as_str())),
        Value::Compound(c) => parse_compound(c, lenient),
        Value::List(list) => {
            let mut items = parse_children(list, lenient)?.into_iter();
            let Some(mut root) = items.next() else {
                return err("empty component list");
            };
            root.extra.extend(items);
            Ok(root)
        }
        other => err(format!("unexpected tag {} for component", other.tag())),
    }
}

fn parse_compound(c: &Compound, lenient: bool) -> ParseResult<Component> {
    let content = match parse_content(c, lenient)? {
        Some(content) => content,
        None if lenient => Content::Text(String::new()),
        None => return err("no matching content type"),
    };
    let extra = match c.get("extra") {
        Some(Value::List(list)) => parse_children(list, lenient)?,
        Some(_) if !lenient => return err("extra is not a list"),
        _ => Vec::new(),
    };
    Ok(Component {
        content,
        style: parse_style(c, lenient)?,
        extra,
    })
}

fn parse_content(c: &Compound, lenient: bool) -> ParseResult<Option<Content>> {
    if let Some(kind) = c.get_str("type") {
        let kind = kind.strip_prefix("minecraft:").unwrap_or(kind);
        return match kind {
            "text" => parse_text(c),
            "translatable" => parse_translatable(c, lenient),
            "keybind" => parse_keybind(c),
            "score" => parse_score(c),
            "selector" => parse_selector(c, lenient),
            "nbt" => parse_nbt(c, lenient),
            "object" => parse_object(c, lenient),
            other => err(format!("unknown content type {other}")),
        };
    }
    for parse in [
        parse_text as fn(&Compound) -> ParseResult<Option<Content>>,
        parse_keybind,
        parse_score,
    ] {
        if let Some(content) = parse(c)? {
            return Ok(Some(content));
        }
    }
    if let Some(content) = parse_translatable(c, lenient)? {
        return Ok(Some(content));
    }
    for parse in [parse_selector, parse_nbt, parse_object] {
        if let Some(content) = parse(c, lenient)? {
            return Ok(Some(content));
        }
    }
    Ok(None)
}

fn parse_text(c: &Compound) -> ParseResult<Option<Content>> {
    Ok(c.get_str("text").map(|t| Content::Text(t.to_owned())))
}

fn parse_keybind(c: &Compound) -> ParseResult<Option<Content>> {
    Ok(c.get_str("keybind").map(|k| Content::Keybind(k.to_owned())))
}

fn parse_score(c: &Compound) -> ParseResult<Option<Content>> {
    let Some(inner) = c.get_compound("score") else {
        return Ok(None);
    };
    Ok(match (inner.get_str("name"), inner.get_str("objective")) {
        (Some(name), Some(objective)) => Some(Content::Score {
            name: name.to_owned(),
            objective: objective.to_owned(),
        }),
        _ => None,
    })
}

fn parse_translatable(c: &Compound, lenient: bool) -> ParseResult<Option<Content>> {
    let Some(key) = c.get_str("translate") else {
        return Ok(None);
    };
    let mut args = Vec::new();
    if let Some(Value::List(list)) = c.get("with") {
        for v in list_values(list) {
            args.push(match v {
                Value::String(s) => Arg::Str(s),
                n @ (Value::Byte(_)
                | Value::Short(_)
                | Value::Int(_)
                | Value::Long(_)
                | Value::Float(_)
                | Value::Double(_)) => Arg::Number(n),
                other => Arg::Component(parse_or_default(&other, lenient)?),
            });
        }
    }
    Ok(Some(Content::Translatable {
        key: key.to_owned(),
        fallback: c.get_str("fallback").map(str::to_owned),
        args,
    }))
}

fn parse_separator(c: &Compound, lenient: bool) -> ParseResult<Option<Box<Component>>> {
    c.get("separator")
        .map(|v| parse_or_default(v, lenient).map(Box::new))
        .transpose()
}

fn parse_selector(c: &Compound, lenient: bool) -> ParseResult<Option<Content>> {
    let Some(selector) = c.get_str("selector") else {
        return Ok(None);
    };
    Ok(Some(Content::Selector {
        selector: selector.to_owned(),
        separator: parse_separator(c, lenient)?,
    }))
}

fn parse_nbt(c: &Compound, lenient: bool) -> ParseResult<Option<Content>> {
    let Some(path) = c.get_str("nbt") else {
        return Ok(None);
    };
    let interpret = c.get("interpret").and_then(as_bool).unwrap_or(false);
    let plain = c.get("plain").and_then(as_bool).unwrap_or(false);
    if interpret && plain {
        return Ok(None);
    }
    let source = match c
        .get_str("source")
        .map(|s| s.strip_prefix("minecraft:").unwrap_or(s))
    {
        Some("entity") => c.get_str("entity").map(|s| NbtSource::Entity(s.to_owned())),
        Some("block") => c.get_str("block").map(|s| NbtSource::Block(s.to_owned())),
        Some("storage") => c
            .get_str("storage")
            .and_then(parse_id)
            .map(NbtSource::Storage),
        Some(_) => None,
        None => c
            .get_str("entity")
            .map(|s| NbtSource::Entity(s.to_owned()))
            .or_else(|| c.get_str("block").map(|s| NbtSource::Block(s.to_owned())))
            .or_else(|| {
                c.get_str("storage")
                    .and_then(parse_id)
                    .map(NbtSource::Storage)
            }),
    };
    let Some(source) = source else {
        return Ok(None);
    };
    Ok(Some(Content::Nbt(Box::new(NbtContent {
        path: path.to_owned(),
        interpret,
        plain,
        separator: parse_separator(c, lenient)?,
        source,
    }))))
}

fn parse_object(c: &Compound, lenient: bool) -> ParseResult<Option<Content>> {
    let kind = c
        .get_str("object")
        .map(|s| s.strip_prefix("minecraft:").unwrap_or(s));
    let object = match kind {
        Some("atlas") => parse_atlas(c),
        Some("player") => parse_player(c),
        Some(_) => None,
        None => parse_atlas(c).or_else(|| parse_player(c)),
    };
    let Some(object) = object else {
        return Ok(None);
    };
    let fallback = c
        .get("fallback")
        .map(|v| parse_or_default(v, lenient).map(Box::new))
        .transpose()?;
    Ok(Some(Content::Object { object, fallback }))
}

fn default_atlas() -> Identifier {
    Identifier::new("minecraft", "blocks").unwrap()
}

fn parse_atlas(c: &Compound) -> Option<ObjectContent> {
    let sprite = parse_id(c.get_str("sprite")?)?;
    let atlas = match c.get_str("atlas") {
        Some(a) => parse_id(a)?,
        None => default_atlas(),
    };
    Some(ObjectContent::Atlas { atlas, sprite })
}

fn parse_player(c: &Compound) -> Option<ObjectContent> {
    Some(ObjectContent::Player {
        player: c.get("player")?.clone(),
        hat: c.get("hat").and_then(as_bool).unwrap_or(true),
    })
}

fn parse_id(s: &str) -> Option<Identifier> {
    Identifier::parse(s).ok()
}

fn as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Byte(n) => Some(*n != 0),
        Value::Short(n) => Some(*n != 0),
        Value::Int(n) => Some(*n != 0),
        Value::Long(n) => Some(*n != 0),
        _ => None,
    }
}

fn field<T>(
    c: &Compound,
    key: &str,
    lenient: bool,
    parse: impl FnOnce(&Value) -> Option<T>,
) -> ParseResult<Option<T>> {
    match c.get(key) {
        None => Ok(None),
        Some(v) => match parse(v) {
            Some(t) => Ok(Some(t)),
            None if lenient => Ok(None),
            None => err(format!("invalid style field {key}")),
        },
    }
}

fn parse_style(c: &Compound, lenient: bool) -> ParseResult<Style> {
    let string = |v: &Value| v.as_str().map(str::to_owned);
    Ok(Style {
        color: field(c, "color", lenient, |v| v.as_str().and_then(Color::parse))?,
        shadow_color: field(c, "shadow_color", lenient, parse_argb)?,
        bold: field(c, "bold", lenient, as_bool)?,
        italic: field(c, "italic", lenient, as_bool)?,
        underlined: field(c, "underlined", lenient, as_bool)?,
        strikethrough: field(c, "strikethrough", lenient, as_bool)?,
        obfuscated: field(c, "obfuscated", lenient, as_bool)?,
        click_event: field(c, "click_event", lenient, |v| {
            v.as_compound().and_then(parse_click).map(Box::new)
        })?,
        hover_event: match c.get("hover_event") {
            None => None,
            Some(v) => match v.as_compound().and_then(|h| parse_hover(h, lenient)) {
                Some(h) => Some(Box::new(h)),
                None if lenient => None,
                None => return err("invalid style field hover_event"),
            },
        },
        insertion: field(c, "insertion", lenient, string)?,
        font: field(c, "font", lenient, |v| v.as_str().and_then(parse_id))?,
    })
}

fn parse_argb(v: &Value) -> Option<i32> {
    match v {
        Value::Int(n) => Some(*n),
        Value::List(List::Float(f)) if f.len() == 4 => {
            let channel = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
            let (r, g, b, a) = (channel(f[0]), channel(f[1]), channel(f[2]), channel(f[3]));
            Some(((a << 24) | (r << 16) | (g << 8) | b) as i32)
        }
        _ => None,
    }
}

impl ClickEvent {
    /// Reads a click event compound keyed by `action`; `None` when the action or its fields are unknown.
    pub fn from_nbt(c: &Compound) -> Option<Self> {
        parse_click(c)
    }
}

fn parse_click(c: &Compound) -> Option<ClickEvent> {
    let string = |key: &str| c.get_str(key).map(str::to_owned);
    Some(match c.get_str("action")? {
        "open_url" => ClickEvent::OpenUrl(string("url")?),
        "open_file" => ClickEvent::OpenFile(string("path")?),
        "run_command" => ClickEvent::RunCommand(string("command")?),
        "suggest_command" => ClickEvent::SuggestCommand(string("command")?),
        "show_dialog" => ClickEvent::ShowDialog(c.get("dialog")?.clone()),
        "change_page" => ClickEvent::ChangePage(c.get("page")?.as_i32()?),
        "copy_to_clipboard" => ClickEvent::CopyToClipboard(string("value")?),
        "custom" => ClickEvent::Custom {
            id: parse_id(c.get_str("id")?)?,
            payload: c.get("payload").cloned(),
        },
        _ => return None,
    })
}

fn parse_uuid(v: &Value) -> Option<Uuid> {
    match v {
        Value::IntArray(a) if a.len() == 4 => {
            let word = |i: usize| a[i] as u32 as u128;
            Some(Uuid::from_u128(
                word(0) << 96 | word(1) << 64 | word(2) << 32 | word(3),
            ))
        }
        Value::String(s) => Uuid::parse_str(s).ok(),
        _ => None,
    }
}

fn parse_hover(c: &Compound, lenient: bool) -> Option<HoverEvent> {
    match c.get_str("action")? {
        "show_text" => Some(HoverEvent::ShowText(Box::new(
            parse_value(c.get("value")?, lenient).ok()?,
        ))),
        "show_item" => {
            let mut item = c.clone();
            item.remove("action");
            Some(HoverEvent::ShowItem(item))
        }
        "show_entity" => Some(HoverEvent::ShowEntity {
            entity: parse_id(c.get_str("id")?)?,
            uuid: parse_uuid(c.get("uuid")?)?,
            name: c
                .get("name")
                .and_then(|v| parse_value(v, lenient).ok())
                .map(Box::new),
        }),
        _ => None,
    }
}

fn write_content(content: &Content, c: &mut Compound) {
    match content {
        Content::Text(t) => {
            c.insert("text", t.as_str());
        }
        Content::Translatable {
            key,
            fallback,
            args,
        } => {
            c.insert("translate", key.as_str());
            if let Some(f) = fallback {
                c.insert("fallback", f.as_str());
            }
            if !args.is_empty() {
                let values = args
                    .iter()
                    .map(|a| match a {
                        Arg::Str(s) => Value::String(s.clone()),
                        Arg::Number(n) => n.clone(),
                        Arg::Component(comp) => comp.to_nbt(),
                    })
                    .collect();
                c.insert("with", Value::List(list_from_values(values)));
            }
        }
        Content::Keybind(k) => {
            c.insert("keybind", k.as_str());
        }
        Content::Score { name, objective } => {
            let mut inner = Compound::new();
            inner.insert("name", name.as_str());
            inner.insert("objective", objective.as_str());
            c.insert("score", Value::Compound(inner));
        }
        Content::Selector {
            selector,
            separator,
        } => {
            c.insert("selector", selector.as_str());
            write_separator(separator, c);
        }
        Content::Nbt(n) => {
            c.insert("nbt", n.path.as_str());
            if n.interpret {
                c.insert("interpret", true);
            }
            if n.plain {
                c.insert("plain", true);
            }
            write_separator(&n.separator, c);
            match &n.source {
                NbtSource::Entity(s) => c.insert("entity", s.as_str()),
                NbtSource::Block(s) => c.insert("block", s.as_str()),
                NbtSource::Storage(id) => c.insert("storage", id.as_str()),
            };
        }
        Content::Object { object, fallback } => {
            match object {
                ObjectContent::Atlas { atlas, sprite } => {
                    if *atlas != default_atlas() {
                        c.insert("atlas", atlas.as_str());
                    }
                    c.insert("sprite", sprite.as_str());
                }
                ObjectContent::Player { player, hat } => {
                    c.insert("player", player.clone());
                    if !hat {
                        c.insert("hat", false);
                    }
                }
            }
            if let Some(f) = fallback {
                c.insert("fallback", f.to_nbt());
            }
        }
    }
}

fn write_separator(separator: &Option<Box<Component>>, c: &mut Compound) {
    if let Some(s) = separator {
        c.insert("separator", s.to_nbt());
    }
}

fn write_style(style: &Style, c: &mut Compound) {
    if let Some(color) = style.color {
        c.insert("color", color.serialize());
    }
    if let Some(shadow) = style.shadow_color {
        c.insert("shadow_color", shadow);
    }
    for (key, flag) in [
        ("bold", style.bold),
        ("italic", style.italic),
        ("underlined", style.underlined),
        ("strikethrough", style.strikethrough),
        ("obfuscated", style.obfuscated),
    ] {
        if let Some(flag) = flag {
            c.insert(key, flag);
        }
    }
    if let Some(click) = &style.click_event {
        c.insert("click_event", Value::Compound(click_to_nbt(click)));
    }
    if let Some(hover) = &style.hover_event {
        c.insert("hover_event", Value::Compound(hover_to_nbt(hover)));
    }
    if let Some(insertion) = &style.insertion {
        c.insert("insertion", insertion.as_str());
    }
    if let Some(font) = &style.font {
        c.insert("font", font.as_str());
    }
}

fn click_to_nbt(click: &ClickEvent) -> Compound {
    let mut c = Compound::new();
    let (action, key): (&str, &str) = match click {
        ClickEvent::OpenUrl(_) => ("open_url", "url"),
        ClickEvent::OpenFile(_) => ("open_file", "path"),
        ClickEvent::RunCommand(_) => ("run_command", "command"),
        ClickEvent::SuggestCommand(_) => ("suggest_command", "command"),
        ClickEvent::ShowDialog(_) => ("show_dialog", "dialog"),
        ClickEvent::ChangePage(_) => ("change_page", "page"),
        ClickEvent::CopyToClipboard(_) => ("copy_to_clipboard", "value"),
        ClickEvent::Custom { .. } => ("custom", "id"),
    };
    c.insert("action", action);
    match click {
        ClickEvent::OpenUrl(s)
        | ClickEvent::OpenFile(s)
        | ClickEvent::RunCommand(s)
        | ClickEvent::SuggestCommand(s)
        | ClickEvent::CopyToClipboard(s) => {
            c.insert(key, s.as_str());
        }
        ClickEvent::ShowDialog(v) => {
            c.insert(key, v.clone());
        }
        ClickEvent::ChangePage(page) => {
            c.insert(key, *page);
        }
        ClickEvent::Custom { id, payload } => {
            c.insert(key, id.as_str());
            if let Some(p) = payload {
                c.insert("payload", p.clone());
            }
        }
    }
    c
}

fn hover_to_nbt(hover: &HoverEvent) -> Compound {
    let mut c = Compound::new();
    match hover {
        HoverEvent::ShowText(text) => {
            c.insert("action", "show_text");
            c.insert("value", text.to_nbt());
        }
        HoverEvent::ShowItem(item) => {
            c.insert("action", "show_item");
            for (k, v) in item.iter() {
                c.insert(k, v.clone());
            }
        }
        HoverEvent::ShowEntity { entity, uuid, name } => {
            c.insert("action", "show_entity");
            c.insert("id", entity.as_str());
            let bits = uuid.as_u128();
            let words = [96, 64, 32, 0].map(|shift| (bits >> shift) as u32 as i32);
            c.insert("uuid", Value::IntArray(words.to_vec()));
            if let Some(name) = name {
                c.insert("name", name.to_nbt());
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use mcr_buf::{from_bytes_exact, to_bytes};

    use super::*;
    use crate::test_util::roundtrip;

    fn rich() -> Component {
        let style = Style {
            color: Some(Color::Rgb(0x12_34AB)),
            shadow_color: Some(-1),
            bold: Some(true),
            italic: Some(false),
            insertion: Some("ins".into()),
            font: Some(Identifier::parse("alt").unwrap()),
            click_event: Some(Box::new(ClickEvent::RunCommand("/say hi".into()))),
            hover_event: Some(Box::new(HoverEvent::ShowText(Box::new(Component::text(
                "tip",
            ))))),
            ..Style::default()
        };
        Component::translatable(
            "chat.type.text",
            vec![
                Arg::Str("Bob".into()),
                Arg::Number(Value::Int(3)),
                Arg::Component(Component::text("x").styled(Style {
                    color: Some(Color::Named(NamedColor::Red)),
                    ..Style::default()
                })),
            ],
        )
        .styled(style)
        .with(Component::text("a"))
        .with(Component::text("b").styled(Style {
            underlined: Some(true),
            ..Style::default()
        }))
    }

    #[test]
    fn plain_text_is_bare_string() {
        let bytes = roundtrip(&Component::text("x"));
        assert_eq!(bytes, [8, 0, 1, b'x']);
        assert_eq!(
            from_bytes_exact::<Component>(&bytes).unwrap(),
            Component::text("x")
        );
    }

    #[test]
    fn rich_roundtrip() {
        roundtrip(&rich());
    }

    #[test]
    fn all_contents_roundtrip() {
        let contents = [
            Content::Keybind("key.jump".into()),
            Content::Score {
                name: "@p".into(),
                objective: "o".into(),
            },
            Content::Selector {
                selector: "@a".into(),
                separator: Some(Box::new(Component::text(", "))),
            },
            Content::Nbt(Box::new(NbtContent {
                path: "a.b".into(),
                interpret: true,
                plain: false,
                separator: None,
                source: NbtSource::Storage(Identifier::parse("s:t").unwrap()),
            })),
            Content::Nbt(Box::new(NbtContent {
                path: "a".into(),
                interpret: false,
                plain: true,
                separator: None,
                source: NbtSource::Block("~ ~ ~".into()),
            })),
            Content::Nbt(Box::new(NbtContent {
                path: "a".into(),
                interpret: false,
                plain: false,
                separator: None,
                source: NbtSource::Entity("@s".into()),
            })),
            Content::Object {
                object: ObjectContent::Atlas {
                    atlas: Identifier::parse("minecraft:items").unwrap(),
                    sprite: Identifier::parse("diamond").unwrap(),
                },
                fallback: Some(Box::new(Component::text("D"))),
            },
            Content::Object {
                object: ObjectContent::Atlas {
                    atlas: default_atlas(),
                    sprite: Identifier::parse("stone").unwrap(),
                },
                fallback: None,
            },
            Content::Object {
                object: ObjectContent::Player {
                    player: Value::String("Notch".into()),
                    hat: false,
                },
                fallback: None,
            },
            Content::Translatable {
                key: "k".into(),
                fallback: Some("fb".into()),
                args: vec![],
            },
        ];
        for content in contents {
            roundtrip(&Component::from_content(content));
        }
    }

    #[test]
    fn events_roundtrip() {
        let clicks = [
            ClickEvent::OpenUrl("https://x".into()),
            ClickEvent::OpenFile("f".into()),
            ClickEvent::SuggestCommand("/x".into()),
            ClickEvent::ShowDialog(Value::String("d".into())),
            ClickEvent::ChangePage(2),
            ClickEvent::CopyToClipboard("c".into()),
            ClickEvent::Custom {
                id: Identifier::parse("a:b").unwrap(),
                payload: Some(Value::Int(1)),
            },
        ];
        for click in clicks {
            let style = Style {
                click_event: Some(Box::new(click)),
                ..Style::default()
            };
            roundtrip(&Component::text("t").styled(style));
        }
        let mut item = Compound::new();
        item.insert("id", "minecraft:stick");
        item.insert("count", 2);
        let hovers = [
            HoverEvent::ShowItem(item),
            HoverEvent::ShowEntity {
                entity: Identifier::parse("pig").unwrap(),
                uuid: Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef),
                name: Some(Box::new(Component::text("Oink"))),
            },
        ];
        for hover in hovers {
            let style = Style {
                hover_event: Some(Box::new(hover)),
                ..Style::default()
            };
            roundtrip(&Component::text("t").styled(style));
        }
    }

    #[test]
    fn decodes_hand_built_compound() {
        let mut bytes = vec![10, 8, 0, 4];
        bytes.extend(*b"text");
        bytes.extend([0, 1, b'a', 1, 0, 4]);
        bytes.extend(*b"bold");
        bytes.extend([1, 0]);
        let c = from_bytes_exact::<Component>(&bytes).unwrap();
        assert_eq!(c.content, Content::Text("a".into()));
        assert_eq!(c.style.bold, Some(true));
    }

    #[test]
    fn list_form_appends_siblings() {
        let list = Value::List(List::String(vec!["a".into(), "b".into(), "c".into()]));
        let c = Component::from_nbt(&list).unwrap();
        assert_eq!(c.content, Content::Text("a".into()));
        assert_eq!(c.extra, vec![Component::text("b"), Component::text("c")]);
        assert_eq!(c.to_plain_string(), "abc");
    }

    #[test]
    fn mixed_extra_list_wraps_and_unwraps() {
        let c =
            Component::text("a")
                .with(Component::text("plain"))
                .with(Component::text("b").styled(Style {
                    bold: Some(true),
                    ..Style::default()
                }));
        roundtrip(&c);
    }

    #[test]
    fn unknown_style_keys_are_ignored() {
        let mut c = Compound::new();
        c.insert("text", "hi");
        c.insert("future_style", 5);
        c.insert("color", "not-a-color");
        c.insert("bold", true);
        let value = Value::Compound(c);
        assert!(Component::from_nbt(&value).is_err());
        let lenient = Component::from_nbt_lenient(&value);
        assert_eq!(lenient.content, Content::Text("hi".into()));
        assert_eq!(lenient.style.color, None);
        assert_eq!(lenient.style.bold, Some(true));
    }

    #[test]
    fn lenient_never_fails() {
        for value in [
            Value::Int(5),
            Value::Compound(Compound::new()),
            Value::List(List::Empty),
        ] {
            assert_eq!(Component::from_nbt_lenient(&value), Component::default());
        }
        let bytes = to_bytes(&mcr_nbt::NetworkNbt(Value::Int(1)));
        assert_eq!(
            from_bytes_exact::<Component>(&bytes).unwrap(),
            Component::default()
        );
    }

    #[test]
    fn typed_content_dispatch() {
        let mut c = Compound::new();
        c.insert("type", "minecraft:keybind");
        c.insert("keybind", "key.use");
        let parsed = Component::from_nbt(&Value::Compound(c)).unwrap();
        assert_eq!(parsed.content, Content::Keybind("key.use".into()));
    }

    #[test]
    fn plain_string_formats_translations() {
        let c = Component::from_content(Content::Translatable {
            key: "greeting".into(),
            fallback: Some("Hi %s, %2$s #%1$s 100%%".into()),
            args: vec![Arg::Str("Bob".into()), Arg::Number(Value::Int(7))],
        })
        .with(Component::text("!"));
        assert_eq!(c.to_plain_string(), "Hi Bob, 7 #Bob 100%!");
        assert_eq!(
            Component::translatable("chat.x", vec![]).to_plain_string(),
            "chat.x"
        );
    }

    struct Fixed(&'static str);

    impl Translate for Fixed {
        fn translate(&self, key: &str) -> Option<&str> {
            (key == "x").then_some(self.0)
        }
    }

    fn translatable(fallback: &str, args: Vec<Arg>) -> Component {
        Component::from_content(Content::Translatable {
            key: "x".into(),
            fallback: Some(fallback.into()),
            args,
        })
    }

    #[test]
    fn invalid_templates_render_raw() {
        let one = || vec![Arg::Str("Bob".into())];
        for template in [
            "a%0$sb",
            "a%99999999999999999999$sc",
            "%d",
            "%2$s",
            "%s %s",
            "50%",
            "%x",
            "%.2f",
            "%1$%",
            "%5s",
        ] {
            assert_eq!(translatable(template, one()).to_plain_string(), template);
        }
        assert_eq!(translatable("%s%%%1$s", one()).to_plain_string(), "Bob%Bob");
    }

    #[test]
    fn plain_string_resolves_through_the_lookup() {
        let c = translatable("fb", vec![Arg::Str("a".into())]);
        assert_eq!(c.to_plain_string_with(&Fixed("<%s>")), "<a>");
        assert_eq!(c.to_plain_string(), "fb");
    }

    #[test]
    fn score_and_nbt_have_no_plain_text() {
        let score = Component::from_content(Content::Score {
            name: "@s".into(),
            objective: "o".into(),
        });
        assert_eq!(score.to_plain_string(), "");
    }

    #[test]
    fn java_double_strings() {
        for (v, s) in [
            (2.63e9, "2.63E9"),
            (1e-4, "1.0E-4"),
            (0.001, "0.001"),
            (1e7, "1.0E7"),
            (9_999_999.0, "9999999.0"),
            (-12.5, "-12.5"),
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (100.0, "100.0"),
            (123456.789, "123456.789"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
        ] {
            assert_eq!(java_double_string(v), s);
        }
        assert_eq!(java_float_to_string(0.1), "0.1");
        assert_eq!(java_float_to_string(1.5e10), "1.5E10");
    }

    #[test]
    fn hostile_plain_expansion_is_bounded() {
        let mut c = Component::text("ab");
        for _ in 0..40 {
            c = Component::translatable("x", vec![Arg::Component(c)]);
        }
        let start = std::time::Instant::now();
        assert!(
            c.to_plain_string_with(&Fixed("%1$s%1$s%1$s%1$s%1$s%1$s%1$s%1$s"))
                .len()
                <= 2 * MAX_PLAIN_BYTES
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(1));

        let long = format!("{}{}", "%1$s".repeat(10_000), "x".repeat(10_000));
        let mut c = Component::text("ab");
        for _ in 0..8 {
            c = translatable(&long, vec![Arg::Component(c)]);
        }
        let start = std::time::Instant::now();
        assert!(c.to_plain_string().len() <= 2 * MAX_PLAIN_BYTES);
        assert!(start.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn color_parsing() {
        assert_eq!(Color::parse("red"), Some(Color::Named(NamedColor::Red)));
        assert_eq!(Color::parse("#FF5555"), Some(Color::Rgb(0xFF5555)));
        assert_eq!(Color::parse("#1000000"), None);
        assert_eq!(Color::parse("#+123"), None);
        assert_eq!(Color::Rgb(0xAB).serialize(), "#0000AB");
    }
}
