# Tasks

## 1. Foundations

- [x] 1.1 Create `mcr-physics` with the sine table, `Aabb`, `CollisionWorld`, and `PhysicsData` built from `mcr-data`.
- [x] 1.2 Implement block collision gathering, axis resolution, step-up and supporting block.

## 2. Player tick

- [x] 2.1 Implement attributes, input handling, sprint/crouch rules, jump, gravity, friction and move.
- [x] 2.2 Implement fluids, climbing, stuck-in-block, bounce, edge-safe sneaking and creative flight.
- [x] 2.3 Implement fall distance, ground/collision flags and the `sendPosition` decision.

## 3. Verification

- [x] 3.1 Unit tests: jump apex, sprint jump, terminal velocity, wall, slab step, ice, water, sneak edge, web, flight, packet decisions.
- [x] 3.2 Criterion benchmark for a tick in a dense area; run fmt, clippy and tests clean.
