//! Cross-checks secure chat against vanilla 26.3's own classes from the user's client jar.
//! `MCR_JVM_TESTS=1 cargo test -p mcr-client --test vanilla_chat -- --nocapture`

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use mcr_auth::Secret;
use mcr_auth::chat::{ChatVerifier, MessageLink, ServicesKeys, SignedBody};
use mcr_auth::keys::PlayerKeys;
use mcr_buf::to_bytes;
use mcr_client::secure_chat::{SecureChat, checksum, now_millis};
use mcr_client::{ClientConfig, OnlineAuth};
use mcr_protocol::play::serverbound::BoundedBytes;
use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey, LineEnding};
use rsa::pkcs1v15::{Signature, SigningKey};
use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey};
use rsa::signature::{DigestSigner, SignatureEncoding};
use rsa::{RsaPrivateKey, RsaPublicKey};
use sha1::{Digest, Sha1};
use uuid::Uuid;

const PROFILE: Uuid = Uuid::from_u128(0x069a_79f4_44e9_4726_a5be_fca9_0e38_aaf5);

fn cache() -> Option<PathBuf> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .map(|d| d.join(".cache"))
        .find(|d| d.join("versions/26.3/client.jar").exists())
}

fn java_tool(cache: &Path, tool: &str) -> PathBuf {
    let bundled = cache.join("tools/jdk25/bin").join(tool);
    if bundled.exists() {
        bundled
    } else {
        PathBuf::from(tool)
    }
}

fn class_path(cache: &Path) -> String {
    let mut jars = vec![cache.join("versions/26.3/client.jar")];
    let mut libs: Vec<PathBuf> = std::fs::read_dir(cache.join("libraries"))
        .expect("libraries")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jar"))
        .collect();
    libs.sort();
    jars.extend(libs);
    jars.iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(":")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn der(key: &RsaPrivateKey) -> Vec<u8> {
    RsaPublicKey::from(key)
        .to_public_key_der()
        .unwrap()
        .into_vec()
}

fn services_sign(services: &RsaPrivateKey, profile: Uuid, expires: i64, der: &[u8]) -> Vec<u8> {
    let mut h = Sha1::new();
    h.update(profile.as_bytes());
    h.update(expires.to_be_bytes());
    h.update(der);
    let s: Signature = SigningKey::<Sha1>::new(services.clone()).sign_digest(h);
    s.to_vec()
}

fn sig(seed: u32) -> [u8; 256] {
    let mut s = [0u8; 256];
    for (i, b) in s.iter_mut().enumerate() {
        *b = (seed
            .wrapping_mul(2_654_435_761)
            .wrapping_add(i as u32 * 7919)
            >> 8) as u8;
    }
    s
}

#[test]
fn vanilla_accepts_our_chat_and_we_accept_vanillas() {
    if std::env::var("MCR_JVM_TESTS").as_deref() != Ok("1") {
        eprintln!("skipped: set MCR_JVM_TESTS=1");
        return;
    }
    let cache = cache().expect("a .cache with versions/26.3/client.jar");
    let mut rng = rand::rngs::OsRng;
    let player = RsaPrivateKey::new(&mut rng, 2048).unwrap();
    let services = RsaPrivateKey::new(&mut rng, 2048).unwrap();
    let now = now_millis();
    let expires = now + 48 * 3_600_000;
    let keys = PlayerKeys {
        private_key_pem: Secret::new(player.to_pkcs1_pem(LineEnding::LF).unwrap().to_string()),
        public_key_pem: RsaPublicKey::from(&player)
            .to_pkcs1_pem(LineEnding::LF)
            .unwrap(),
        public_key_signature: STANDARD.encode([0u8; 4]),
        public_key_signature_v2: STANDARD.encode(services_sign(
            &services,
            PROFILE,
            expires,
            &der(&player),
        )),
        expires_at_ms: expires,
        refreshed_after_ms: expires - 3_600_000,
    };
    let mut auth = OnlineAuth::new(Secret::new("t"), PROFILE.simple().to_string());
    auth.keys = Some(keys);
    auth.services = Arc::new(ServicesKeys::new([RsaPublicKey::from(&services)]));
    let config = ClientConfig {
        online: Some(auth),
        ..ClientConfig::default()
    };
    let mut chat = SecureChat::new(&config, PROFILE);
    let update = chat.on_login(true, true, now).expect("session");

    let mut lines = vec![
        format!("services {}", hex(&der(&services))),
        format!("profile {PROFILE}"),
        format!("session {}", hex(&to_bytes(&update))),
    ];
    let mut bad = update.clone();
    bad.key_signature = BoundedBytes(services_sign(
        &services,
        Uuid::from_u128(1),
        expires,
        &der(&player),
    ));
    lines.push(format!("bad_session {}", hex(&to_bytes(&bad))));
    let mut expected_checks = 2;

    let mut seed = 0;
    let mut recv = |chat: &mut SecureChat, lines: &mut Vec<String>, shown: bool| -> usize {
        seed += 1;
        let s = sig(seed);
        lines.push(format!("recv {}", hex(&s)));
        match chat.mark_processed(&s, shown) {
            Some(ack) => {
                lines.push(format!("ack {}", hex(&to_bytes(&ack))));
                1
            }
            None => 0,
        }
    };
    for _ in 0..3 {
        expected_checks += recv(&mut chat, &mut lines, true);
    }
    let one = chat.chat("one", now + 1);
    lines.push(format!("chat {} {}", hex(&to_bytes(&one)), hex(b"one")));
    expected_checks += 1;
    expected_checks += recv(&mut chat, &mut lines, false);
    expected_checks += recv(&mut chat, &mut lines, true);
    let args = vec![("message".to_owned(), "hi there ü".to_owned())];
    let command = chat.signed_command("msg Alex hi there ü", &args, now + 2);
    lines.push(format!(
        "command {} message {}",
        hex(&to_bytes(&command)),
        hex(args[0].1.as_bytes())
    ));
    expected_checks += 1;
    for i in 0..140 {
        expected_checks += recv(&mut chat, &mut lines, i % 7 != 3);
    }
    let two = chat.chat("two", now + 3);
    lines.push(format!("chat {} {}", hex(&to_bytes(&two)), hex(b"two")));
    expected_checks += 1;
    let three = chat.chat("three", now + 4);
    lines.push(format!("forged {}", hex(&to_bytes(&three))));
    expected_checks += 1;

    for set in [vec![], vec![sig(900)], vec![sig(901), sig(902), sig(903)]] {
        let mut line = format!("checksum {}", checksum(&set));
        for s in &set {
            line.push(' ');
            line.push_str(&hex(s));
        }
        lines.push(line);
        expected_checks += 1;
    }

    let vanilla_sender = Uuid::from_u128(77);
    let vanilla_session = Uuid::from_u128(78);
    let seen = [sig(1000), sig(1001)];
    lines.push(format!(
        "signing_key {}",
        hex(player.to_pkcs8_der().unwrap().as_bytes())
    ));
    lines.push(format!(
        "vanilla_sign {vanilla_sender} {vanilla_session} -9 {} {} {} {}",
        now,
        hex("héllo ".as_bytes()),
        hex(&seen[0]),
        hex(&seen[1])
    ));

    let work = std::env::temp_dir().join(format!("mcr-vanilla-chat-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let input = work.join("in.txt");
    let output = work.join("out.txt");
    std::fs::write(&input, lines.join("\n")).unwrap();
    let cp = class_path(&cache);
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/vanilla_chat/ChatCrossCheck.java");
    let status = Command::new(java_tool(&cache, "javac"))
        .args(["-nowarn", "-cp", &cp, "-d"])
        .arg(&work)
        .arg(&source)
        .status()
        .expect("javac");
    assert!(status.success(), "javac failed");
    let run = Command::new(java_tool(&cache, "java"))
        .args(["-Xmx256m", "-XX:+UseSerialGC", "-cp"])
        .arg(format!("{}:{cp}", work.display()))
        .arg("ChatCrossCheck")
        .arg(&input)
        .arg(&output)
        .output()
        .expect("java");
    assert!(
        run.status.success(),
        "vanilla rejected our chat:\n{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let result = std::fs::read_to_string(&output).unwrap();
    let verifier = ChatVerifier::new(RsaPublicKey::from(&player));
    let mut verified = 0;
    for line in result.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "checks" => assert_eq!(f[1].parse::<usize>().unwrap(), expected_checks),
            "signed" => {
                let index: i32 = f[1].parse().unwrap();
                let text = String::from_utf8(unhex(f[2])).unwrap();
                let body = SignedBody {
                    content: &text,
                    timestamp_ms: now,
                    salt: -9,
                    last_seen: &seen,
                };
                let link = MessageLink {
                    sender: vanilla_sender,
                    session: vanilla_session,
                    index,
                };
                assert!(verifier.verify(&link, &body, &unhex(f[3])), "{line}");
                let wrong = MessageLink {
                    index: index + 1,
                    ..link
                };
                assert!(!verifier.verify(&wrong, &body, &unhex(f[3])));
                verified += 1;
            }
            _ => panic!("unexpected output {line}"),
        }
    }
    assert_eq!(verified, 2);
    println!(
        "vanilla cross-check: {expected_checks} server-side checks passed, 2 vanilla signatures verified"
    );
    let _ = std::fs::remove_dir_all(&work);
}
