use std::time::Instant;

use mcr_auth::chat::SignedBody;
use mcr_buf::{BitSet, IdOr, LengthPrefixed};
use mcr_protocol::dispatch::PlayClientbound as P;
use mcr_protocol::dispatch::PlayServerbound as S;
use mcr_protocol::play::clientbound::player::{
    BoundChatType, DeleteChat, FilterMask, PackedSignature, PlayerChat, PlayerInfoAction,
    PlayerInfoEntry, PlayerInfoUpdate, SignedMessageBody, StartConfiguration,
};
use mcr_protocol::types::Component;
use mcr_protocol::types::game::MessageSignature;
use mcr_protocol::types::text::Content;
use uuid::Uuid;

use super::*;
use crate::config::ClientConfig;
use crate::secure_chat::now_millis;
use crate::secure_chat::tests::{
    PROFILE, online_config, other_player_key, remote_session, sign_as, valid_keys,
};

const ALEX: Uuid = Uuid::from_u128(42);
const SESSION: Uuid = Uuid::from_u128(4242);

fn add_player(s: &mut ClientState, uuid: Uuid, name: &str, session: bool) {
    let mut entry = PlayerInfoEntry::new(uuid);
    entry.profile.name = name.into();
    if session {
        entry.chat_session = Some(remote_session(
            uuid,
            other_player_key(),
            SESSION,
            now_millis() + 3_600_000,
        ));
    }
    run(
        s,
        P::PlayerInfoUpdate(PlayerInfoUpdate::with_actions(
            &[
                PlayerInfoAction::AddPlayer,
                PlayerInfoAction::InitializeChat,
            ],
            vec![entry],
        )),
    );
}

fn offline_state() -> ClientState {
    let mut s = ClientState::new(&ClientConfig::default(), PROFILE);
    s.secure.on_login(false, false, now_millis());
    add_player(&mut s, ALEX, "Alex", false);
    s
}

fn online_state(config: ClientConfig) -> ClientState {
    let mut s = ClientState::new(&config, PROFILE);
    s.secure.on_login(true, true, now_millis());
    add_player(&mut s, ALEX, "Alex", true);
    s
}

fn run(s: &mut ClientState, packet: P<'_>) -> Outbox {
    let mut out = Outbox::default();
    s.handle_play(packet, &mut out, Instant::now()).unwrap();
    out
}

fn chat(global: i32, index: i32, content: &str, signature: Option<[u8; 256]>) -> PlayerChat {
    PlayerChat {
        global_index: global,
        sender: ALEX,
        index,
        signature: signature.map(MessageSignature),
        body: SignedMessageBody {
            content: content.into(),
            timestamp_millis: now_millis(),
            salt: 11,
            last_seen: LengthPrefixed(Vec::new()),
        },
        unsigned_content: None,
        filter_mask: FilterMask::PassThrough,
        chat_type: BoundChatType {
            chat_type: IdOr::Id(0),
            name: Component::text("Alex"),
            target_name: None,
        },
    }
}

fn signed(global: i32, index: i32, content: &str) -> PlayerChat {
    let mut p = chat(global, index, content, None);
    let body = SignedBody {
        content,
        timestamp_ms: p.body.timestamp_millis,
        salt: p.body.salt,
        last_seen: &[],
    };
    p.signature = Some(MessageSignature(sign_as(
        other_player_key(),
        ALEX,
        SESSION,
        index,
        &body,
    )));
    p
}

fn shown(out: &Outbox) -> Vec<(String, Option<ChatTag>, bool)> {
    out.events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::Chat {
                message,
                tag,
                signature,
                ..
            } => Some((message.to_plain_string(), tag.clone(), signature.is_some())),
            _ => None,
        })
        .collect()
}

fn disconnect_key(out: &Outbox) -> Option<String> {
    out.events.iter().find_map(|e| match e {
        ClientEvent::Disconnected(DisconnectReason::Server(c)) => match &c.content {
            Content::Translatable { key, .. } => Some(key.clone()),
            _ => None,
        },
        _ => None,
    })
}

fn acks(out: &Outbox) -> Vec<i32> {
    out.replies
        .iter()
        .filter_map(|r| match r {
            S::ChatAck(a) => Some(a.offset),
            _ => None,
        })
        .collect()
}

#[test]
fn offline_chat_is_shown_not_secure() {
    let mut s = offline_state();
    let out = run(&mut s, P::PlayerChat(chat(0, 0, "hello", None)));
    assert_eq!(
        shown(&out),
        [("Alex hello".to_owned(), Some(ChatTag::NotSecure), false)]
    );
    assert!(out.replies.is_empty());
    assert_eq!(s.chat.history[0].tag, Some(ChatTag::NotSecure));
}

#[test]
fn signature_from_a_sender_without_session_is_stripped_offline() {
    let mut s = offline_state();
    let out = run(&mut s, P::PlayerChat(chat(0, 0, "hello", Some([3; 256]))));
    assert_eq!(shown(&out)[0].1, Some(ChatTag::NotSecure));
    assert!(acks(&out).is_empty());
    assert_eq!(s.secure.tracker().offset(), 0);
}

#[test]
fn unknown_sender_shows_a_chat_error() {
    let mut s = offline_state();
    let mut p = chat(0, 0, "hi", None);
    p.sender = Uuid::from_u128(7);
    let out = run(&mut s, P::PlayerChat(p));
    let lines = shown(&out);
    assert_eq!(lines[0].1, Some(ChatTag::Error));
    assert!(lines[0].0.contains("chat.validation_error"));
}

#[test]
fn chat_index_gap_disconnects() {
    let mut s = offline_state();
    run(&mut s, P::PlayerChat(chat(0, 0, "a", None)));
    let out = run(&mut s, P::PlayerChat(chat(2, 0, "b", None)));
    assert_eq!(disconnect_key(&out).as_deref(), Some(BAD_CHAT_INDEX));
    assert!(shown(&out).is_empty());
}

#[test]
fn unknown_cache_id_disconnects() {
    let mut s = offline_state();
    let mut p = chat(0, 0, "a", None);
    p.body.last_seen = LengthPrefixed(vec![PackedSignature::Cached(500)]);
    let out = run(&mut s, P::PlayerChat(p));
    assert_eq!(disconnect_key(&out).as_deref(), Some(INVALID_PACKET));
}

#[test]
fn verified_chat_has_no_tag_and_is_acknowledged() {
    let mut s = online_state(online_config(Some(valid_keys())));
    let out = run(&mut s, P::PlayerChat(signed(0, 0, "hello")));
    assert_eq!(shown(&out), [("Alex hello".to_owned(), None, true)]);
    assert_eq!(s.secure.tracker().offset(), 1);
    let packet = s.secure.chat("reply", now_millis());
    assert_eq!(packet.last_seen.offset, 1);
    assert!(packet.last_seen.acknowledged.get(19));
}

#[test]
fn forged_chat_shows_an_error_and_breaks_the_sender() {
    let mut s = online_state(online_config(Some(valid_keys())));
    let mut forged = signed(0, 0, "hello");
    forged.body.content = "goodbye".into();
    let out = run(&mut s, P::PlayerChat(forged));
    assert_eq!(shown(&out)[0].1, Some(ChatTag::Error));
    assert_eq!(s.secure.tracker().offset(), 1, "processed but not shown");
    let out = run(&mut s, P::PlayerChat(signed(1, 1, "legit")));
    assert_eq!(shown(&out)[0].1, Some(ChatTag::Error));
}

#[test]
fn decoration_that_drops_the_signed_text_is_modified() {
    let mut s = online_state(online_config(Some(valid_keys())));
    let mut p = signed(0, 0, "hi");
    p.unsigned_content = Some(Component::text("hello"));
    let out = run(&mut s, P::PlayerChat(p));
    assert_eq!(
        shown(&out)[0].1,
        Some(ChatTag::Modified {
            original: "hi".into()
        })
    );
}

#[test]
fn only_show_secure_chat_hides_unsigned_and_strips_decoration() {
    let mut config = online_config(Some(valid_keys()));
    config.only_show_secure_chat = true;
    let mut s = online_state(config);
    let steve = Uuid::from_u128(43);
    add_player(&mut s, steve, "Steve", false);
    s.secure.on_login(true, false, now_millis());
    add_player(&mut s, ALEX, "Alex", true);
    add_player(&mut s, steve, "Steve", false);
    let mut p = chat(0, 0, "plain", None);
    p.sender = steve;
    assert!(shown(&run(&mut s, P::PlayerChat(p))).is_empty());

    let mut p = signed(1, 0, "hi");
    p.unsigned_content = Some(Component::text("decorated hi"));
    let out = run(&mut s, P::PlayerChat(p));
    assert_eq!(shown(&out), [("Alex hi".to_owned(), None, true)]);
}

#[test]
fn filter_masks_hide_or_hash() {
    let mut s = online_state(online_config(Some(valid_keys())));
    let mut p = signed(0, 0, "bad word");
    p.filter_mask = FilterMask::FullyFiltered;
    let out = run(&mut s, P::PlayerChat(p));
    assert!(shown(&out).is_empty());
    assert_eq!(s.secure.tracker().offset(), 1);

    let mut mask = BitSet::new();
    for i in 0..3 {
        mask.set(i, true);
    }
    let mut p = signed(1, 1, "bad word");
    p.filter_mask = FilterMask::PartiallyFiltered(mask);
    let out = run(&mut s, P::PlayerChat(p));
    assert_eq!(shown(&out)[0].0, "Alex ### word");
}

#[test]
fn filter_mask_counts_utf16_units() {
    let mut mask = BitSet::new();
    mask.set(1, true);
    mask.set(2, true);
    let c = apply_filter_mask("a😀b", &mask);
    assert_eq!(c.to_plain_string(), "a##b");
}

#[test]
fn sixty_five_signed_messages_send_one_ack() {
    let mut s = online_state(online_config(Some(valid_keys())));
    let mut sent = Vec::new();
    for i in 0..65 {
        sent.extend(acks(&run(
            &mut s,
            P::PlayerChat(signed(i, i, &format!("m{i}"))),
        )));
    }
    assert_eq!(sent, [65]);
}

#[test]
fn delete_chat_replaces_the_line_and_clears_pending() {
    let mut s = online_state(online_config(Some(valid_keys())));
    let p = signed(0, 0, "oops");
    let sig = p.signature.clone().unwrap();
    run(&mut s, P::PlayerChat(p));
    let out = run(
        &mut s,
        P::DeleteChat(DeleteChat {
            message_signature: PackedSignature::Cached(0),
        }),
    );
    assert!(
        out.events
            .iter()
            .any(|e| matches!(e, ClientEvent::ChatDeleted(d) if **d == sig))
    );
    assert_eq!(s.chat.history[0].content, deleted_marker());
    assert_eq!(s.chat.history[0].tag, Some(ChatTag::System));

    let out = run(
        &mut s,
        P::DeleteChat(DeleteChat {
            message_signature: PackedSignature::Cached(90),
        }),
    );
    assert_eq!(disconnect_key(&out).as_deref(), Some(INVALID_PACKET));
}

#[test]
fn deleting_before_the_ack_drops_the_pending_entry() {
    let mut s = online_state(online_config(Some(valid_keys())));
    let p = signed(0, 0, "oops");
    let sig = p.signature.clone().unwrap();
    run(&mut s, P::PlayerChat(p));
    run(
        &mut s,
        P::DeleteChat(DeleteChat {
            message_signature: PackedSignature::Full(Box::new(sig)),
        }),
    );
    let next = s.secure.chat("x", now_millis());
    assert_eq!(next.last_seen.offset, 1);
    assert!(!next.last_seen.acknowledged.get(19));
}

#[test]
fn own_messages_on_the_local_server_are_secure() {
    let config = ClientConfig {
        local_server: true,
        ..ClientConfig::default()
    };
    let mut s = ClientState::new(&config, PROFILE);
    s.secure.on_login(false, false, now_millis());
    add_player(&mut s, PROFILE, "Me", false);
    let mut p = chat(0, 0, "mine", None);
    p.sender = PROFILE;
    let out = run(&mut s, P::PlayerChat(p));
    assert_eq!(shown(&out)[0].1, None);
}

#[test]
fn start_configuration_acknowledges_first() {
    let mut s = online_state(online_config(Some(valid_keys())));
    run(&mut s, P::PlayerChat(signed(0, 0, "x")));
    let out = run(&mut s, P::StartConfiguration(StartConfiguration));
    assert!(matches!(out.replies[0], S::ChatAck(sb_ack) if sb_ack.offset == 1));
    assert!(matches!(out.replies[1], S::ConfigurationAcknowledged(_)));
}

#[test]
fn system_and_disguised_lines_carry_the_system_tag() {
    let mut s = offline_state();
    let out = run(
        &mut s,
        P::SystemChat(mcr_protocol::play::clientbound::player::SystemChat {
            content: Component::text("joined"),
            overlay: false,
        }),
    );
    assert_eq!(shown(&out)[0].1, Some(ChatTag::System));
}
