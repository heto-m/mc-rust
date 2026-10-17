# Proposal

## Why

Three popular camera mods are only partial under the bridge because mcr owns the camera perspective and cycles it
itself on F5. Shoulder Surfing's own perspective is never reached with its default config, since mcr's F5 drains
the vanilla key and mcr forces Java's camera type back to its own. Freelook's default third-person view is
overruled for the same reason. Camera Overhaul rolls the level's pose stack inside `GameRenderer.bobHurt`, and mcr
replaces Java's whole level projection with its own view, so only the hand tilts.

## What Changes

- **Java leads the perspective in mod mode.** While Java draws the world, Java's `Options` camera type (including
  the camera types mods switch to, and the perspectives mods layer on top of them) decides. Java reports it every
  frame, and mcr follows it for its world camera, its own player model and its hand. F5 is no longer handled by
  mcr then: the key reaches Java's vanilla `keyTogglePerspective`, so mods that cycle perspectives on it and
  rebinding in Java work. Java no longer forces its camera type to mcr's; on the frame Java switches, vanilla's
  own alignment runs and the result reaches mcr through the camera handoff until mcr has followed.
- **Mod rolls of the level pose.** Java compares the pose stack vanilla's `bobHurt` and `bobView` built with the
  pose vanilla alone would build from the same state, and reports the extra roll about the view axis. mcr rolls
  its view by it on top of its own tilts, so Camera Overhaul's roll reaches the world.
- Without mods nothing changes: no JVM runs, and mcr cycles its perspective on F5 as before.
- ABI: `BRIDGE_VERSION` 14. The frame state gains `FSTATE_PERSPECTIVE` and
  `FSTATE_VIEW_ROLL`.

## Impact

- `mcr-modbridge` (ABI, frame state, compose link), `mcr-app` (perspective following, F5, view roll, autopilot
  report), `java/mcr-bridge` (camera mixin, gameplay keys, perspective and pose-roll reports), `java/mcr-testmod`
  (perspective cycle and pose roll), live tests in `mcr-app/tests/entity_scene.rs`, `docs/mod-bridge.md`,
  `docs/mod-compat.md`.
