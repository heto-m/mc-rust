# Tasks

## 1. Helpers and entity packets

- [ ] 1.1 Add `LpVec3`, `VecDelta`, `PositionPath`, relative-flag and shared enum helpers in `entity.rs`; verify with byte fixtures (`cargo test -p mcr-protocol entity`).
- [ ] 1.2 Add entity spawn, movement, teleport, data, equipment, effect, attribute and passenger packets with roundtrip tests (`cargo test -p mcr-protocol entity`).

## 2. World packets

- [ ] 2.1 Add block, chunk batch, border, time, ticking, sound, particle, explosion, event, waypoint and debug packets with roundtrip and fixture tests (`cargo test -p mcr-protocol world`).
- [ ] 2.2 Add chunk/biome/light packets and a test feeding their payloads to `mcr-world` (dev-dependency); verify the section block update bit layout against the fixture.

## 3. Verification

- [ ] 3.1 Run `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` and `openspec validate protocol-play-world --strict`.
