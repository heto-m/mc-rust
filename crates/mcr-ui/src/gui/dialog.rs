use mcr_buf::Identifier;
use mcr_nbt::{Compound, List, Value};
use mcr_protocol::types::text::{ClickEvent, Component};

pub const MAX_BODY: usize = 32;
pub const MAX_INPUTS: usize = 32;
pub const MAX_BUTTONS: usize = 64;
pub const MAX_OPTIONS: usize = 64;
pub const MAX_SUB_DIALOGS: usize = 64;
pub const MAX_TEXT_LENGTH: usize = 4096;
pub const MAX_TEMPLATE: usize = 4096;
const MAX_WIDTH: i32 = 1024;
pub const MAX_COLUMNS: usize = 64;
pub const MAX_LINES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterAction {
    Close,
    None,
    WaitForResponse,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Static(ClickEvent),
    CommandTemplate(String),
    Custom {
        id: Identifier,
        additions: Option<Compound>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ActionButton {
    pub label: Component,
    pub tooltip: Option<Component>,
    pub width: i32,
    pub action: Option<Action>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Text {
        contents: Component,
        width: i32,
    },
    Item {
        item: String,
        count: i32,
        description: Option<(Component, i32)>,
        show_decorations: bool,
        show_tooltip: bool,
        width: i32,
        height: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SingleOption {
    pub id: String,
    pub display: Option<Component>,
    pub initial: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Text {
        width: i32,
        label: Component,
        label_visible: bool,
        initial: String,
        max_length: usize,
        multiline: Option<Multiline>,
    },
    Boolean {
        label: Component,
        initial: bool,
        on_true: String,
        on_false: String,
    },
    SingleOption {
        width: i32,
        options: Vec<SingleOption>,
        label: Component,
        label_visible: bool,
    },
    NumberRange {
        width: i32,
        label: Component,
        label_format: String,
        start: f32,
        end: f32,
        initial: Option<f32>,
        step: Option<f32>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Multiline {
    pub max_lines: Option<usize>,
    pub height: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub key: String,
    pub control: Control,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum DialogKind {
    Notice {
        action: ActionButton,
    },
    Confirmation {
        yes: ActionButton,
        no: ActionButton,
    },
    MultiAction {
        actions: Vec<ActionButton>,
        exit: Option<ActionButton>,
        columns: usize,
    },
    DialogList {
        dialogs: Vec<Compound>,
        exit: Option<ActionButton>,
        columns: usize,
        button_width: i32,
    },
    ServerLinks {
        exit: Option<ActionButton>,
        columns: usize,
        button_width: i32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    pub title: Component,
    pub external_title: Option<Component>,
    pub can_close_with_escape: bool,
    pub pause: bool,
    pub after_action: AfterAction,
    pub body: Vec<Body>,
    pub inputs: Vec<Input>,
    pub kind: DialogKind,
}

impl Dialog {
    /// Parses a server dialog compound; anything malformed or over the limits is rejected whole.
    pub fn from_nbt(value: &Value) -> Option<Self> {
        let c = value.as_compound()?;
        let common = Common::parse(c)?;
        let kind = match short(c.get_str("type")?) {
            "notice" => DialogKind::Notice {
                action: c
                    .get("action")
                    .map_or_else(default_ok, |v| button(v.as_compound()?))?,
            },
            "confirmation" => DialogKind::Confirmation {
                yes: button(c.get_compound("yes")?)?,
                no: button(c.get_compound("no")?)?,
            },
            "multi_action" => DialogKind::MultiAction {
                actions: buttons(c.get_list("actions")?)?,
                exit: optional_button(c, "exit_action")?,
                columns: columns(c)?,
            },
            "dialog_list" => DialogKind::DialogList {
                dialogs: sub_dialogs(c.get("dialogs")?)?,
                exit: optional_button(c, "exit_action")?,
                columns: columns(c)?,
                button_width: width(c, "button_width", 150)?,
            },
            "server_links" => DialogKind::ServerLinks {
                exit: optional_button(c, "exit_action")?,
                columns: columns(c)?,
                button_width: width(c, "button_width", 150)?,
            },
            _ => return None,
        };
        if common.pause && common.after_action == AfterAction::None {
            return None;
        }
        Some(Self {
            title: common.title,
            external_title: common.external_title,
            can_close_with_escape: common.can_close_with_escape,
            pause: common.pause,
            after_action: common.after_action,
            body: common.body,
            inputs: common.inputs,
            kind,
        })
    }

    pub fn list_title(&self) -> &Component {
        self.external_title.as_ref().unwrap_or(&self.title)
    }

    /// What Escape triggers.
    pub fn on_cancel(&self) -> Option<&Action> {
        match &self.kind {
            DialogKind::Notice { action } => action.action.as_ref(),
            DialogKind::Confirmation { no, .. } => no.action.as_ref(),
            DialogKind::MultiAction { exit, .. }
            | DialogKind::DialogList { exit, .. }
            | DialogKind::ServerLinks { exit, .. } => exit.as_ref()?.action.as_ref(),
        }
    }
}

struct Common {
    title: Component,
    external_title: Option<Component>,
    can_close_with_escape: bool,
    pause: bool,
    after_action: AfterAction,
    body: Vec<Body>,
    inputs: Vec<Input>,
}

impl Common {
    fn parse(c: &Compound) -> Option<Self> {
        let after_action = match c.get_str("after_action") {
            None | Some("close") => AfterAction::Close,
            Some("none") => AfterAction::None,
            Some("wait_for_response") => AfterAction::WaitForResponse,
            Some(_) => return None,
        };
        Some(Self {
            title: component(c.get("title")?),
            external_title: c.get("external_title").map(component),
            can_close_with_escape: c.get_bool("can_close_with_escape").unwrap_or(true),
            pause: c.get_bool("pause").unwrap_or(true),
            after_action,
            body: compact_list(c.get("body"), MAX_BODY, |v| body(v.as_compound()?))?,
            inputs: compact_list(c.get("inputs"), MAX_INPUTS, |v| input(v.as_compound()?))?,
        })
    }
}

fn short(id: &str) -> &str {
    id.strip_prefix("minecraft:").unwrap_or(id)
}

fn component(value: &Value) -> Component {
    Component::from_nbt_lenient(value)
}

fn width(c: &Compound, key: &str, default: i32) -> Option<i32> {
    match c.get(key) {
        None => Some(default),
        Some(v) => v.as_i32().filter(|w| (1..=MAX_WIDTH).contains(w)),
    }
}

fn columns(c: &Compound) -> Option<usize> {
    match c.get("columns") {
        None => Some(2),
        Some(v) => usize::try_from(v.as_i32()?)
            .ok()
            .filter(|n| (1..=MAX_COLUMNS).contains(n)),
    }
}

fn string_limited(c: &Compound, key: &str, default: &str) -> Option<String> {
    let value = c.get_str(key).unwrap_or(default);
    (value.chars().count() <= MAX_TEXT_LENGTH).then(|| value.to_owned())
}

fn elements(value: Option<&Value>) -> Option<Vec<&Value>> {
    Some(match value {
        None => Vec::new(),
        Some(v @ Value::Compound(_)) => vec![v],
        _ => return None,
    })
}

/// A missing key is an empty list, a compound stands for a one-element list.
fn compact_list<T>(
    value: Option<&Value>,
    max: usize,
    mut parse: impl FnMut(&Value) -> Option<T>,
) -> Option<Vec<T>> {
    match value {
        Some(Value::List(List::Compound(items))) => {
            if items.len() > max {
                return None;
            }
            items
                .iter()
                .map(|c| parse(&Value::Compound(c.clone())))
                .collect()
        }
        Some(Value::List(List::Empty)) | None => Some(Vec::new()),
        _ => elements(value)?.into_iter().map(parse).collect(),
    }
}

fn default_ok() -> Option<ActionButton> {
    Some(ActionButton {
        label: Component::translatable("gui.ok", Vec::new()),
        tooltip: None,
        width: 150,
        action: None,
    })
}

fn buttons(list: &List) -> Option<Vec<ActionButton>> {
    let List::Compound(items) = list else {
        return None;
    };
    if items.is_empty() || items.len() > MAX_BUTTONS {
        return None;
    }
    items.iter().map(button).collect()
}

fn optional_button(c: &Compound, key: &str) -> Option<Option<ActionButton>> {
    match c.get(key) {
        None => Some(None),
        Some(v) => Some(Some(button(v.as_compound()?)?)),
    }
}

fn button(c: &Compound) -> Option<ActionButton> {
    Some(ActionButton {
        label: component(c.get("label")?),
        tooltip: c.get("tooltip").map(component),
        width: width(c, "width", 150)?,
        action: match c.get("action") {
            None => None,
            Some(v) => Some(action(v.as_compound()?)?),
        },
    })
}

fn action(c: &Compound) -> Option<Action> {
    let kind = short(c.get_str("type")?);
    match kind {
        "dynamic/run_command" => {
            let template = c.get_str("template")?;
            (template.len() <= MAX_TEMPLATE).then(|| Action::CommandTemplate(template.to_owned()))
        }
        "dynamic/custom" => Some(Action::Custom {
            id: Identifier::parse(c.get_str("id")?).ok()?,
            additions: match c.get("additions") {
                None => None,
                Some(v) => Some(v.as_compound()?.clone()),
            },
        }),
        "open_file" => None,
        _ => {
            let mut click = c.clone();
            click.remove("type");
            click.insert("action", kind);
            ClickEvent::from_nbt(&click).map(Action::Static)
        }
    }
}

fn sub_dialogs(value: &Value) -> Option<Vec<Compound>> {
    let dialogs: Vec<Compound> = match value {
        Value::List(List::Compound(items)) => items.clone(),
        Value::Compound(c) => vec![c.clone()],
        _ => return None,
    };
    if dialogs.len() > MAX_SUB_DIALOGS {
        return None;
    }
    Some(dialogs)
}

fn plain_message(value: &Value) -> Option<(Component, i32)> {
    match value {
        Value::Compound(c) if c.contains_key("contents") => {
            Some((component(c.get("contents")?), width(c, "width", 200)?))
        }
        other => Some((component(other), 200)),
    }
}

fn body(c: &Compound) -> Option<Body> {
    match short(c.get_str("type")?) {
        "plain_message" => Some(Body::Text {
            contents: component(c.get("contents")?),
            width: width(c, "width", 200)?,
        }),
        "item" => {
            let item = c.get_compound("item")?;
            let size = |key: &str| match c.get(key) {
                None => Some(16),
                Some(v) => v.as_i32().filter(|n| (1..=256).contains(n)),
            };
            Some(Body::Item {
                item: short(item.get_str("id")?).to_owned(),
                count: item.get_i32("count").unwrap_or(1).clamp(1, 99),
                description: match c.get("description") {
                    None => None,
                    Some(v) => Some(plain_message(v)?),
                },
                show_decorations: c.get_bool("show_decorations").unwrap_or(true),
                show_tooltip: c.get_bool("show_tooltip").unwrap_or(true),
                width: size("width")?,
                height: size("height")?,
            })
        }
        _ => None,
    }
}

fn input(c: &Compound) -> Option<Input> {
    let key = c.get_str("key")?;
    let valid_key = !key.is_empty()
        && key.len() <= 64
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
    if !valid_key {
        return None;
    }
    let label = || Some(component(c.get("label")?));
    let control = match short(c.get_str("type")?) {
        "text" => {
            let max_length = match c.get("max_length") {
                None => 32,
                Some(v) => usize::try_from(v.as_i32()?).ok().filter(|&n| n > 0)?,
            };
            let max_length = max_length.min(MAX_TEXT_LENGTH);
            let initial = string_limited(c, "initial", "")?;
            if initial.chars().count() > max_length {
                return None;
            }
            let multiline = match c.get("multiline") {
                None => None,
                Some(v) => {
                    let m = v.as_compound()?;
                    Some(Multiline {
                        max_lines: match m.get("max_lines") {
                            None => None,
                            Some(n) => Some(
                                usize::try_from(n.as_i32()?)
                                    .ok()
                                    .filter(|n| (1..=MAX_LINES).contains(n))?,
                            ),
                        },
                        height: match m.get("height") {
                            None => None,
                            Some(n) => Some(n.as_i32().filter(|n| (1..=512).contains(n))?),
                        },
                    })
                }
            };
            Control::Text {
                width: width(c, "width", 200)?,
                label: label()?,
                label_visible: c.get_bool("label_visible").unwrap_or(true),
                initial,
                max_length,
                multiline,
            }
        }
        "boolean" => Control::Boolean {
            label: label()?,
            initial: c.get_bool("initial").unwrap_or(false),
            on_true: string_limited(c, "on_true", "true")?,
            on_false: string_limited(c, "on_false", "false")?,
        },
        "single_option" => {
            let options = single_options(c.get_list("options")?)?;
            if options.is_empty() || options.iter().filter(|o| o.initial).count() > 1 {
                return None;
            }
            Control::SingleOption {
                width: width(c, "width", 200)?,
                options,
                label: label()?,
                label_visible: c.get_bool("label_visible").unwrap_or(true),
            }
        }
        "number_range" => {
            let start = c.get_f32("start")?;
            let end = c.get_f32("end")?;
            let initial = match c.get("initial") {
                None => None,
                Some(v) => Some(v.as_f32()?),
            };
            let step = match c.get("step") {
                None => None,
                Some(v) => Some(v.as_f32().filter(|s| *s > 0.0 && s.is_finite())?),
            };
            if !start.is_finite() || !end.is_finite() || !(end - start).is_finite() {
                return None;
            }
            if let Some(i) = initial
                && (!i.is_finite() || i < start.min(end) || i > start.max(end))
            {
                return None;
            }
            Control::NumberRange {
                width: width(c, "width", 200)?,
                label: label()?,
                label_format: string_limited(c, "label_format", "options.generic_value")?,
                start,
                end,
                initial,
                step,
            }
        }
        _ => return None,
    };
    Some(Input {
        key: key.to_owned(),
        control,
    })
}

fn single_options(list: &List) -> Option<Vec<SingleOption>> {
    match list {
        List::String(ids) if ids.len() <= MAX_OPTIONS => Some(
            ids.iter()
                .map(|id| SingleOption {
                    id: id.clone(),
                    display: None,
                    initial: false,
                })
                .collect(),
        ),
        List::Compound(items) if items.len() <= MAX_OPTIONS => items
            .iter()
            .map(|o| match o.get("") {
                Some(Value::String(id)) => Some(SingleOption {
                    id: id.clone(),
                    display: None,
                    initial: false,
                }),
                Some(Value::Compound(inner)) => single_option(inner),
                _ => single_option(o),
            })
            .collect(),
        _ => None,
    }
}

fn single_option(o: &Compound) -> Option<SingleOption> {
    Some(SingleOption {
        id: o.get_str("id")?.to_owned(),
        display: o.get("display").map(component),
        initial: o.get_bool("initial").unwrap_or(false),
    })
}

/// Maps `$(name)` variables in a command template to the given values; unknown names become empty.
pub fn instantiate(template: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("$(") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find(')') {
            Some(end)
                if after[..end]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_') =>
            {
                out.push_str(&value(&after[..end]).unwrap_or_default());
                rest = &after[end + 1..];
            }
            _ => {
                out.push_str("$(");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Quotes-free escaping of a string the way a text input value is substituted into a command.
pub fn escape_without_quotes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c == '\\' || c == '"' || c == '\'' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snbt(text: &str) -> Value {
        mcr_nbt::snbt::parse(text).expect("snbt")
    }

    #[test]
    fn notice_with_defaults() {
        let d = Dialog::from_nbt(&snbt(
            r#"{type:"minecraft:notice",title:"Hello",body:{type:"minecraft:plain_message",contents:"text"}}"#,
        ))
        .unwrap();
        assert!(d.can_close_with_escape && d.pause);
        assert_eq!(d.after_action, AfterAction::Close);
        assert_eq!(d.body.len(), 1);
        let DialogKind::Notice { action } = &d.kind else {
            panic!()
        };
        assert_eq!(action.width, 150);
        assert!(action.action.is_none());
    }

    #[test]
    fn confirmation_with_actions_and_inputs() {
        let d = Dialog::from_nbt(&snbt(
            r#"{type:"confirmation",title:"Q",inputs:[
                {type:"text",key:"name",label:"Name",max_length:10,initial:"abc"},
                {type:"boolean",key:"ok",label:"OK",on_true:"yes"},
                {type:"single_option",key:"c",label:"C",options:[{"":"a"},{"":{id:"b",initial:1b}}]},
                {type:"number_range",key:"n",label:"N",start:0f,end:10f,step:2f}
            ],
            yes:{label:"Yes",action:{type:"dynamic/run_command",template:"say $(name)"}},
            no:{label:"No",action:{type:"dynamic/custom",id:"a:b"}}}"#,
        ))
        .unwrap();
        assert_eq!(d.inputs.len(), 4);
        let DialogKind::Confirmation { yes, no } = &d.kind else {
            panic!()
        };
        assert_eq!(
            yes.action,
            Some(Action::CommandTemplate("say $(name)".into()))
        );
        assert!(matches!(no.action, Some(Action::Custom { .. })));
    }

    #[test]
    fn static_actions_become_click_events() {
        let d = Dialog::from_nbt(&snbt(
            r#"{type:"multi_action",title:"T",pause:0b,actions:[{label:"a",action:{type:"open_url",url:"https://x.y"}}]}"#,
        ))
        .unwrap();
        let DialogKind::MultiAction { actions, .. } = &d.kind else {
            panic!()
        };
        assert_eq!(
            actions[0].action,
            Some(Action::Static(ClickEvent::OpenUrl("https://x.y".into())))
        );
    }

    #[test]
    fn pausing_dialogs_must_unpause_and_limits_hold() {
        assert!(
            Dialog::from_nbt(&snbt(r#"{type:"notice",title:"T",after_action:"none"}"#)).is_none()
        );
        assert!(Dialog::from_nbt(&snbt(r#"{type:"multi_action",title:"T",actions:[]}"#)).is_none());
        let many = format!(
            r#"{{type:"multi_action",title:"T",actions:[{}]}}"#,
            vec![r#"{label:"x"}"#; MAX_BUTTONS + 1].join(",")
        );
        assert!(Dialog::from_nbt(&snbt(&many)).is_none());
        assert!(Dialog::from_nbt(&snbt(r#"{type:"bogus",title:"T"}"#)).is_none());
        assert!(Dialog::from_nbt(&Value::String("x".into())).is_none());
    }

    #[test]
    fn bad_inputs_reject_the_dialog() {
        let bad = [
            r#"{type:"text",key:"has space",label:"x"}"#,
            r#"{type:"text",key:"k",label:"x",max_length:2,initial:"long"}"#,
            r#"{type:"number_range",key:"k",label:"x",start:0f,end:1f,initial:5f}"#,
            r#"{type:"single_option",key:"k",label:"x",options:[]}"#,
        ];
        for input in bad {
            let d = format!(r#"{{type:"notice",title:"T",inputs:[{input}]}}"#);
            assert!(Dialog::from_nbt(&snbt(&d)).is_none(), "{input}");
        }
    }

    #[test]
    fn templates_substitute_known_variables_only() {
        let value = |k: &str| (k == "a").then(|| "1".to_owned());
        assert_eq!(instantiate("x $(a) $(b) $(", value), "x 1  $(");
        assert_eq!(escape_without_quotes(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn hostile_sizes_are_rejected() {
        for body in [
            r#"{type:"multi_action",title:"T",columns:2000000000,actions:[{label:"a"}]}"#,
            r#"{type:"notice",title:"T",inputs:[{type:"text",key:"k",label:"L",multiline:{max_lines:2000000000}}]}"#,
            r#"{type:"notice",title:"T",inputs:[{type:"number_range",key:"k",label:"L",start:-3e38f,end:3e38f}]}"#,
            r#"{type:"notice",title:"T",inputs:[{type:"number_range",key:"k",label:"L",start:0f,end:1f,step:NaNf}]}"#,
        ] {
            assert!(Dialog::from_nbt(&snbt(body)).is_none(), "{body}");
        }
    }
}
