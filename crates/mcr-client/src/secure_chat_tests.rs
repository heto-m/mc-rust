use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mcr_auth::Secret;
use mcr_auth::chat::{ChatVerifier, MessageLink, ServicesKeys, SignedBody};
use mcr_auth::keys::{KeyRefresh, PlayerKeys};
use mcr_buf::{LengthPrefixed, to_bytes};
use mcr_protocol::play::clientbound::player::{PackedSignature, RemoteChatSession};
use mcr_protocol::types::Component;
use mcr_protocol::types::text::{Arg, Style};
use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey, LineEnding};
use rsa::pkcs1v15::{Signature, SigningKey};
use rsa::pkcs8::EncodePublicKey;
use rsa::signature::{DigestSigner, SignatureEncoding};
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha1::{Digest, Sha1};
use uuid::Uuid;

use super::*;
use crate::config::{ClientConfig, OnlineAuth};

pub const NOW_MS: i64 = 1_800_000_000_000;
pub const PROFILE: Uuid = Uuid::from_u128(0x069a_79f4_44e9_4726_a5be_fca9_0e38_aaf5);

fn generated(slot: &'static OnceLock<RsaPrivateKey>) -> &'static RsaPrivateKey {
    slot.get_or_init(|| RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).unwrap())
}

/// Test keys generated per run; never real ones.
pub fn player_key() -> &'static RsaPrivateKey {
    static KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
    generated(&KEY)
}

pub fn other_player_key() -> &'static RsaPrivateKey {
    static KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
    generated(&KEY)
}

pub fn services_key() -> &'static RsaPrivateKey {
    static KEY: OnceLock<RsaPrivateKey> = OnceLock::new();
    generated(&KEY)
}

pub fn der(key: &RsaPrivateKey) -> Vec<u8> {
    RsaPublicKey::from(key)
        .to_public_key_der()
        .unwrap()
        .into_vec()
}

pub fn services() -> Arc<ServicesKeys> {
    Arc::new(ServicesKeys::new([RsaPublicKey::from(services_key())]))
}

pub fn key_signature(profile: Uuid, expires_ms: i64, der: &[u8]) -> Vec<u8> {
    let mut hasher = Sha1::new();
    hasher.update(profile.as_bytes());
    hasher.update(expires_ms.to_be_bytes());
    hasher.update(der);
    let signature: Signature = SigningKey::<Sha1>::new(services_key().clone()).sign_digest(hasher);
    signature.to_vec()
}

pub fn keys_for(key: &RsaPrivateKey, expires_ms: i64, refreshed_ms: i64) -> PlayerKeys {
    let public = RsaPublicKey::from(key);
    PlayerKeys {
        private_key_pem: Secret::new(key.to_pkcs1_pem(LineEnding::LF).unwrap().to_string()),
        public_key_pem: public.to_pkcs1_pem(LineEnding::LF).unwrap(),
        public_key_signature: STANDARD.encode([9u8; 4]),
        public_key_signature_v2: STANDARD.encode(key_signature(PROFILE, expires_ms, &der(key))),
        expires_at_ms: expires_ms,
        refreshed_after_ms: refreshed_ms,
    }
}

pub fn online_config(keys: Option<PlayerKeys>) -> ClientConfig {
    let mut auth = OnlineAuth::new(Secret::new("token"), PROFILE.simple().to_string());
    auth.keys = keys;
    auth.services = services();
    ClientConfig {
        online: Some(auth),
        ..ClientConfig::default()
    }
}

pub fn valid_keys() -> PlayerKeys {
    keys_for(player_key(), NOW_MS + 3_600_000, NOW_MS + 1_800_000)
}

pub fn remote_session(
    player: Uuid,
    key: &RsaPrivateKey,
    session: Uuid,
    expires_ms: i64,
) -> RemoteChatSession {
    let der = der(key);
    RemoteChatSession {
        session_id: session,
        expires_at_millis: expires_ms,
        key_signature: LengthPrefixed(key_signature(player, expires_ms, &der)),
        public_key: LengthPrefixed(der),
    }
}

pub fn sign_as(
    key: &RsaPrivateKey,
    sender: Uuid,
    session: Uuid,
    index: i32,
    body: &SignedBody<'_>,
) -> [u8; 256] {
    mcr_auth::chat::ChatSigner::new(key.clone())
        .unwrap()
        .sign(
            &MessageLink {
                sender,
                session,
                index,
            },
            body,
        )
        .unwrap()
}

fn sig(fill: u8) -> [u8; 256] {
    [fill; 256]
}

#[test]
fn checksum_matches_java() {
    assert_eq!(checksum(&[]), 1);
    assert_eq!(checksum(&[sig(1), sig(2), sig(0xab)]), 64);
    assert_eq!(checksum(&[sig(1)]), 32);
    let ramp: [u8; 256] = std::array::from_fn(|i| i as u8);
    assert_eq!(checksum(&[ramp]), 160);
}

#[test]
fn tracker_update_bytes() {
    let mut t = LastSeenTracker::default();
    for fill in [1, 2, 0xab] {
        assert!(t.add_pending(&sig(fill), true));
    }
    let u = t.generate_update();
    assert_eq!(u.last_seen, [sig(1), sig(2), sig(0xab)]);
    assert_eq!(to_bytes(&u.update), [3, 0x00, 0x00, 0x0e, 64]);

    let again = t.generate_update();
    assert_eq!(to_bytes(&again.update), [0, 0x00, 0x00, 0x0e, 64]);
}

#[test]
fn tracker_repeats_hidden_and_ignored_entries() {
    let mut t = LastSeenTracker::default();
    assert!(t.add_pending(&sig(1), true));
    assert!(!t.add_pending(&sig(1), true));
    assert!(t.add_pending(&sig(2), false));
    assert!(t.add_pending(&sig(3), true));
    t.ignore_pending(&sig(3));
    let u = t.generate_update();
    assert_eq!(u.update.offset, 3);
    assert_eq!(u.last_seen, [sig(1)]);
    assert!(u.update.acknowledged.get(17));
    assert!(!u.update.acknowledged.get(18) && !u.update.acknowledged.get(19));

    t.add_pending(&sig(4), true);
    t.generate_update();
    t.ignore_pending(&sig(4));
    assert_eq!(t.generate_update().last_seen, [sig(1), sig(4)]);
}

#[test]
fn tracker_window_keeps_the_last_twenty() {
    let mut t = LastSeenTracker::default();
    for i in 0..25u8 {
        t.add_pending(&sig(i), true);
    }
    let u = t.generate_update();
    assert_eq!(u.update.offset, 25);
    assert_eq!(u.last_seen.len(), LAST_SEEN);
    assert_eq!(u.last_seen[0], sig(5));
    assert_eq!(u.last_seen[19], sig(24));
    assert_eq!(to_bytes(&u.update)[1..4], [0xff, 0xff, 0x0f]);
}

#[test]
fn ack_after_sixty_four_unacknowledged() {
    let mut chat = SecureChat::new(&ClientConfig::default(), PROFILE);
    for i in 0..64u32 {
        let mut s = [0u8; 256];
        s[..4].copy_from_slice(&i.to_be_bytes());
        assert_eq!(chat.mark_processed(&s, true), None);
    }
    assert_eq!(
        chat.mark_processed(&sig(0xff), false),
        Some(sb::ChatAck { offset: 65 })
    );
    assert_eq!(chat.take_ack(), None);
    chat.mark_processed(&sig(0xfe), true);
    assert_eq!(chat.take_ack(), Some(sb::ChatAck { offset: 1 }));
}

#[test]
fn cache_push_order_and_hostile_ids() {
    let mut cache = SignatureCache::default();
    cache.push(&[sig(1), sig(2)], Some(&sig(3)));
    assert_eq!(cache.get(0), Some(&sig(3)));
    assert_eq!(cache.get(1), Some(&sig(2)));
    assert_eq!(cache.get(2), Some(&sig(1)));
    cache.push(&[sig(3)], Some(&sig(4)));
    let order: Vec<u8> = (0..5).filter_map(|i| cache.get(i).map(|s| s[0])).collect();
    assert_eq!(order, [4, 3, 2, 1]);
    assert_eq!(cache.get(500), None);
    assert_eq!(cache.get(-1), None);
    assert_eq!(cache.get(127), None);
    assert_eq!(cache.unpack(&PackedSignature::Cached(1)), Some(sig(3)));
    assert_eq!(cache.unpack(&PackedSignature::Cached(128)), None);

    for i in 0..300u32 {
        let mut s = [0u8; 256];
        s[..4].copy_from_slice(&i.to_be_bytes());
        cache.push(&[], Some(&s));
    }
    assert!(cache.get(127).is_some());
    assert_eq!(cache.get(0).unwrap()[..4], 299u32.to_be_bytes());
}

#[test]
fn session_starts_only_on_online_servers_with_valid_keys() {
    let mut chat = SecureChat::new(&online_config(Some(valid_keys())), PROFILE);
    assert!(chat.on_login(false, true, NOW_MS).is_none());
    assert!(!chat.has_session());
    let unsigned = chat.chat("hi", NOW_MS);
    assert!(unsigned.signature.is_none());
    assert_eq!(unsigned.last_seen.checksum, 1);

    let update = chat.on_login(true, true, NOW_MS).unwrap();
    assert_eq!(update.public_key.0, der(player_key()));
    assert_eq!(
        update.key_signature.0,
        key_signature(PROFILE, NOW_MS + 3_600_000, &der(player_key()))
    );
    assert_eq!(update.expires_at_ms, NOW_MS + 3_600_000);
    assert_eq!(update.session_id.get_version_num(), 4);
    assert_eq!(chat.session_id(), Some(update.session_id));

    let again = chat.on_login(true, true, NOW_MS).unwrap();
    assert_ne!(again.session_id, update.session_id);

    let expired = keys_for(player_key(), NOW_MS - 1, NOW_MS - 2);
    let mut stale = SecureChat::new(&online_config(Some(expired)), PROFILE);
    assert!(stale.on_login(true, true, NOW_MS).is_none());
    let mut none = SecureChat::new(&online_config(None), PROFILE);
    assert!(none.on_login(true, true, NOW_MS).is_none());
    let mut offline = SecureChat::new(&ClientConfig::default(), PROFILE);
    assert!(offline.on_login(true, false, NOW_MS).is_none());
}

#[test]
fn small_keys_never_start_a_session() {
    let small = RsaPrivateKey::new(&mut rand::rngs::OsRng, 1024).unwrap();
    let keys = keys_for(&small, NOW_MS + 3_600_000, NOW_MS + 1_800_000);
    let mut chat = SecureChat::new(&online_config(Some(keys)), PROFILE);
    assert!(chat.on_login(true, true, NOW_MS).is_none());
}

#[test]
fn chat_chain_signs_in_order_and_resets_on_login() {
    let mut chat = SecureChat::new(&online_config(Some(valid_keys())), PROFILE);
    let session = chat.on_login(true, true, NOW_MS).unwrap().session_id;
    let verifier = ChatVerifier::new(RsaPublicKey::from(player_key()));
    for (index, text) in ["first", "second"].iter().enumerate() {
        let packet = chat.chat(text, NOW_MS + index as i64);
        let body = SignedBody {
            content: text,
            timestamp_ms: packet.timestamp_ms,
            salt: packet.salt,
            last_seen: &[],
        };
        let link = MessageLink {
            sender: PROFILE,
            session,
            index: index as i32,
        };
        assert!(verifier.verify(&link, &body, &packet.signature.unwrap().0));
    }
    let session2 = chat.on_login(true, true, NOW_MS).unwrap().session_id;
    let packet = chat.chat("third", NOW_MS);
    let body = SignedBody {
        content: "third",
        timestamp_ms: packet.timestamp_ms,
        salt: packet.salt,
        last_seen: &[],
    };
    let link = MessageLink {
        sender: PROFILE,
        session: session2,
        index: 0,
    };
    assert!(verifier.verify(&link, &body, &packet.signature.unwrap().0));
}

#[test]
fn chat_signs_over_acknowledged_messages() {
    let mut chat = SecureChat::new(&online_config(Some(valid_keys())), PROFILE);
    let session = chat.on_login(true, true, NOW_MS).unwrap().session_id;
    chat.mark_processed(&sig(7), true);
    chat.mark_processed(&sig(8), true);
    let packet = chat.chat("hey", NOW_MS);
    assert_eq!(packet.last_seen.offset, 2);
    assert_eq!(packet.last_seen.checksum, checksum(&[sig(7), sig(8)]));
    let body = SignedBody {
        content: "hey",
        timestamp_ms: NOW_MS,
        salt: packet.salt,
        last_seen: &[sig(7), sig(8)],
    };
    let link = MessageLink {
        sender: PROFILE,
        session,
        index: 0,
    };
    let verifier = ChatVerifier::new(RsaPublicKey::from(player_key()));
    assert!(verifier.verify(&link, &body, &packet.signature.unwrap().0));
}

#[test]
fn signed_command_shares_salt_and_advances_per_argument() {
    let mut chat = SecureChat::new(&online_config(Some(valid_keys())), PROFILE);
    let session = chat.on_login(true, true, NOW_MS).unwrap().session_id;
    let args = vec![
        ("message".to_owned(), "hello there".to_owned()),
        ("other".to_owned(), "x".to_owned()),
    ];
    let packet = chat.signed_command("msg Alex hello there", &args, NOW_MS);
    let verifier = ChatVerifier::new(RsaPublicKey::from(player_key()));
    for (index, (arg, value)) in packet.argument_signatures.0.iter().zip(&args).enumerate() {
        assert_eq!(arg.name, value.0);
        let body = SignedBody {
            content: &value.1,
            timestamp_ms: NOW_MS,
            salt: packet.salt,
            last_seen: &[],
        };
        let link = MessageLink {
            sender: PROFILE,
            session,
            index: index as i32,
        };
        assert!(verifier.verify(&link, &body, &arg.signature.0));
    }

    let mut unsigned = SecureChat::new(&ClientConfig::default(), PROFILE);
    unsigned.on_login(false, false, NOW_MS);
    let packet = unsigned.signed_command("msg Alex hi", &args, NOW_MS);
    assert!(packet.argument_signatures.0.is_empty());
}

#[test]
fn key_refresh_starts_a_new_session() {
    let fresh = keys_for(player_key(), NOW_MS + 7_200_000, NOW_MS + 5_000_000);
    let returned = fresh.clone();
    let mut config = online_config(Some(keys_for(player_key(), NOW_MS + 3_600_000, NOW_MS - 1)));
    if let Some(o) = config.online.as_mut() {
        o.refresh = Some(KeyRefresh::new(move || Some(returned.clone())));
    }
    let mut chat = SecureChat::new(&config, PROFILE);
    let first = chat.on_login(true, true, NOW_MS).unwrap().session_id;
    let start = Instant::now();
    assert!(chat.poll_refresh(start, NOW_MS).is_none());
    let mut update = None;
    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(10));
        if let Some(u) = chat.poll_refresh(start, NOW_MS) {
            update = Some(u);
            break;
        }
    }
    let update = update.expect("refresh delivered a session");
    assert_ne!(update.session_id, first);
    assert_eq!(update.expires_at_ms, NOW_MS + 7_200_000);
    assert!(chat.poll_refresh(start, NOW_MS).is_none());
    assert!(chat.set_keys(&fresh, NOW_MS).is_none());
}

#[test]
fn refresh_waits_between_attempts() {
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = calls.clone();
    let mut config = online_config(Some(keys_for(player_key(), NOW_MS + 3_600_000, NOW_MS - 1)));
    if let Some(o) = config.online.as_mut() {
        o.refresh = Some(KeyRefresh::new(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            None
        }));
    }
    let mut chat = SecureChat::new(&config, PROFILE);
    chat.on_login(true, true, NOW_MS);
    let start = Instant::now();
    for _ in 0..100 {
        chat.poll_refresh(start, NOW_MS);
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

fn remote_chat() -> (SecureChat, Uuid, Uuid) {
    let mut chat = SecureChat::new(&online_config(Some(valid_keys())), PROFILE);
    chat.on_login(true, true, NOW_MS);
    let alex = Uuid::from_u128(42);
    let session = Uuid::from_u128(4242);
    chat.init_session(
        alex,
        Some(&remote_session(
            alex,
            other_player_key(),
            session,
            NOW_MS + 3_600_000,
        )),
    );
    (chat, alex, session)
}

fn body(content: &str) -> SignedBody<'_> {
    SignedBody {
        content,
        timestamp_ms: NOW_MS,
        salt: 5,
        last_seen: &[],
    }
}

#[test]
fn remote_sessions_need_a_services_signature() {
    let (mut chat, alex, session) = remote_chat();
    assert!(chat.has_remote_session(alex));
    let mut forged = remote_session(alex, other_player_key(), session, NOW_MS + 3_600_000);
    forged.expires_at_millis += 1;
    chat.init_session(alex, Some(&forged));
    assert!(!chat.has_remote_session(alex));

    let steve = Uuid::from_u128(43);
    chat.init_session(
        steve,
        Some(&remote_session(alex, other_player_key(), session, NOW_MS)),
    );
    assert!(!chat.has_remote_session(steve));

    let mut hostile = remote_session(alex, other_player_key(), session, NOW_MS);
    hostile.public_key = LengthPrefixed(vec![0x30; 400]);
    chat.init_session(alex, Some(&hostile));
    assert!(!chat.has_remote_session(alex));

    let mut no_services = SecureChat::new(&ClientConfig::default(), PROFILE);
    no_services.init_session(
        alex,
        Some(&remote_session(
            alex,
            other_player_key(),
            session,
            NOW_MS + 3_600_000,
        )),
    );
    assert!(!no_services.has_remote_session(alex));

    let (mut chat, alex, _) = remote_chat();
    chat.remove_players(&[alex]);
    assert!(!chat.has_remote_session(alex));
}

#[test]
fn valid_messages_pass_and_a_forgery_breaks_the_chain() {
    let (mut chat, alex, session) = remote_chat();
    let key = other_player_key();
    let s0 = sign_as(key, alex, session, 0, &body("a"));
    assert_eq!(
        chat.validate(alex, 0, Some(&s0), &body("a"), NOW_MS),
        Validated::Accepted { signed: true }
    );
    assert_eq!(
        chat.validate(alex, 0, Some(&s0), &body("a"), NOW_MS),
        Validated::Accepted { signed: true },
        "a repeat of the last message stays valid"
    );
    let s2 = sign_as(key, alex, session, 2, &body("c"));
    assert_eq!(
        chat.validate(alex, 2, Some(&s2), &body("c"), NOW_MS),
        Validated::Accepted { signed: true }
    );
    assert_eq!(
        chat.validate(alex, 3, Some(&s2), &body("d"), NOW_MS),
        Validated::Rejected
    );
    let s4 = sign_as(key, alex, session, 4, &body("e"));
    assert_eq!(
        chat.validate(alex, 4, Some(&s4), &body("e"), NOW_MS),
        Validated::Rejected,
        "the chain stays broken"
    );
}

#[test]
fn out_of_order_unsigned_and_expired_messages_are_rejected() {
    let (mut chat, alex, session) = remote_chat();
    let key = other_player_key();
    let s5 = sign_as(key, alex, session, 5, &body("x"));
    assert_eq!(
        chat.validate(alex, 5, Some(&s5), &body("x"), NOW_MS),
        Validated::Accepted { signed: true }
    );
    let s4 = sign_as(key, alex, session, 4, &body("y"));
    assert_eq!(
        chat.validate(alex, 4, Some(&s4), &body("y"), NOW_MS),
        Validated::Rejected
    );

    let (mut chat, alex, session) = remote_chat();
    assert_eq!(
        chat.validate(alex, 0, None, &body("x"), NOW_MS),
        Validated::Rejected
    );

    let (mut chat, alex, session2) = remote_chat();
    let s = sign_as(key, alex, session2, 0, &body("late"));
    let after_grace = NOW_MS + 3_600_000 + 8 * 3_600_000 + 1;
    assert_eq!(
        chat.validate(alex, 0, Some(&s), &body("late"), after_grace),
        Validated::Rejected
    );
    let _ = session;
}

#[test]
fn senders_without_a_session_follow_enforcement() {
    let mut enforcing = SecureChat::new(&online_config(None), PROFILE);
    enforcing.on_login(true, true, NOW_MS);
    assert!(enforcing.enforces_secure_chat());
    let nobody = Uuid::from_u128(9);
    assert_eq!(
        enforcing.validate(nobody, 0, Some(&sig(1)), &body("x"), NOW_MS),
        Validated::Rejected
    );
    let mut lax = SecureChat::new(&online_config(None), PROFILE);
    lax.on_login(true, false, NOW_MS);
    assert_eq!(
        lax.validate(nobody, 0, Some(&sig(1)), &body("x"), NOW_MS),
        Validated::Accepted { signed: false }
    );
    let mut offline = SecureChat::new(&ClientConfig::default(), PROFILE);
    offline.on_login(false, true, NOW_MS);
    assert!(!offline.enforces_secure_chat());
    assert_eq!(
        offline.validate(nobody, 0, None, &body("x"), NOW_MS),
        Validated::Accepted { signed: false }
    );
}

#[test]
fn insecure_warning_once_per_connection_and_never_locally() {
    let mut chat = SecureChat::new(&ClientConfig::default(), PROFILE);
    chat.on_login(false, false, NOW_MS);
    assert!(chat.should_warn_insecure());
    chat.on_login(false, false, NOW_MS);
    assert!(!chat.should_warn_insecure());
    let mut local = SecureChat::new(
        &ClientConfig {
            local_server: true,
            ..ClientConfig::default()
        },
        PROFILE,
    );
    local.on_login(false, false, NOW_MS);
    assert!(!local.should_warn_insecure());
    let mut enforcing = SecureChat::new(&online_config(None), PROFILE);
    enforcing.on_login(true, true, NOW_MS);
    assert!(!enforcing.should_warn_insecure());
}

fn decorated(sender: &str, content: Component) -> Component {
    Component::translatable(
        "chat.type.text",
        vec![
            Arg::Component(Component::text(sender)),
            Arg::Component(content),
        ],
    )
}

#[test]
fn trust_levels() {
    let plain = decorated("Alex", Component::text("hi there"));
    assert_eq!(
        evaluate_trust(true, NOW_MS, NOW_MS, "hi there", &plain, None),
        TrustLevel::Secure
    );
    assert_eq!(
        evaluate_trust(false, NOW_MS, NOW_MS, "hi there", &plain, None),
        TrustLevel::NotSecure
    );
    assert_eq!(
        evaluate_trust(
            true,
            NOW_MS - 7 * 60_000 - 1,
            NOW_MS,
            "hi there",
            &plain,
            None
        ),
        TrustLevel::NotSecure
    );
    let changed = Component::text("hello");
    let shown = decorated("Alex", changed.clone());
    assert_eq!(
        evaluate_trust(true, NOW_MS, NOW_MS, "hi", &shown, Some(&changed)),
        TrustLevel::Modified
    );
    let fancy = Component::text("hi").styled(Style {
        font: Some(mcr_buf::Identifier::parse("minecraft:alt").unwrap()),
        ..Style::default()
    });
    let shown = decorated("Alex", fancy.clone());
    assert_eq!(
        evaluate_trust(true, NOW_MS, NOW_MS, "hi", &shown, Some(&fancy)),
        TrustLevel::Modified
    );
    let default_font = Component::text("hi").styled(Style {
        font: Some(mcr_buf::Identifier::parse("default").unwrap()),
        ..Style::default()
    });
    let shown = decorated("Alex", default_font.clone());
    assert_eq!(
        evaluate_trust(true, NOW_MS, NOW_MS, "hi", &shown, Some(&default_font)),
        TrustLevel::Secure
    );
}
