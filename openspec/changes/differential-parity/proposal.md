## Why

Parity rounds 1–7 found gaps by reading the decompiled client and checking live. That finds what someone
thinks to look at and misses the rest: a wrong UV on one block state, a tooltip line in the wrong order, a
movement edge case on one stair shape. The goal is now a client that behaves exactly like vanilla 26.3
before mod work resumes, and "exactly" needs a measurement.

We already run the real client jar in our embedded JVM, with its renderer on our own GPU backend. The
real client can therefore act as an oracle: give vanilla's code and ours the same input, record both
outputs in one normalized form, and diff them over thousands of generated cases.

## What Changes

- A differential harness framework:
  - Java **oracles** that call vanilla's classes from the user's own client jar;
  - Rust **probes** that run our code on the same cases;
  - a seeded case generator both sides read;
  - JSONL results;
  - a diff report that names the first mismatching case;
  - `cargo xtask parity` to run, cache and select harnesses.
- Harnesses over pure client logic:
  - text layout and components;
  - translations;
  - block model baking;
  - section meshing;
  - item models;
  - tooltips;
  - options.txt;
  - key mappings;
  - entity model poses;
  - local player physics.
- A **draw-stream** harness. The vanilla client runs headless in the JVM on our GPU backend, which
  records its GUI and world draws. Our client records the same normalized stream. The two are compared
  per screen and HUD state, and per world frame at a fixed camera.
- A **frame** harness: the same server, world and camera, with the vanilla frame replayed through our
  wgpu backend against our native frame. It reports a pixel diff and diff images.
- Gate integration:
  - `merge.sh` runs the harnesses whose inputs a branch touched;
  - a parity table, with a score per subsystem, is kept in the `differential-parity` spec;
  - every failing case found is filed into the next `vanilla-parity-N` change.

## Out of scope

- Server-side logic: world generation, mob AI, redstone and commands. The vanilla server jar runs
  them, so they match by construction.
- Mod mode. Mods are paused until the vanilla client reaches 100% on these harnesses.
- Committing any oracle output. Results come from Mojang's code and assets and stay under `.cache/`.

## Impact

- New spec: `differential-parity`.
- New code:
  - the `crates/mcr-parity` probes, a dev-only crate;
  - `java/mcr-oracle`, our own Java, built by `cargo xtask java-build`;
  - an `xtask parity` subcommand.
- Small, behaviour-free recording hooks in `mcr-ui` (batch export) and `mcr-render` (frame and draw
  capture) behind a cargo feature, plus a recording mode in the `java/mcr-bridge` GPU backend.
- No protocol changes.
