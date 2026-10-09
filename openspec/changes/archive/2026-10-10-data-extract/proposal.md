# Proposal

## Why

The client needs game facts (collision shapes, light, opacity, packet ids, registries) that the vanilla data reports do not contain. They must be derived reproducibly from the user's own game files and exposed through a version-aware API.

## What Changes

- Add `tools/extractor`, a small Java program that bootstraps the game headlessly and dumps per-state, per-block, item, entity type and fluid facts as JSON.
- Add `mcr-data-gen`, a binary that turns reports plus extracted JSON into compact generated Rust under `mcr-data`.
- Add `mcr-data`, a typed version-aware API with `v26_3()`.

## Capabilities

### New Capabilities
- `game-data`: version-aware lookup of block states, shapes, registries and packet ids.

### Modified Capabilities

## Impact

New crates `mcr-data` and `mcr-data-gen`, new `tools/extractor`. Generated tables are game facts only; no Mojang code, jars or textures are committed.
