# Proposal

## Why

The owner's first long mod-mode playtest (29 mods, KDE Wayland) hit five functional bugs and lower frame rates
than the offscreen runs: an item dropped with Q stayed in Java's hotbar, Java's hand never swung while mining, typing
into Java's chat did nothing, the movement tutorial toast never went away, the ocean blinked from afar, and the
client averaged about 350 fps windowed against 1200 to 1600 offscreen.

## What Changes

- **Key text while text input is enabled.** On Wayland, winit reports `Ime::Enabled` as soon as the compositor
  offers text input, with no input method running. mcr dropped key text from that point, so Java's chat got no
  characters. Key text is now dropped only while an IME composition is in progress.
- **Local actions on Java's player.** Drops, swings, mining progress and attack-strength resets are client-side in
  vanilla and never come back from the server, so Java's `LocalPlayer` missed them. Rust now posts each one to
  Java as a `NET_ACTION` message and Java replays it with vanilla's own calls (remove from the selected slot, swing
  with the held item's animation, `Tutorial.onDestroyBlock`, reset the attack ticker).
- **Movement keys for Java.** Rust's player state carries the held movement keys; Java's `ClientInput` reports
  them as key presses for the tutorial and mods that read them, while movement itself stays Rust's.
- mcr no longer sends a punch after a successful item use (vanilla only swings locally).
- **Cheaper per-frame section walks in Java.** Java's visible list holds every section in the frustum since
  bridge sections hide nothing; vanilla's dirty walk and block-entity walk over it are skipped or narrowed while
  nothing changed, and meshless sections no longer wait for neighbour chunks.
- ABI: `BRIDGE_VERSION` 16 (15 is reserved). `NET_ACTION`, `PLAYER_INPUT` and three probe answers are added.

## Impact

- `mcr-app` (IME text routing, attack reset and dig progress forwarding), `mcr-client` (local action relay hook),
  `mcr-physics` (held keys), `mcr-modbridge` (ABI, wire, relay), `java/mcr-bridge` (player shadow actions and
  input, local player mixin, net probe, extractor and section mixins), `java/mcr-testmod` (tutorial probe), live
  tests in `mcr-modbridge/tests/net_bridge.rs`, `docs/mod-bridge.md`.
