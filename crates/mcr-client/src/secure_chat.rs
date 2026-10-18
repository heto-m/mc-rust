//! Vanilla's secure chat: the local chat session and chain, last-seen acknowledgements, the signature
//! cache, and verification of other players' sessions and messages.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use mcr_auth::chat::{
    ChatSigner, ChatVerifier, MAX_LAST_SEEN, MessageLink, ServicesKeys, SignedBody,
};
use mcr_auth::keys::{KeyRefresh, PlayerKeys};
use mcr_buf::{FixedBitSet, LengthPrefixed};
use mcr_protocol::play::clientbound::player::{PackedSignature, RemoteChatSession};
use mcr_protocol::play::serverbound as sb;
use mcr_protocol::play::serverbound::BoundedBytes;
use mcr_protocol::types::Component;
use mcr_protocol::types::game::MessageSignature;
use mcr_protocol::types::text::{Arg, Content};
use rustc_hash::FxHashMap;
use uuid::Uuid;

use crate::config::ClientConfig;

pub const LAST_SEEN: usize = MAX_LAST_SEEN;
pub const CACHE_SIZE: usize = 128;
const ACK_AFTER: i32 = 64;
const KEY_GRACE_MS: i64 = 8 * 3600 * 1000;
const CLIENT_EXPIRY_MS: i64 = 7 * 60 * 1000;
const REFRESH_RETRY: Duration = Duration::from_secs(600);
const MAX_SESSIONS: usize = 8192;
pub const MAX_COMMAND_SIGNATURES: usize = 8;
const DEFAULT_FONT: &str = "minecraft:default";

type Sig = [u8; 256];

/// Java's `Arrays.hashCode(byte[])` folded per vanilla's `LastSeenMessages.computeChecksum`.
pub fn checksum(entries: &[Sig]) -> u8 {
    let mut sum: i32 = 1;
    for entry in entries {
        let hash = entry.iter().fold(1i32, |h, &b| {
            h.wrapping_mul(31).wrapping_add(i32::from(b as i8))
        });
        sum = sum.wrapping_mul(31).wrapping_add(hash);
    }
    match sum as u8 {
        0 => 1,
        b => b,
    }
}

/// Vanilla's `LastSeenMessagesTracker` with a window of 20.
#[derive(Debug, Clone)]
pub struct LastSeenTracker {
    slots: [Option<(Box<Sig>, bool)>; LAST_SEEN],
    tail: usize,
    offset: i32,
    last: Option<Box<Sig>>,
}

impl Default for LastSeenTracker {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            tail: 0,
            offset: 0,
            last: None,
        }
    }
}

/// What one sent message acknowledges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackerUpdate {
    pub last_seen: Vec<Sig>,
    pub update: sb::LastSeenUpdate,
}

impl LastSeenTracker {
    /// False when `signature` repeats the last tracked one.
    pub fn add_pending(&mut self, signature: &Sig, shown: bool) -> bool {
        if self.last.as_deref() == Some(signature) {
            return false;
        }
        self.last = Some(Box::new(*signature));
        self.slots[self.tail] = shown.then(|| (Box::new(*signature), true));
        self.tail = (self.tail + 1) % LAST_SEEN;
        self.offset += 1;
        true
    }

    pub fn ignore_pending(&mut self, signature: &Sig) {
        if let Some(slot) = self.slots.iter_mut().find(|s| {
            s.as_ref()
                .is_some_and(|(sig, pending)| *pending && **sig == *signature)
        }) {
            *slot = None;
        }
    }

    pub fn offset(&self) -> i32 {
        self.offset
    }

    pub fn take_offset(&mut self) -> i32 {
        std::mem::take(&mut self.offset)
    }

    pub fn generate_update(&mut self) -> TrackerUpdate {
        let offset = self.take_offset();
        let mut acknowledged = FixedBitSet::new();
        let mut last_seen = Vec::new();
        for i in 0..LAST_SEEN {
            if let Some((sig, pending)) = &mut self.slots[(self.tail + i) % LAST_SEEN] {
                acknowledged.set(i, true);
                last_seen.push(**sig);
                *pending = false;
            }
        }
        let checksum = checksum(&last_seen);
        TrackerUpdate {
            last_seen,
            update: sb::LastSeenUpdate {
                offset,
                acknowledged,
                checksum,
            },
        }
    }
}

/// Vanilla's 128-entry `MessageSignatureCache`.
#[derive(Debug, Clone)]
pub struct SignatureCache {
    entries: Vec<Option<Box<Sig>>>,
}

impl Default for SignatureCache {
    fn default() -> Self {
        Self {
            entries: vec![None; CACHE_SIZE],
        }
    }
}

impl SignatureCache {
    pub fn get(&self, id: i32) -> Option<&Sig> {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.entries.get(i))
            .and_then(|e| e.as_deref())
    }

    pub fn unpack(&self, packed: &PackedSignature) -> Option<Sig> {
        match packed {
            PackedSignature::Cached(id) => self.get(*id).copied(),
            PackedSignature::Full(sig) => Some(sig.0),
        }
    }

    /// The last-seen signatures then the message's own go to the front, older entries shift back.
    pub fn push(&mut self, last_seen: &[Sig], own: Option<&Sig>) {
        let mut queue: VecDeque<Box<Sig>> = last_seen
            .iter()
            .chain(own)
            .take(LAST_SEEN + 1)
            .map(|s| Box::new(*s))
            .collect();
        let fresh: Vec<Sig> = queue.iter().map(|s| **s).collect();
        for slot in &mut self.entries {
            let Some(next) = queue.pop_back() else {
                break;
            };
            if let Some(old) = slot.replace(next)
                && !fresh.contains(&old)
            {
                queue.push_front(old);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct LocalKeys {
    keys: PlayerKeys,
    signer: ChatSigner,
    public_der: Vec<u8>,
    signature_v2: Vec<u8>,
}

impl LocalKeys {
    fn parse(keys: &PlayerKeys) -> Option<Self> {
        let signer = ChatSigner::new(keys.private_key().ok()?)?;
        let public_der = keys.public_key_der().ok()?;
        let signature_v2 = keys.signature_v2_bytes().ok()?;
        (public_der.len() <= 512 && signature_v2.len() <= 4096).then(|| Self {
            keys: keys.clone(),
            signer,
            public_der,
            signature_v2,
        })
    }
}

#[derive(Debug, Clone)]
struct LocalSession {
    id: Uuid,
    next_index: Option<i32>,
}

#[derive(Debug, Clone)]
struct RemoteSession {
    id: Uuid,
    verifier: ChatVerifier,
    expires_at_ms: i64,
    last: Option<(i32, Box<Sig>)>,
    chain_valid: bool,
}

/// Vanilla's chat trust level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustLevel {
    Secure,
    Modified,
    NotSecure,
}

/// The outcome of validating one player chat message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Validated {
    /// Shown; `signed` is false when the signature was stripped (no session, secure chat not enforced).
    Accepted {
        signed: bool,
    },
    Rejected,
}

/// A packet the session code asks the client to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatReply {
    Session(sb::ChatSessionUpdate),
    Ack(sb::ChatAck),
}

#[derive(Debug)]
pub struct SecureChat {
    profile: Uuid,
    keys: Option<LocalKeys>,
    services: Arc<ServicesKeys>,
    refresh: Option<KeyRefresh>,
    refresh_job: Option<Receiver<Option<PlayerKeys>>>,
    last_refresh: Option<Instant>,
    session: Option<LocalSession>,
    tracker: LastSeenTracker,
    cache: SignatureCache,
    remote: FxHashMap<Uuid, RemoteSession>,
    next_global_index: i32,
    online_mode: bool,
    server_enforces: bool,
    pub only_show_secure: bool,
    pub local_server: bool,
    warned_insecure: bool,
    pub renderer: Option<TextRenderer>,
}

pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn random_uuid() -> Uuid {
    uuid::Builder::from_random_bytes(rand::random()).into_uuid()
}

impl SecureChat {
    pub fn new(config: &ClientConfig, profile: Uuid) -> Self {
        let online = config.online.as_ref();
        Self {
            profile,
            keys: online
                .and_then(|o| o.keys.as_ref())
                .and_then(LocalKeys::parse),
            services: online.map(|o| o.services.clone()).unwrap_or_default(),
            refresh: online.and_then(|o| o.refresh.clone()),
            refresh_job: None,
            last_refresh: None,
            session: None,
            tracker: LastSeenTracker::default(),
            cache: SignatureCache::default(),
            remote: FxHashMap::default(),
            next_global_index: 0,
            online_mode: false,
            server_enforces: false,
            only_show_secure: config.only_show_secure_chat,
            local_server: config.local_server,
            warned_insecure: false,
            renderer: config.text_renderer.clone(),
        }
    }

    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }

    pub fn session_id(&self) -> Option<Uuid> {
        self.session.as_ref().map(|s| s.id)
    }

    /// Vanilla's `enforcesSecureChat`: the server enforces it and this client can check keys.
    pub fn enforces_secure_chat(&self) -> bool {
        self.server_enforces && !self.services.is_empty()
    }

    /// Vanilla's once-per-connection toast for a remote server that does not enforce secure chat.
    pub fn should_warn_insecure(&mut self) -> bool {
        let warn = !self.local_server && !self.warned_insecure && !self.enforces_secure_chat();
        self.warned_insecure |= warn;
        warn
    }

    pub fn tracker(&self) -> &LastSeenTracker {
        &self.tracker
    }

    /// Vanilla's `handleLogin` reset; returns the session announcement when one starts.
    pub fn on_login(
        &mut self,
        online_mode: bool,
        enforces_secure_chat: bool,
        now_ms: i64,
    ) -> Option<sb::ChatSessionUpdate> {
        self.session = None;
        self.next_global_index = 0;
        self.tracker = LastSeenTracker::default();
        self.cache = SignatureCache::default();
        self.remote.clear();
        self.online_mode = online_mode;
        self.server_enforces = enforces_secure_chat;
        if online_mode {
            self.start_session(now_ms)
        } else {
            None
        }
    }

    fn start_session(&mut self, now_ms: i64) -> Option<sb::ChatSessionUpdate> {
        let keys = self.keys.as_ref()?;
        if now_ms >= keys.keys.expires_at_ms {
            return None;
        }
        let id = random_uuid();
        self.session = Some(LocalSession {
            id,
            next_index: Some(0),
        });
        Some(sb::ChatSessionUpdate {
            session_id: id,
            expires_at_ms: keys.keys.expires_at_ms,
            public_key: BoundedBytes(keys.public_der.clone()),
            key_signature: BoundedBytes(keys.signature_v2.clone()),
        })
    }

    /// Replaces the keys (an in-game refresh); a new session starts when they differ and one was running.
    pub fn set_keys(&mut self, keys: &PlayerKeys, now_ms: i64) -> Option<sb::ChatSessionUpdate> {
        if self.keys.as_ref().is_some_and(|k| k.keys == *keys) {
            return None;
        }
        self.keys = LocalKeys::parse(keys);
        if self.online_mode && self.keys.is_some() {
            self.start_session(now_ms)
        } else {
            None
        }
    }

    /// Starts or collects an in-game key refresh; call about once a second.
    pub fn poll_refresh(&mut self, now: Instant, now_ms: i64) -> Option<sb::ChatSessionUpdate> {
        if let Some(rx) = &self.refresh_job {
            return match rx.try_recv() {
                Ok(keys) => {
                    self.refresh_job = None;
                    keys.and_then(|k| self.set_keys(&k, now_ms))
                }
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    self.refresh_job = None;
                    None
                }
            };
        }
        let due = self.session.is_some()
            && self
                .keys
                .as_ref()
                .is_some_and(|k| now_ms >= k.keys.refreshed_after_ms);
        let rested = self
            .last_refresh
            .is_none_or(|t| now.duration_since(t) >= REFRESH_RETRY);
        if let (true, true, Some(refresh)) = (due, rested, &self.refresh) {
            self.last_refresh = Some(now);
            let (tx, rx) = channel();
            let refresh = refresh.clone();
            let spawned = std::thread::Builder::new()
                .name("mcr-key-refresh".into())
                .spawn(move || {
                    let _ = tx.send(refresh.fetch());
                });
            if spawned.is_ok() {
                self.refresh_job = Some(rx);
            }
        }
        None
    }

    fn sign(&mut self, body: &SignedBody<'_>) -> Option<MessageSignature> {
        let keys = self.keys.as_ref()?;
        let session = self.session.as_mut()?;
        let index = session.next_index?;
        session.next_index = index.checked_add(1);
        let link = MessageLink {
            sender: self.profile,
            session: session.id,
            index,
        };
        keys.signer.sign(&link, body).map(MessageSignature)
    }

    /// Vanilla's `sendChat`: the tracker update always applies; the signature only with a session.
    pub fn chat(&mut self, message: &str, now_ms: i64) -> sb::Chat {
        let salt: i64 = rand::random();
        let update = self.tracker.generate_update();
        let signature = self.sign(&SignedBody {
            content: message,
            timestamp_ms: now_ms,
            salt,
            last_seen: &update.last_seen,
        });
        sb::Chat {
            message: message.to_owned(),
            timestamp_ms: now_ms,
            salt,
            signature,
            last_seen: update.update,
        }
    }

    /// `arguments` are the command's `minecraft:message` arguments as (name, text).
    pub fn signed_command(
        &mut self,
        command: &str,
        arguments: &[(String, String)],
        now_ms: i64,
    ) -> sb::ChatCommandSigned {
        let salt: i64 = rand::random();
        let update = self.tracker.generate_update();
        let mut signatures = Vec::new();
        for (name, value) in arguments.iter().take(MAX_COMMAND_SIGNATURES) {
            let body = SignedBody {
                content: value,
                timestamp_ms: now_ms,
                salt,
                last_seen: &update.last_seen,
            };
            if let Some(signature) = self.sign(&body) {
                signatures.push(sb::ArgumentSignature {
                    name: name.chars().take(16).collect(),
                    signature,
                });
            }
        }
        sb::ChatCommandSigned {
            command: command.to_owned(),
            timestamp_ms: now_ms,
            salt,
            argument_signatures: LengthPrefixed(signatures),
            last_seen: update.update,
        }
    }

    /// `initialize_chat` for a listed player: a valid session replaces the old one, anything else clears it.
    pub fn init_session(&mut self, player: Uuid, data: Option<&RemoteChatSession>) {
        self.remote.remove(&player);
        let Some(data) = data else {
            return;
        };
        if self.remote.len() >= MAX_SESSIONS
            || !self.services.validate_profile_key(
                player,
                data.expires_at_millis,
                &data.public_key.0,
                &data.key_signature.0,
            )
        {
            return;
        }
        if let Ok(verifier) = ChatVerifier::from_der(&data.public_key.0) {
            self.remote.insert(
                player,
                RemoteSession {
                    id: data.session_id,
                    verifier,
                    expires_at_ms: data.expires_at_millis,
                    last: None,
                    chain_valid: true,
                },
            );
        }
    }

    pub fn has_remote_session(&self, player: Uuid) -> bool {
        self.remote.contains_key(&player)
    }

    pub fn remove_players(&mut self, players: &[Uuid]) {
        for p in players {
            self.remote.remove(p);
        }
    }

    /// The next expected `global_index`, advancing it like vanilla's `nextChatIndex++`.
    pub fn take_global_index(&mut self) -> i32 {
        let expected = self.next_global_index;
        self.next_global_index = self.next_global_index.wrapping_add(1);
        expected
    }

    /// The body's last-seen signatures; `None` when a cache id is unknown.
    pub fn unpack_last_seen(&self, packed: &[PackedSignature]) -> Option<Vec<Sig>> {
        packed
            .iter()
            .take(LAST_SEEN)
            .map(|p| self.cache.unpack(p))
            .collect()
    }

    pub fn unpack(&self, packed: &PackedSignature) -> Option<Sig> {
        self.cache.unpack(packed)
    }

    pub fn cache_push(&mut self, last_seen: &[Sig], own: Option<&Sig>) {
        self.cache.push(last_seen, own);
    }

    /// Vanilla's per-sender message validator.
    pub fn validate(
        &mut self,
        sender: Uuid,
        index: i32,
        signature: Option<&Sig>,
        body: &SignedBody<'_>,
        now_ms: i64,
    ) -> Validated {
        let enforces = self.enforces_secure_chat();
        let Some(session) = self.remote.get_mut(&sender) else {
            return if enforces {
                Validated::Rejected
            } else {
                Validated::Accepted { signed: false }
            };
        };
        let duplicate = session
            .last
            .as_ref()
            .is_some_and(|(i, s)| *i == index && Some(&**s) == signature);
        let valid = session.chain_valid
            && session.expires_at_ms.saturating_add(KEY_GRACE_MS) >= now_ms
            && signature.is_some_and(|sig| {
                let link = MessageLink {
                    sender,
                    session: session.id,
                    index,
                };
                session.verifier.verify(&link, body, sig)
            })
            && (duplicate || session.last.as_ref().is_none_or(|(i, _)| index > *i));
        session.chain_valid = valid;
        if !valid {
            return Validated::Rejected;
        }
        if let Some(sig) = signature {
            session.last = Some((index, Box::new(*sig)));
        }
        Validated::Accepted { signed: true }
    }

    /// Vanilla's `markMessageAsProcessed`: an ack once more than 64 are unacknowledged.
    pub fn mark_processed(&mut self, signature: &Sig, shown: bool) -> Option<sb::ChatAck> {
        if self.tracker.add_pending(signature, shown) && self.tracker.offset() > ACK_AFTER {
            self.take_ack()
        } else {
            None
        }
    }

    pub fn ignore_pending(&mut self, signature: &Sig) {
        self.tracker.ignore_pending(signature);
    }

    /// The pending acknowledgement, if any (vanilla's `sendChatAcknowledgement`).
    pub fn take_ack(&mut self) -> Option<sb::ChatAck> {
        let offset = self.tracker.take_offset();
        (offset > 0).then_some(sb::ChatAck { offset })
    }
}

/// The text a component shows on screen, from the UI's own flattening with its language, so a trust check
/// sees exactly what the player reads.
#[derive(Clone)]
pub struct TextRenderer(Arc<dyn Fn(&Component) -> String + Send + Sync>);

impl TextRenderer {
    pub fn new(render: impl Fn(&Component) -> String + Send + Sync + 'static) -> Self {
        Self(Arc::new(render))
    }

    pub fn render(&self, component: &Component) -> String {
        (self.0)(component)
    }
}

impl std::fmt::Debug for TextRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TextRenderer")
    }
}

/// Without a language a translation's shown text is unknown, so only literal text without `§` codes counts.
fn shown_without_language(component: &Component) -> Option<String> {
    fn literal_only(c: &Component, depth: usize) -> bool {
        depth < 64
            && match &c.content {
                Content::Translatable { .. } => false,
                Content::Text(t) => !t.contains('§'),
                Content::Object {
                    fallback: Some(f), ..
                } => literal_only(f, depth + 1),
                _ => true,
            }
            && c.extra.iter().all(|e| literal_only(e, depth + 1))
    }
    literal_only(component, 0).then(|| component.to_plain_string())
}

fn uses_custom_font(component: &Component, depth: usize) -> bool {
    if depth > 32 {
        return false;
    }
    let custom = component
        .style
        .font
        .as_ref()
        .is_some_and(|f| f.as_str() != DEFAULT_FONT);
    let in_args = match &component.content {
        Content::Translatable { args, .. } => args.iter().any(|a| match a {
            Arg::Component(c) => uses_custom_font(c, depth + 1),
            _ => false,
        }),
        _ => false,
    };
    custom
        || in_args
        || component
            .extra
            .iter()
            .any(|c| uses_custom_font(c, depth + 1))
}

/// Vanilla's `ChatTrustLevel.evaluate`, on the decorated line as it is drawn (`renderer`); without a
/// renderer a line with translations counts as modified.
pub fn evaluate_trust(
    signed: bool,
    timestamp_ms: i64,
    now_ms: i64,
    signed_content: &str,
    decorated: &Component,
    renderer: Option<&TextRenderer>,
    unsigned_content: Option<&Component>,
) -> TrustLevel {
    if !signed || now_ms > timestamp_ms.saturating_add(CLIENT_EXPIRY_MS) {
        return TrustLevel::NotSecure;
    }
    let shown = match renderer {
        Some(r) => Some(r.render(decorated)),
        None => shown_without_language(decorated),
    };
    if !shown.is_some_and(|text| text.contains(signed_content))
        || unsigned_content.is_some_and(|c| uses_custom_font(c, 0))
    {
        TrustLevel::Modified
    } else {
        TrustLevel::Secure
    }
}

#[cfg(test)]
#[path = "secure_chat_tests.rs"]
pub(crate) mod tests;
