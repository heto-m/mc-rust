## Why

The machine running mc-rust has 15 GiB of RAM and an out-of-memory kill takes the whole terminal session with it. `vanilla-polish-3` fixed the texture-layer leaks but left the remaining long-lived collections, GPU buffers, threads and channels unchecked, and nothing measures memory over a long session. A client that is only leak-free by inspection is not provably leak-free.

## What Changes

- Audit every long-lived collection reachable from the client state, the app's `Play`/`Shell`/world, the renderers, audio, UI screens, the mod bridge and the net layer. Anything that grows with server data or time and is not cleaned on its natural lifecycle (entity removal, chunk unload, player leave, dimension change, respawn, reconfiguration, disconnect) is fixed and gets a churn test that asserts it stays bounded.
- Disconnect, reconnect and dimension change free everything world-scoped.
- wgpu's internal resource counters are enabled; the renderer and the entity renderer expose cheap GPU allocation counters.
- `mcr --soak <secs>` runs the shell offscreen against a server, reconnects after a kick and prints a stats line every 10 seconds.
- `cargo xtask soak` starts a dedicated vanilla server, runs `mcr --soak` in its own memory-capped scope, drives churn through the server console (entity waves, far teleports, dimension changes, `/reload`, chat, titles, bossbars, scoreboards, items, signs and banners, weather and time, kicks), samples RSS, threads and GPU counters into a CSV and fails on a sustained RSS slope or thread growth.

## Impact

- Affected specs: new `long-session-memory`; `dev-tooling` (soak command).
- Code: `xtask` (soak), `mcr-app` (soak mode, lifecycle cleanup), `mcr-render` and `mcr-entity-model` (counters), plus the crates whose collections the audit fixes.
