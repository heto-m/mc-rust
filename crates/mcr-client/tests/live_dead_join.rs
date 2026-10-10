//! A player saved dead far from spawn gets no chunks until respawning; the client must neither
//! stall waiting for them nor fall through the empty world and provoke teleports.

mod common;

use common::Live;

#[test]
#[ignore = "needs a running vanilla server"]
fn live_join_while_dead() {
    let mut first = Live::connect("Ghost");
    first.console("op Ghost");
    first.until("loaded", 400, |l| l.client.state.player.loaded);
    first.console("difficulty peaceful");
    first.console("gamemode survival Ghost");
    first.idle(20);
    let before = first.teleports();
    first.console("tp Ghost 4000 -60 4000");
    first.until("teleported", 100, |l| l.teleports() > before);
    first.idle(40);
    first.console("kill Ghost");
    first.until("dead", 100, |l| l.client.state.player.dead);
    first.client.disconnect();
    std::thread::sleep(std::time::Duration::from_millis(800));

    let mut live = Live::connect("Ghost");
    live.idle(100);
    let p = &live.client.state.player;
    println!(
        "dead={} loaded={} teleports={} chunks={} pos={:?}",
        p.dead,
        p.loaded,
        p.teleports_received,
        live.client.state.chunks.len(),
        p.physics.pos
    );
    assert!(p.dead, "death state restored on join");
    assert!(
        p.loaded,
        "a dead player is let in without waiting for chunks"
    );
    let teleports = live.teleports();
    assert!(teleports <= 2, "teleport storm: {teleports}");
    live.idle(200);
    assert_eq!(live.teleports(), teleports, "teleports while dead");
    assert!(
        (live.client.state.player.physics.pos[1] + 60.0).abs() < 0.5,
        "fell through the unloaded world: {:?}",
        live.client.state.player.physics.pos
    );

    live.client.respawn().unwrap();
    live.until("respawned", 200, |l| !l.client.state.player.dead);
    live.until("chunks after respawn", 400, |l| {
        l.client.state.chunks.len() > 20
    });
    live.idle(40);
    assert!(
        live.teleports() <= teleports + 3,
        "teleport storm after respawn"
    );
    assert!(!live.log_text().contains("Ghost moved wrongly"));
}
