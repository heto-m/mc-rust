//! `MCR_LIVE_ADDR=... MCR_LIVE_CONSOLE=... MCR_LIVE_LOG=... cargo test -p mcr-client --test live_edits -- --ignored --nocapture`

mod common;

use common::Live;
use mcr_buf::BlockPos;
use mcr_client::{ClientEvent, UseResult, UseTarget};
use mcr_protocol::play::serverbound::Facing;

#[test]
#[ignore = "needs a running vanilla server"]
fn live_sign_book_and_state() {
    let mut live = Live::connect("Editor");
    live.console("op Editor");
    live.until("loaded", 400, |l| l.client.state.player.loaded);
    live.console("gamemode survival Editor");
    live.console("clear Editor");
    live.console("recipe give Editor *");
    live.console("give Editor minecraft:writable_book");
    live.idle(40);

    assert!(live.client.state.gameplay.advancements.defs.len() > 100);
    assert!(!live.client.state.gameplay.recipe_book.entries.is_empty());

    live.client.request_stats().unwrap();
    live.until("stats", 60, |l| {
        l.events
            .iter()
            .any(|e| matches!(e, ClientEvent::StatsUpdated))
    });

    let p = live.pos();
    let sign = BlockPos::new(
        p[0].floor() as i32 + 1,
        p[1].floor() as i32,
        p[2].floor() as i32,
    );
    live.console(&format!(
        "setblock {} {} {} minecraft:oak_sign",
        sign.x, sign.y, sign.z
    ));
    live.until("sign", 60, |l| l.block(sign) != 0);
    let r = live
        .client
        .start_use(UseTarget::Block {
            pos: sign,
            face: Facing::West,
            cursor: [0.0, 0.5, 0.5],
            inside: false,
        })
        .unwrap();
    assert_eq!(r, UseResult::Success);
    live.until("sign editor", 60, |l| {
        l.client.state.gameplay.sign_editor.is_some()
    });
    live.client
        .submit_sign(["alpha", "beta", "", "delta"])
        .unwrap();
    live.idle(10);
    assert!(live.server_says(
        &format!(
            "data get block {} {} {} front_text.messages[0]",
            sign.x, sign.y, sign.z
        ),
        "alpha"
    ));

    live.client.select_hotbar(0).unwrap();
    live.client.start_use(UseTarget::Air).unwrap();
    live.until("book editor", 60, |l| {
        l.client.state.gameplay.book_editor.is_some()
    });
    live.client
        .submit_book(&["page one".to_owned(), "page two".to_owned()], None)
        .unwrap();
    live.idle(10);
    assert!(live.server_says("data get entity Editor Inventory[0]", "page one"));
}
