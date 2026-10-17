# Design

## Setup

Ryzen 7 5800X, RX 6800 XT (RADV), offscreen 1920x1080, `.cache/play-mods` (29 jars, 101 mod containers). In game
the client is on a local vanilla server (port 25568, view distance 32) at noon with time frozen, in front of 6
villagers, 6 cows, 4 sheep and 2 armor stands (`NoAI`). Every number comes from one `mcr --screenshot … --bench
N` command line; in game after `--after-ticks 600` (30 s warm). The machine was shared with another agent's
builds and tests, so runs alternate before/after and the tables give every run.

## Where the Rust time went

`MCR_MOD_PROFILE` now splits the Rust time of Java's frame by kind (`gpu::backend::cost`): every encoder op,
pass replay, `finish`, staging, and the render thread's queue writes and submits of Java's work. Before this
change, in game (per Java frame, µs, calls in brackets):

| Kind | Java thread | Render thread |
|---|---|---|
| `E_SPLIT` (5) incl. `finish` | 172–220 | |
| `finish` (6) | 154 | |
| `E_FLUSH_MAPPED` (9) | 23–130 | |
| pass replay (5) | 38 | |
| clears (4) | 9 | |
| staging (6) | 9 | |
| queue writes (6) | | 25 |
| `queue.submit` + callbacks (6) | | 196–205 |

On the title screen the render thread, not Java, bounded the frame: 0.31 ms per frame against Java's 0.21 ms, and
its submits of Java's work took 0.082 ms of it.

A micro-benchmark of wgpu 30 on this GPU (`queue.submit` of one command buffer: 15–20 µs; each extra command
buffer in the same submit: about 6 µs; an empty render pass in `finish`: 4 µs) and an in-process sampling profile
(pprof, not committed) explained it. Every `queue.submit` pays a fixed cost (submission bookkeeping, the pending
writes buffer, `maintain` and triage of finished submissions), and every command buffer, render pass and usage
scope pays for wgpu tracker vectors sized to the device's texture and buffer index range. In game, mcr made four
submits per frame and Java's queued work four more.

## One submit per frame

`mcr-render` no longer submits at its hook points. `ExternalCtx::batch` (`FrameBatch`) collects the frame's
command buffers in order; the hooks add Java's after mcr's, and the frame ends with one `queue.submit`, wrapped
in an error scope. `ExternalSubmitter::submitted` hands the submission index back, and `Slots::submitted` then
records fence progress, arms `on_submitted_work_done` (once per encoder track) and maps readbacks.

Ordering rules that had relied on separate submits:

- **Host writes.** `queue.write_buffer` lands before the whole next submit. Java's staged bytes (upload ring and
  dirty transient ranges) only target Java's buffers, which mcr's command buffers never touch. Within Java's
  work, writes of one Java submit (an *epoch*: the splits up to its `E_SUBMIT`) never overlap: transient memory
  rotates only at a submit, and the upload ring now keeps its offsets across splits and resets only at a
  submit. An item with writes from another epoch or encoder therefore sends the batch first
  (`writes_conflict`).
- **The late-camera patch.** It had to land between the in-stream copy of Java's projection and the first world
  draw. It is now a copy from a small staging buffer, in its own command buffer placed exactly there.
- **Composite copies** (loading screens) go through the batch too.
- **Failures.** A queue submit with one invalid command buffer drops the whole batch, mcr's frame included.
  Java's `finish` runs in an error scope and drops a buffer whose recording failed (the downcall still reports
  the error). If a batch is rejected anyway, Java's work goes back to its own submits for the session.
- **Overflow.** When Java fills the slot queue (4096 items) while a batch is open, its overflow path waits up to
  2 s for the batch to be submitted before submitting on its own, so it cannot overtake batched work.

mcr's own encoder is held open across a hook (`FrameBatch::hold`/`resume`) and only finished when the hook adds
work after it. On the title screen Java has no world slots, so mcr records its whole frame in one command buffer.

## Downcall hot path

- **Lazy clears.** Java's full clears of single-layer textures wait in the encoder. The next pass that renders to
  the texture clears as it loads; a later clear of the same texture replaces the pending one; copies, readbacks,
  region clears, blits and passes that sample the texture record it first. Java clears a 1920x1080 depth texture
  twice per frame on the title screen that nothing reads before the next clear, so those passes are gone.
- **Mapped flushes** copy straight from the host shadow into the upload ring. They had built a temporary `Vec`,
  which at 256 KiB and above is a fresh `mmap` with page faults every time (mcr pins glibc's mmap threshold).
- **Pass replay** skips vertex buffers, index buffers, scissors and pipeline lookups that did not change. wgpu
  validates each `set_index_buffer` again, which was 13% of the Java thread's Rust time in game.

## Results

fps from `--bench` (title 5000 frames; in game 4000 with mods, 8000 without), three alternating runs each,
`main` at b6d1e0f against this branch merged with it:

| Scene | main | this change | Change |
|---|---|---|---|
| no mods, in game | 1593 / 1586 / 1589 | 1589 / 1589 / 1586 | none |
| play-mods title | 3067 / 3093 / 3134 | 4307 / 4281 / 4234 | +38% |
| play-mods in game (warm) | 1105 / 1029 / 1043 | 1303 / 1263 / 1228 | +19% |

After, per Java frame (µs, calls in brackets), from `MCR_MOD_PROFILE=frame`:

| Kind | In game | Title |
|---|---|---|
| `E_SPLIT` incl. `finish` | 125 (5) | 70 (2) |
| `finish` | 116 (5) | 54 (2) |
| pass replay | 40 (5) | 12 (2) |
| `E_FLUSH_MAPPED` | 10 (9) | 2 (3) |
| staging | 8 (6) | 1 (3) |
| clears | 2 (4) | 1 (2) |
| other encoder ops and device calls | 9 | 5 |
| queue writes (render thread) | 23 (6) | 9 (3) |
| `queue.submit` + callbacks (render thread) | 0.5 (1) | 0.4 (1) |

Rust inside Java's frame went from 0.251 to 0.185 ms in game and from 0.109 to 0.090 ms on the title screen;
Java's frame from 0.824 to 0.776 ms in game (GUI 0.161 to 0.122 ms). The render thread's own submits of Java's
work are gone: mcr's single frame submit carries it. The profile's render-thread `submit` figure (time in the
hooks' drains) is not comparable with before: a drain that adds Java work now also finishes mcr's held
encoder, whose passes wgpu encodes at `finish`. In game the frame is now bound by Java (`wait` 0.11 ms); on the
title screen by the render thread.

Without mods the frame rate is unchanged within noise.

## Rejected or left

- **Removing the `beginWorld` cut.** It only exists so the projection patch can land between Java's immediate
  work and its world draws. Patching the staged bytes instead would need the ring location of Java's projection
  write and the patch decided before `before_frame` drains that work. It would save one Java command buffer
  (finish fixed cost and one per-buffer submit cost, about 15–20 µs a frame) for a fragile coupling.
- **Handing the upload ring to the render thread** instead of copying it (`pooled_copy`), or a mapped staging
  belt instead of `queue.write_buffer`: staging plus queue writes are about 35 µs a frame in game across both
  threads; a belt needs map/unmap cycling per block. Left as a follow-up.
- **Reusing an unchanged GUI.** Still not done, for the reasons in mod-frame-pipeline: Java's GUI draws into the
  scene mcr redraws each frame and the menu blur samples it, so reuse needs an offscreen GUI layer, a composite
  pass and change detection that holds for animated widgets and mods that draw every frame. The GUI is
  0.13–0.15 ms of Java's frame, and in ahead mode it already overlaps mcr's frame.
- **mcr's own culling compute passes** run on the mod-mode title screen with no world loaded and cost about a
  fifth of the render thread there. Skipping them when there are no sections is an `mcr-render` change for
  another day.
