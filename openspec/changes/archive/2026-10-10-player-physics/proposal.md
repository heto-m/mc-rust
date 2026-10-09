# Proposal

## Why

The server validates client movement and rubber-bands any player whose reported position diverges from its own simulation. The client must therefore reproduce vanilla player movement exactly: same arithmetic, same operation order, same collision epsilons.

## What Changes

- Add `mcr-physics`, a deterministic `f64` reimplementation of the vanilla local-player tick: input to acceleration, gravity and drag, collision with block shapes, step-up, sneaking edge safety, fluids, climbing, stuck-in-block effects, creative flight and fall-distance tracking.
- Add the `sendPosition` decision logic that picks the serverbound move packet kind (pos, rot, pos+rot, status only) and its flags.
- Abstract world access behind a `CollisionWorld` trait so tests use tiny synthetic worlds; provide the implementation for `mcr_world::ChunkStore`.
- Derive per-state physics facts (collision boxes, friction, speed and jump factors, fluids, climbable, bounce) once from `mcr-data`.

Out of scope: world border, entity collisions, elytra, vehicles and riding, mob effects other than jump boost, levitation and slow falling, block-entity driven shapes, scaffolding collision context, validation against real server traces (a later change).

## Capabilities

### New Capabilities
- `player-physics`: client-side player movement simulation and movement packet decision logic.

### Modified Capabilities

## Impact

New crate `crates/mcr-physics` depending on `mcr-data` and `mcr-world`. No changes to existing crates.
