# Proposal

## Why

With the 29 mods in `.cache/play-mods`, mcr runs at about 3300 fps on the title screen and 1200–1600 fps in game,
against about 3900 fps in game without mods. In game, Java's frame takes about 0.68 ms, and 0.30 ms of that is
Rust work inside its GPU downcalls. The render thread spends another 0.23 ms per frame submitting Java's work.
mod-frame-pipeline left two items open: the per-command-buffer `finish` and `queue.submit` costs, and the cost of
Java's GUI. This change measures where the Rust side of Java's frame goes and cuts what pays.

## What Changes

- **Cost breakdown.** With `MCR_MOD_PROFILE` set, the profile line also splits the Rust time of Java's frame by
  kind: every encoder op, pass replay, command-buffer `finish`, staging, and the render thread's queue writes and
  submits of Java's work.
- **One submit per frame.** mcr no longer submits at its hook points. Its command buffers and Java's queued work
  go into one frame batch, in the same order as before, and the frame ends with a single `queue.submit`. Java's
  staged bytes are still written with `queue.write_buffer`, which lands before the whole batch. The upload ring
  is no longer reused between the splits of one Java submit, so writes of one submit never overwrite each other.
  Work of a different Java submit with writes flushes the batch first. The late-frame projection patch becomes a
  small copy inside the batch. A command buffer whose recording failed is dropped, so it cannot sink mcr's frame;
  a rejected batch switches Java's work back to its own submits for the session.
- **Cheaper downcalls.** Measured per op; changes are kept only where they pay (see design).
- No ABI change (`BRIDGE_VERSION` stays as on `main`).

## Impact

- `crates/mcr-render`: `FrameBatch` in `ExternalCtx`, one submit per frame, `ExternalSubmitter::submitted`.
- `crates/mcr-modbridge`: `gpu/slots.rs` (batched drains, epochs, patch copy), `gpu/backend.rs` (cost buckets,
  ring per submit, validated finish), `gpu/ffi.rs`, `profile.rs`, `compose.rs`.
- Specs: `mod-frame-compose` (slot-ordered submission, frame profile).
