//! Replays frames captured from a vanilla 26.3 server through the dispatch enums and checks that
//! every one decodes completely and re-encodes to the same bytes.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use mcr_protocol::dispatch::{
    ConfigClientbound, ConfigServerbound, HandshakeServerbound, LoginClientbound, LoginServerbound,
    PlayClientbound, PlayServerbound, StatusClientbound, StatusServerbound,
};
use mcr_protocol::{Direction, PacketIds, ProtocolState};

const CAPTURE_SETS: [&str; 2] = ["26.3", "26.3-no-known-packs"];

fn capture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/captures")
}

fn state_of(name: &str) -> ProtocolState {
    match name {
        "handshake" => ProtocolState::Handshake,
        "status" => ProtocolState::Status,
        "login" => ProtocolState::Login,
        "configuration" => ProtocolState::Configuration,
        "play" => ProtocolState::Play,
        other => panic!("unknown state {other}"),
    }
}

fn direction_of(name: &str) -> Direction {
    match name {
        "clientbound" => Direction::Clientbound,
        "serverbound" => Direction::Serverbound,
        other => panic!("unknown direction {other}"),
    }
}

fn reencode(
    state: ProtocolState,
    direction: Direction,
    id: i32,
    payload: &[u8],
    ids: &PacketIds,
) -> Result<(i32, Vec<u8>), String> {
    macro_rules! via {
        ($enum:ty) => {
            <$enum>::decode(id, payload, ids)
                .map(|packet| packet.encode(ids))
                .map_err(|e| e.to_string())
        };
    }
    match (state, direction) {
        (ProtocolState::Handshake, Direction::Serverbound) => via!(HandshakeServerbound),
        (ProtocolState::Status, Direction::Clientbound) => via!(StatusClientbound),
        (ProtocolState::Status, Direction::Serverbound) => via!(StatusServerbound),
        (ProtocolState::Login, Direction::Clientbound) => via!(LoginClientbound),
        (ProtocolState::Login, Direction::Serverbound) => via!(LoginServerbound),
        (ProtocolState::Configuration, Direction::Clientbound) => via!(ConfigClientbound),
        (ProtocolState::Configuration, Direction::Serverbound) => via!(ConfigServerbound),
        (ProtocolState::Play, Direction::Clientbound) => via!(PlayClientbound),
        (ProtocolState::Play, Direction::Serverbound) => via!(PlayServerbound),
        (state, direction) => Err(format!("no packets for {state:?} {direction:?}")),
    }
}

fn first_difference(a: &[u8], b: &[u8]) -> usize {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()))
}

#[derive(Default)]
struct Tally {
    passed: usize,
    failed: usize,
    first_failure: Option<String>,
}

#[test]
fn captured_frames_are_byte_exact() {
    let root = capture_root();
    if !root.is_dir() {
        eprintln!("skipping: no captures at {}", root.display());
        return;
    }
    let ids = PacketIds::v26_3();
    let mut failures = 0;
    for set in CAPTURE_SETS {
        let dir = root.join(set);
        let Ok(index) = fs::read_to_string(dir.join("index.jsonl")) else {
            eprintln!("skipping set {set}: no index.jsonl");
            continue;
        };
        let mut tallies: BTreeMap<(String, String, String), Tally> = BTreeMap::new();
        for line in index.lines().filter(|l| !l.trim().is_empty()) {
            let entry: serde_json::Value = serde_json::from_str(line).unwrap();
            let field = |key: &str| entry[key].as_str().unwrap().to_owned();
            let (state, direction, name) = (field("state"), field("direction"), field("name"));
            let id = entry["id"].as_i64().unwrap() as i32;
            let seq = entry["seq"].as_u64().unwrap();
            let path = dir
                .join(&state)
                .join(&direction)
                .join(&name)
                .join(format!("{seq}.bin"));
            let payload = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let outcome = match reencode(
                state_of(&state),
                direction_of(&direction),
                id,
                &payload,
                ids,
            ) {
                Ok((out_id, bytes)) if out_id == id && bytes == payload => Ok(()),
                Ok((out_id, bytes)) => Err(format!(
                    "seq {seq}: re-encoded id {out_id} len {} vs {} (first difference at byte {})",
                    bytes.len(),
                    payload.len(),
                    first_difference(&bytes, &payload)
                )),
                Err(e) => Err(format!("seq {seq}: {e}")),
            };
            let tally = tallies.entry((state, direction, name)).or_default();
            match outcome {
                Ok(()) => tally.passed += 1,
                Err(message) => {
                    tally.failed += 1;
                    tally.first_failure.get_or_insert(message);
                }
            }
        }
        let (mut passed, mut failed) = (0, 0);
        for ((state, direction, name), tally) in &tallies {
            passed += tally.passed;
            failed += tally.failed;
            if let Some(message) = &tally.first_failure {
                println!(
                    "[{set}] FAIL {state}/{direction}/{name}: {}/{} failed; {message}",
                    tally.failed,
                    tally.passed + tally.failed
                );
            }
        }
        println!("[{set}] {passed} passed, {failed} failed");
        failures += failed;
    }
    assert_eq!(failures, 0, "captured frames failed to round-trip");
}
