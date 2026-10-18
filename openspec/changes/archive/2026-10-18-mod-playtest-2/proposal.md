# Proposal

## Why

Three leftovers from the owner's mod-mode playtest remain open after `mod-playtest-1`: distant ocean blinks at
render distance 32, Controlling and Durability Tooltip never loaded in the play set because
their library mods were missing, and Difficulty in Java's World Options is greyed out in local mcr worlds
because Java has no integrated server.

## What Changes

- **Translucent draws follow uploaded geometry.** When a frame's uploads exceed the 64 MiB budget, the rest
  waits for later frames, but `set_section` switched a section's translucent draw range, tint table and slot at
  once. The translucent pass then drew water from arena memory that had not been written yet, or that still
  held another section's quads, with a tint table that was not uploaded: water vanished, turned black or showed
  wrong quads for a frame. A section's translucent draw now switches only when its upload batch is written,
  and the old range stays drawable until then.
- **Distant kelp no longer blinks.** The blink reproduced offscreen is distant kelp, seen through the ocean,
  vanishing on two of its twenty animation frames: cutout mips scaled each frame on its own, so those frames
  lost all coverage at the coarsest levels. Animated cutout sprites are now mipped as one strip with one
  coverage scale, as vanilla does.
- **Difficulty in mod mode.** In a local mcr world Java's difficulty button and lock are active for the owner,
  as in vanilla singleplayer. Java sends the choice to Rust over `REPORT_WORLD` (`difficulty\t<id>`,
  `lockdifficulty`). Rust changes the difficulty through the managed server's console (`difficulty <name>`),
  or with the vanilla packet when the player is an operator, so no admin chat line appears. The lock holds for
  the rest of the session in both clients (`WORLD_LOCKED`) and is written into `level.dat` once the server
  stopped, so the server reports it from the next start on. Hardcore stays locked.
- **Difficulty packets on remote servers.** `change_difficulty` and `lock_difficulty` join the outbound forward
  list, so an operator's World Options work on a remote server in mod mode too.
- **Play set.** Searchables and SuperMartijn642's Config Lib are added to the local play set (not committed),
  and Controlling and Durability Tooltip are checked in mod mode.
- ABI: `BRIDGE_VERSION` 19 (or the next free number when merged), `WORLD_LOCKED` added.

## Impact

- `mcr-render` (translucent commits on flush), `mcr-assets` (strip-wide cutout mips), `mcr-modbridge` (ABI, world requests and events, forward list),
  `mcr-app` (difficulty requests, lock on stop), `mcr-singleplayer` (`level.dat` lock),
  `java/mcr-bridge` (World Options difficulty mixin, packet reroute, lock flag), live and GPU tests,
  `docs/mod-bridge.md`.
