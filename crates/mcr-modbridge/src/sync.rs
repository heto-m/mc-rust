//! Settings Java follows: vanilla key bindings and the server packs mcr applied, as control messages.

use std::path::{Path, PathBuf};

use crate::abi::SYNC_MAX_LINES;

/// A settings message stays well inside the control ring.
pub const MAX_SYNC_BYTES: usize = 16 << 10;
/// Longer pack paths are left out rather than truncated.
const MAX_PATH_BYTES: usize = 1024;
/// A model request names at most this many bytes of blocks.
const MAX_MODELS_BYTES: usize = 64 << 10;

/// `name\tkey` lines; pairs with a tab, a newline or an empty side are skipped.
pub fn encode_keys<'a>(bindings: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, key) in bindings.into_iter().take(SYNC_MAX_LINES) {
        let clean = |s: &str| !s.is_empty() && !s.contains(['\t', '\n', '\r']);
        if !clean(name) || !clean(key) {
            continue;
        }
        let line = name.len() + key.len() + 2;
        if out.len() + line > MAX_SYNC_BYTES {
            break;
        }
        out.extend_from_slice(name.as_bytes());
        out.push(b'\t');
        out.extend_from_slice(key.as_bytes());
        out.push(b'\n');
    }
    out
}

/// One absolute path per line; relative paths are made absolute against the current directory.
pub fn encode_packs(paths: &[PathBuf]) -> Vec<u8> {
    let mut out = Vec::new();
    for path in paths.iter().take(SYNC_MAX_LINES) {
        let Some(text) = absolute(path) else {
            tracing::warn!(path = %path.display(), "server pack path not sent to Java");
            continue;
        };
        if out.len() + text.len() + 1 > MAX_SYNC_BYTES {
            break;
        }
        out.extend_from_slice(text.as_bytes());
        out.push(b'\n');
    }
    out
}

/// Block names for `MSG_MODELS`, one per line; names with a newline are left out.
pub fn encode_models<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<u8> {
    let mut out = Vec::new();
    for name in names {
        if name.is_empty()
            || name.contains(['\n', '\r'])
            || out.len() + name.len() + 1 > MAX_MODELS_BYTES
        {
            continue;
        }
        out.extend_from_slice(name.as_bytes());
        out.push(b'\n');
    }
    out
}

fn absolute(path: &Path) -> Option<String> {
    let abs = std::path::absolute(path).ok()?;
    let text = abs.to_str()?;
    (text.len() <= MAX_PATH_BYTES && !text.contains(['\n', '\r'])).then(|| text.to_owned())
}

/// What the next posted frame carries ahead of its input: each message once, the newest value wins.
#[derive(Debug, Default)]
pub(crate) struct Pending {
    keys: Option<Vec<u8>>,
    sent_keys: Option<Vec<u8>>,
    packs: Option<Vec<u8>>,
    models: Option<Vec<u8>>,
    world: crate::world::WorldQueue,
}

impl Pending {
    /// Queues `encoded` unless Java already has exactly that table.
    pub fn keys(&mut self, encoded: Vec<u8>) {
        if self.sent_keys.as_ref() != Some(&encoded) {
            self.keys = Some(encoded);
        }
    }

    pub fn packs(&mut self, encoded: Vec<u8>) {
        self.packs = Some(encoded);
    }

    pub fn models(&mut self, encoded: Vec<u8>) {
        self.models = Some(encoded);
    }

    pub fn world(&mut self, event: crate::world::WorldEvent) {
        self.world.push(event);
    }

    /// Pushes what is queued; anything the ring has no room for waits for the next frame.
    pub fn flush(&mut self, mut push: impl FnMut(u16, &[u8]) -> bool) {
        if let Some(keys) = self.keys.take() {
            if push(crate::abi::MSG_KEYS, &keys) {
                self.sent_keys = Some(keys);
            } else {
                self.keys = Some(keys);
            }
        }
        if let Some(packs) = self.packs.take()
            && !push(crate::abi::MSG_PACKS, &packs)
        {
            self.packs = Some(packs);
        }
        if let Some(models) = self.models.take()
            && !push(crate::abi::MSG_MODELS, &models)
        {
            self.models = Some(models);
        }
        while let Some(event) = self.world.front() {
            if !push(crate::abi::MSG_WORLD, &event.encode()) {
                break;
            }
            self.world.pop();
        }
    }

    /// Nothing waits to be sent.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.keys.is_none()
            && self.packs.is_none()
            && self.models.is_none()
            && self.world.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::{MSG_KEYS, MSG_PACKS};

    #[test]
    fn keys_skip_unsafe_pairs_and_stay_bounded() {
        let out = encode_keys([
            ("key.jump", "key.keyboard.space"),
            ("bad\tname", "key.keyboard.a"),
            ("key.forward", ""),
            ("key.left", "key.keyboard.a"),
        ]);
        assert_eq!(
            out,
            b"key.jump\tkey.keyboard.space\nkey.left\tkey.keyboard.a\n"
        );
        let long = "k".repeat(4000);
        let many: Vec<(&str, &str)> = (0..20).map(|_| (long.as_str(), "x")).collect();
        assert!(encode_keys(many).len() <= MAX_SYNC_BYTES);
    }

    #[test]
    fn packs_are_absolute_lines() {
        let out = encode_packs(&[PathBuf::from("downloads/abc"), PathBuf::from("/x/y")]);
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(Path::new(lines[0]).is_absolute() && lines[0].ends_with("downloads/abc"));
        assert_eq!(lines[1], "/x/y");
        assert!(encode_packs(&[PathBuf::from("/a\nb")]).is_empty());
        assert!(encode_packs(&[]).is_empty());
    }

    #[test]
    fn pending_sends_once_and_retries_when_full() {
        let mut p = Pending::default();
        p.keys(encode_keys([("key.jump", "key.keyboard.j")]));
        p.packs(Vec::new());
        let mut sent = Vec::new();
        p.flush(|kind, _| {
            sent.push(kind);
            kind == MSG_PACKS
        });
        assert_eq!(sent, [MSG_KEYS, MSG_PACKS]);
        assert!(!p.is_empty(), "keys wait for room");
        sent.clear();
        p.flush(|kind, _| {
            sent.push(kind);
            true
        });
        assert_eq!(sent, [MSG_KEYS]);
        p.keys(encode_keys([("key.jump", "key.keyboard.j")]));
        assert!(p.is_empty(), "an unchanged table is not sent again");
        p.keys(encode_keys([("key.jump", "key.keyboard.k")]));
        assert!(!p.is_empty());
    }

    #[test]
    fn world_messages_go_in_order_and_wait_for_room() {
        use crate::abi::MSG_WORLD;
        use crate::world::WorldEvent;
        let mut p = Pending::default();
        p.world(WorldEvent::Eula);
        p.world(WorldEvent::Ready);
        let mut sent = Vec::new();
        p.flush(|kind, payload| {
            if sent.is_empty() {
                sent.push((kind, payload.to_vec()));
                true
            } else {
                false
            }
        });
        assert_eq!(sent, [(MSG_WORLD, WorldEvent::Eula.encode())]);
        sent.clear();
        p.flush(|kind, payload| {
            sent.push((kind, payload.to_vec()));
            true
        });
        assert_eq!(sent, [(MSG_WORLD, WorldEvent::Ready.encode())]);
        assert!(p.is_empty());
    }
}
