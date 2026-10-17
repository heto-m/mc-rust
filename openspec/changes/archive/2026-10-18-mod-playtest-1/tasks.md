## 1. Chat typing

- [x] 1.1 Route key text unless an IME composition is in progress; `Ime::Disabled` ends it; unit tests

## 2. Local actions and movement keys (ABI 16)

- [x] 2.1 `NET_ACTION`, `PLAYER_INPUT`, probe answers, `BRIDGE_VERSION` 16, Java constants and ABI test
- [x] 2.2 `Relay::local_action` from drop, swing, use, mining and attack reset; no punch after a use
- [x] 2.3 Java replays actions on its `LocalPlayer` and reports held keys to `ClientInput`
- [x] 2.4 Wire and relay unit tests; test mod tutorial probe and live net bridge test (drop, swing, tutorial)

## 3. Ocean blink

- [x] 3.1 Offscreen attempts to reproduce with and without mods (static, loading, flying, turning); not reproduced

## 4. Performance

- [x] 4.1 Profile mod mode (frame profile, JFR, GC log) windowed and offscreen
- [x] 4.2 Skip Java's dirty walk while nothing changed, narrow the block-entity walk, meshless sections need no
  neighbours; live entity scene test
- [x] 4.3 Document findings in `docs/mod-bridge.md`
