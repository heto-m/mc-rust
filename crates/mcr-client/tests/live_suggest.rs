//! Local suggestion providers on the real command tree, and chat with a delay:
//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_suggest -- --ignored --nocapture`

use mcr_client::commands::SuggestSource;
use mcr_client::{ChatKind, ClientEvent};

mod common;
use common::*;

struct Data(&'static mcr_data::Version);

impl SuggestSource for Data {
    fn players(&self) -> Vec<String> {
        Vec::new()
    }

    fn registry(&self, id: &str) -> Vec<String> {
        let Some(r) = self.0.registry(id) else {
            return Vec::new();
        };
        (0..r.len() as u32)
            .filter_map(|i| r.name_of(i))
            .map(str::to_owned)
            .collect()
    }
}

#[test]
#[ignore = "needs a running vanilla server"]
fn live_local_suggestions_and_delayed_chat() {
    let mut a = Live::connect("SuggestA");
    let mut b = Live::connect("SuggestB");
    a.console("op SuggestA");
    a.until("an operator's commands", 400, |l| {
        !l.client
            .state
            .commands
            .signable_arguments("say x")
            .is_empty()
    });
    let source = Data(a.client.state.data);
    let tree = &a.client.state.commands;

    let summon = tree.complete("/summon minecraft:zo", 20, &source);
    println!("summon: {:?}", summon.local);
    assert!(!summon.ask_server);
    assert!(summon.local.contains(&"minecraft:zombie".to_owned()));
    assert!(summon.local.iter().all(|s| s.starts_with("minecraft:zo")));
    let players = tree.complete("/summon minecraft:pla", 21, &source);
    assert!(players.local.is_empty(), "{:?}", players.local);

    let sound = tree.complete("/playsound entity.zombie.am", 27, &source);
    println!("playsound: {:?}", sound.local);
    assert!(!sound.ask_server);
    assert_eq!(sound.local, ["minecraft:entity.zombie.ambient"]);

    b.client.set_chat_delay(true).unwrap();
    a.client.send_chat("delayed hello").unwrap();
    for _ in 0..40 {
        a.idle(1);
        b.idle(1);
    }
    let seen = b.events.iter().any(|e| {
        matches!(e, ClientEvent::Chat { kind: ChatKind::Player, message, .. }
            if message.to_plain_string().contains("delayed hello"))
    });
    assert!(seen, "the message reached the chat queue");
    b.client.chat_queue_drained().unwrap();
    b.client.set_chat_delay(false).unwrap();
    b.client.send_chat("reply").unwrap();
    for _ in 0..20 {
        a.idle(1);
        b.idle(1);
    }
    let log = a.log_text();
    assert!(
        log.contains("<SuggestB> reply"),
        "the server accepted the reply"
    );
    assert!(!log.contains("SuggestB lost connection"));
}
