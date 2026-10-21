//! Incoming secure chat: vanilla's `handlePlayerChat`, `handleDeleteChat` and `ChatListener` display rules.

use mcr_auth::chat::SignedBody;
use mcr_buf::BitSet;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::clientbound::player::{
    BoundChatType, FilterMask, PackedSignature, PlayerChat,
};
use mcr_protocol::types::Component;
use mcr_protocol::types::game::MessageSignature;
use mcr_protocol::types::text::{Arg, Color, HoverEvent, NamedColor, Style};
use uuid::Uuid;

use crate::events::{ChatKind, ChatTag, ClientEvent, DisconnectReason};
use crate::secure_chat::{TrustLevel, Validated, evaluate_trust, now_millis};
use crate::social::ChatMessage;
use crate::state::{ClientState, Outbox};

pub const BAD_CHAT_INDEX: &str = "multiplayer.disconnect.bad_chat_index";
pub const INVALID_PACKET: &str = "multiplayer.disconnect.invalid_packet";
pub const DELETED_MARKER: &str = "chat.deleted_marker";

pub(crate) fn disconnect(out: &mut Outbox, key: &str) {
    out.event(ClientEvent::Disconnected(DisconnectReason::Server(
        Component::translatable(key, Vec::new()),
    )));
}

fn styled(text: Component, color: NamedColor, italic: bool) -> Component {
    text.styled(Style {
        color: Some(Color::Named(color)),
        italic: italic.then_some(true),
        ..Style::default()
    })
}

pub fn deleted_marker() -> Component {
    styled(
        Component::translatable(DELETED_MARKER, Vec::new()),
        NamedColor::Gray,
        true,
    )
}

/// Vanilla's `FilterMask.applyWithFormatting`: masked UTF-16 units become dark gray `#`.
pub fn apply_filter_mask(content: &str, mask: &BitSet) -> Component {
    let filtered_style = Style {
        color: Some(Color::Named(NamedColor::DarkGray)),
        hover_event: Some(Box::new(HoverEvent::ShowText(Box::new(
            Component::translatable("chat.filtered", Vec::new()),
        )))),
        ..Style::default()
    };
    let mut out = Component::text("");
    let mut run = String::new();
    let mut run_filtered = false;
    let mut unit = 0usize;
    let flush = |run: &mut String, filtered: bool, out: &mut Component| {
        if run.is_empty() {
            return;
        }
        let piece = Component::text(std::mem::take(run));
        out.extra.push(if filtered {
            piece.styled(filtered_style.clone())
        } else {
            piece
        });
    };
    for ch in content.chars() {
        let units = ch.len_utf16();
        let filtered = (unit..unit + units).any(|i| mask.get(i));
        unit += units;
        if filtered != run_filtered {
            flush(&mut run, run_filtered, &mut out);
            run_filtered = filtered;
        }
        if filtered {
            run.extend(std::iter::repeat_n('#', units));
        } else {
            run.push(ch);
        }
    }
    flush(&mut run, run_filtered, &mut out);
    out
}

impl ClientState {
    pub(crate) fn on_player_chat(&mut self, p: PlayerChat, out: &mut Outbox) {
        let now_ms = now_millis();
        let expected = self.secure.take_global_index();
        if p.global_index != expected {
            tracing::error!(
                expected,
                got = p.global_index,
                "missing or out-of-order chat message from the server"
            );
            disconnect(out, BAD_CHAT_INDEX);
            return;
        }
        let Some(last_seen) = self.secure.unpack_last_seen(&p.body.last_seen.0) else {
            disconnect(out, INVALID_PACKET);
            return;
        };
        let signature = p.signature.as_ref().map(|s| s.0);
        self.secure.cache_push(&last_seen, signature.as_ref());
        if self.players.get(p.sender).is_none() {
            self.chat_error(&p.chat_type, p.sender, signature.as_ref(), out);
            return;
        }
        let body = SignedBody {
            content: &p.body.content,
            timestamp_ms: p.body.timestamp_millis,
            salt: p.body.salt,
            last_seen: &last_seen,
        };
        match self
            .secure
            .validate(p.sender, p.index, signature.as_ref(), &body, now_ms)
        {
            Validated::Rejected => self.chat_error(&p.chat_type, p.sender, signature.as_ref(), out),
            Validated::Accepted { signed } => {
                let kept = signature.filter(|_| signed);
                self.show_player_chat(&p, kept, now_ms, out);
            }
        }
    }

    fn show_player_chat(
        &mut self,
        p: &PlayerChat,
        signature: Option<[u8; 256]>,
        now_ms: i64,
        out: &mut Outbox,
    ) {
        let only_secure = self.secure.only_show_secure;
        let unsigned = p.unsigned_content.as_ref().filter(|_| !only_secure);
        let content = unsigned
            .cloned()
            .unwrap_or_else(|| Component::text(p.body.content.clone()));
        let decorated = self.format_chat(&p.chat_type, content);
        let trust = if self.secure.local_server && p.sender == self.uuid {
            TrustLevel::Secure
        } else {
            evaluate_trust(
                signature.is_some(),
                p.body.timestamp_millis,
                now_ms,
                &p.body.content,
                &decorated,
                self.secure.renderer.as_ref(),
                p.unsigned_content.as_ref(),
            )
        };
        let shown_text = match &p.filter_mask {
            FilterMask::PassThrough => Some(decorated),
            FilterMask::FullyFiltered => None,
            FilterMask::PartiallyFiltered(mask) => {
                Some(self.format_chat(&p.chat_type, apply_filter_mask(&p.body.content, mask)))
            }
        };
        let shown = match shown_text {
            Some(text) if !(only_secure && trust == TrustLevel::NotSecure) => {
                let tag = match trust {
                    TrustLevel::Secure => None,
                    TrustLevel::Modified => Some(ChatTag::Modified {
                        original: p.body.content.clone(),
                    }),
                    TrustLevel::NotSecure => Some(ChatTag::NotSecure),
                };
                self.push_chat_tagged(
                    ChatKind::Player,
                    Some(p.sender),
                    text,
                    signature.map(|s| Box::new(MessageSignature(s))),
                    Some(p.body.timestamp_millis),
                    tag,
                    out,
                );
                true
            }
            _ => false,
        };
        if let Some(sig) = signature
            && let Some(ack) = self.secure.mark_processed(&sig, shown)
        {
            out.replies.push(S::ChatAck(ack));
        }
    }

    fn chat_error(
        &mut self,
        chat_type: &BoundChatType,
        sender: Uuid,
        signature: Option<&[u8; 256]>,
        out: &mut Outbox,
    ) {
        if let Some(sig) = signature
            && let Some(ack) = self.secure.mark_processed(sig, false)
        {
            out.replies.push(S::ChatAck(ack));
        }
        let error = styled(
            Component::translatable("chat.validation_error", Vec::<Arg>::new()),
            NamedColor::Red,
            true,
        );
        let decorated = self.format_chat(chat_type, error);
        self.push_chat_tagged(
            ChatKind::Player,
            Some(sender),
            decorated,
            None,
            None,
            Some(ChatTag::Error),
            out,
        );
    }

    pub(crate) fn on_delete_chat(&mut self, signature: &PackedSignature, out: &mut Outbox) {
        let Some(sig) = self.secure.unpack(signature) else {
            disconnect(out, INVALID_PACKET);
            return;
        };
        self.secure.ignore_pending(&sig);
        self.secure.drop_deferred(&sig);
        for message in &mut self.chat.history {
            if message.signature.as_deref().is_some_and(|s| s.0 == sig) {
                *message = ChatMessage {
                    kind: ChatKind::System,
                    sender: None,
                    content: deleted_marker(),
                    signature: None,
                    timestamp_millis: message.timestamp_millis,
                    tag: Some(ChatTag::System),
                };
            }
        }
        out.event(ClientEvent::ChatDeleted(Box::new(MessageSignature(sig))));
    }
}

#[cfg(test)]
#[path = "chat_in_tests.rs"]
mod tests;
