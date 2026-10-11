use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use mcr_buf::Identifier;
use mcr_nbt::{Compound, List, Value};
use mcr_protocol::types::Component;

use crate::registry::Registries;

pub const DIALOG_REGISTRY: &str = "minecraft:dialog";
const MAX_SUB_DIALOGS: usize = 64;

/// What the server asked the app to show.
#[derive(Debug, Clone, PartialEq)]
pub enum DialogRequest {
    Show(Compound),
    Clear,
}

/// What a dialog screen wants sent back to the server or done to the connection.
#[derive(Debug, Clone, PartialEq)]
pub enum DialogReply {
    Custom {
        id: Identifier,
        payload: Option<Value>,
    },
    Command(String),
    /// A dialog reference (registry id string or inline compound) to resolve and show.
    Open(Value),
    Disconnect(Component),
}

/// The client's half of the dialog channel pair; it works in configuration and play alike.
#[derive(Debug, Clone)]
pub struct DialogLink {
    pub(crate) requests: Sender<DialogRequest>,
    replies: Arc<Mutex<Receiver<DialogReply>>>,
}

impl DialogLink {
    pub(crate) fn next_reply(&self) -> Option<DialogReply> {
        self.replies.lock().ok()?.try_recv().ok()
    }
}

/// The app's half: read requests, answer with replies.
#[derive(Debug)]
pub struct DialogEnd {
    pub requests: Receiver<DialogRequest>,
    replies: Sender<DialogReply>,
}

impl DialogEnd {
    pub fn reply(&self, reply: DialogReply) {
        let _ = self.replies.send(reply);
    }
}

pub fn dialog_channel() -> (DialogLink, DialogEnd) {
    let (request_tx, request_rx) = channel();
    let (reply_tx, reply_rx) = channel();
    (
        DialogLink {
            requests: request_tx,
            replies: Arc::new(Mutex::new(reply_rx)),
        },
        DialogEnd {
            requests: request_rx,
            replies: reply_tx,
        },
    )
}

fn registry_dialog(registries: &Registries, id: &str) -> Option<Compound> {
    let registry = registries.get(DIALOG_REGISTRY)?;
    let index = registry.id_of(id)?;
    registry.get(i32::try_from(index).ok()?)?.data.clone()
}

/// The dialog compound a registry network id names.
pub fn registry_dialog_by_index(registries: &Registries, index: i32) -> Option<Compound> {
    let mut dialog = registries.get(DIALOG_REGISTRY)?.get(index)?.data.clone()?;
    inline_sub_dialogs(registries, &mut dialog);
    Some(dialog)
}

/// A dialog compound for a reference: an inline compound as is, a string looked up in the dialog registry.
pub fn resolve_dialog(registries: &Registries, value: &Value) -> Option<Compound> {
    let mut dialog = match value {
        Value::Compound(c) => c.clone(),
        Value::String(id) => registry_dialog(registries, id)?,
        _ => return None,
    };
    inline_sub_dialogs(registries, &mut dialog);
    Some(dialog)
}

fn tag_members(registries: &Registries, tag: &str) -> Vec<Compound> {
    let Some(registry) = registries.get(DIALOG_REGISTRY) else {
        return Vec::new();
    };
    registries
        .tag(DIALOG_REGISTRY, tag)
        .unwrap_or_default()
        .iter()
        .filter_map(|id| registry.get(*id)?.data.clone())
        .collect()
}

/// Replaces `dialogs` references (ids, tags, wrapped entries) by the compounds they name, one level deep.
fn inline_sub_dialogs(registries: &Registries, dialog: &mut Compound) {
    let Some(value) = dialog.get("dialogs") else {
        return;
    };
    let entries: Vec<Value> = match value {
        Value::String(_) | Value::Compound(_) => vec![value.clone()],
        Value::List(List::String(items)) => items.iter().cloned().map(Value::String).collect(),
        Value::List(List::Compound(items)) => items.iter().cloned().map(Value::Compound).collect(),
        _ => Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.into_iter().take(MAX_SUB_DIALOGS) {
        let entry = match entry {
            Value::Compound(c) => match c.get("") {
                Some(inner) => inner.clone(),
                None => Value::Compound(c),
            },
            other => other,
        };
        match entry {
            Value::String(id) => match id.strip_prefix('#') {
                Some(tag) => out.extend(tag_members(registries, tag)),
                None => out.extend(registry_dialog(registries, &id)),
            },
            Value::Compound(c) => out.push(c),
            _ => {}
        }
        if out.len() >= MAX_SUB_DIALOGS {
            out.truncate(MAX_SUB_DIALOGS);
            break;
        }
    }
    dialog.insert("dialogs", Value::List(List::Compound(out)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snbt(text: &str) -> Value {
        mcr_nbt::snbt::parse(text).unwrap()
    }

    #[test]
    fn inline_compounds_pass_through_and_strings_need_the_registry() {
        let registries = Registries::default();
        let inline = snbt(r#"{type:"notice",title:"x"}"#);
        assert_eq!(
            resolve_dialog(&registries, &inline),
            inline.as_compound().cloned()
        );
        assert!(resolve_dialog(&registries, &Value::String("a:b".into())).is_none());
        assert!(resolve_dialog(&registries, &Value::Int(1)).is_none());
    }

    #[test]
    fn dialog_lists_inline_their_entries_and_are_bounded() {
        let registries = Registries::default();
        let list = snbt(
            r#"{type:"dialog_list",title:"x",dialogs:[{"":"missing:one"},{"":{type:"notice",title:"y"}}]}"#,
        );
        let resolved = resolve_dialog(&registries, &list).unwrap();
        let List::Compound(items) = resolved.get_list("dialogs").unwrap() else {
            panic!()
        };
        assert_eq!(items.len(), 1);
        let many = format!(
            r#"{{type:"dialog_list",title:"x",dialogs:[{}]}}"#,
            vec![r#"{type:"notice",title:"y"}"#; 100].join(",")
        );
        let resolved = resolve_dialog(&registries, &snbt(&many)).unwrap();
        assert_eq!(resolved.get_list("dialogs").unwrap().len(), MAX_SUB_DIALOGS);
    }
}
