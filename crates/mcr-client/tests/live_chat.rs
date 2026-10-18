//! Secure chat against a real offline-mode vanilla server (unsigned path, real command tree):
//! `MCR_LIVE_ADDR=localhost:25573 MCR_LIVE_CONSOLE=<file the server's stdin tails> \
//!  MCR_LIVE_LOG=<server log> cargo test -p mcr-client --test live_chat -- --ignored --nocapture`

use mcr_client::ToastEvent;
use mcr_client::{ChatKind, ChatTag, ClientEvent};

mod common;
use common::*;

fn player_lines(live: &Live) -> Vec<(String, Option<ChatTag>)> {
    live.events
        .iter()
        .filter_map(|e| match e {
            ClientEvent::Chat {
                kind: ChatKind::Player,
                message,
                tag,
                ..
            } => Some((message.to_plain_string(), tag.clone())),
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "needs a running vanilla server"]
fn chat_and_commands_on_an_offline_server() {
    let mut a = Live::connect("ChatA");
    let mut b = Live::connect("ChatB");
    a.console("op ChatA");
    a.until("an operator's commands", 400, |l| {
        !l.client
            .state
            .commands
            .signable_arguments("say x")
            .is_empty()
    });
    b.until("joined", 400, |l| l.client.state.player.loaded);

    let tree = &a.client.state.commands;
    let pair = |n: &str, v: &str| vec![(n.to_owned(), v.to_owned())];
    assert_eq!(
        tree.signable_arguments("msg ChatB hello there"),
        pair("message", "hello there")
    );
    assert_eq!(
        tree.signable_arguments("say hi all"),
        pair("message", "hi all")
    );
    assert_eq!(tree.signable_arguments("me waves"), pair("action", "waves"));
    assert_eq!(
        tree.signable_arguments("execute as @a[limit=1, sort=nearest] run say hi"),
        pair("message", "hi")
    );
    assert!(tree.signable_arguments("time query daytime").is_empty());
    assert!(tree.signable_arguments("tellraw @a \"x\"").is_empty());
    assert!(
        a.events
            .iter()
            .any(|e| matches!(e, ClientEvent::Toast(ToastEvent::InsecureServer)))
    );

    a.client.send_chat("hello from A").unwrap();
    a.client.send_command("msg ChatB a secret").unwrap();
    a.client.send_command("say broadcast").unwrap();
    a.client.send_command("me waves").unwrap();
    for _ in 0..60 {
        a.idle(1);
        b.idle(1);
    }
    let lines = player_lines(&b);
    println!("ChatB saw: {lines:?}");
    for needle in ["hello from A", "a secret", "broadcast", "waves"] {
        let line = lines
            .iter()
            .find(|(text, _)| text.contains(needle))
            .unwrap_or_else(|| panic!("ChatB never saw {needle:?}"));
        assert_eq!(line.1, Some(ChatTag::NotSecure), "{needle}");
    }
    assert!(
        !b.events
            .iter()
            .chain(&a.events)
            .any(|e| matches!(e, ClientEvent::Disconnected(_)))
    );
    let log = a.log_text();
    assert!(
        log.contains("<ChatA> hello from A"),
        "server log lacks the chat"
    );
    assert!(!log.contains("Failed to validate message acknowledgements"));
}
