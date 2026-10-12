# Proposal

## Why

The mod layer is complete per plan but has only run Fabric API, Mod Menu and our own test mod. Nobody knows which
popular Fabric client mods for 26.3 boot, join a server and do their job under the bridge, or which bridge gaps
stop the rest.

## What Changes

- Sweep about 30 of the most-downloaded Fabric client mods for 26.3 (UI, visual, quality-of-life and performance)
  plus their libraries through the real `mcr --mods` binary offscreen against a vanilla server: each alone, then
  all together.
- Record the result per mod (works, partial, broken, blocked) with its root cause in `docs/mod-compat.md`.
- Fix the bridge gaps behind broken or partial mods where the fix is contained, with regression tests driven by
  the test mod rather than downloaded jars.
- Add policy entries with reasons for mods that cannot work under design B.

## Impact

- `mcr-modbridge` (policy data, bridge ABI or GPU backend as the fixes require), `java/mcr-bridge`,
  `java/mcr-testmod`, the mod-mode parts of `mcr-app`, `docs/`.
- New capability `mod-compat`: the compatibility record and the sweep procedure.
