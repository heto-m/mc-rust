# Design

## Measuring

Java times its frame phases with `System.nanoTime` (`McrFrameTimes`, about ten calls per frame). The hooks are
`awaitFrame` (pre: packets, input and size), `Minecraft.tick`, `Minecraft.renderFrame`, `GameRenderer.extract`,
`GameRenderer.render`, `GameRenderer.renderLevel` and `GuiRenderer.render`. Java writes the phases as `u32`
microseconds into the frame state's spare bytes (`FSTATE_PROF`, eight counters at offset 80) before it signals the
frame done.

The render thread records each mcr frame in `FrameProfiler` (`profile.rs`): the period from one `before_frame`
to the next, its waits for Java, and its drains of Java's queued work. Each finished Java frame adds Java's
phases and the backend counters accrued since the previous one: passes, draws, submits, uploaded bytes and pass
encode time. When `MCR_MOD_PROFILE` is set, every GPU downcall is also timed (`ffi_ns`, `ffi_calls`). That costs
two `Instant::now` calls per downcall, so it stays behind the flag. The flag also prints one line per second. It
starts `McrProfiler` in Java too: a JFR `RecordingStream` that samples the render thread (Java and native
frames) every millisecond. Each sample goes to the innermost frame on the stack that belongs to a mod. Mixin
handlers merged into vanilla classes are recognised by the mod id in their name (`handler$zza000$modid$…`), and
class owners are found once per class through the mods' root paths. A sample with no mod frame goes to Minecraft.
A native sample under vanilla or the bridge counts as Rust GPU work. Samples taken while the thread is parked in
the frame doorbell are idle and do not count. Nested jars count for the mod that contains them.

`mcr --screenshot out.png --bench N` (without `--world`) now runs N unpaced frames after the shell flow has
settled. This works on the title screen, and after `--connect … --after-ticks T` in game. It prints offscreen fps
and the profile totals, so every number below comes from one command line.

## Where the time went

The scene: Ryzen 7 5800X, offscreen at 1920x1080, `.cache/play-mods` (29 jars, 101 mod containers). In game,
the client is on a local vanilla server, at noon with time frozen, in front of 6 villagers, 6 cows, 4 sheep and
2 armor stands (all `NoAI`). The "before" binary is `main` (c3564f9) with only the bench added.

**The JIT warms up for a long time.** On `main`, 15 s after joining (`--after-ticks 300`), the game ran at
379 fps, with Java's frame at a 1.35 ms p50 and the render thread waiting 1.12 ms. 30 s after joining
(`--after-ticks 600`), the same build ran at about 817 fps. The owner's "~350 fps" matches the early number. All
the comparisons below were taken after the longer warm-up, three runs each.

Breakdown on the title screen before the changes (lockstep, 1875 fps, 0.53 ms per frame): Java's frame took
0.28 ms, and 0.15 ms of that was Rust work inside its 27 downcalls. Fence waits took 68 µs a frame (5 calls at
12.6 µs), readback polls 41 µs (6 calls at 6.5 µs), and two flush submits forced by zero-timeout fence polls
about 50 µs. Each command buffer `finish()` cost about 33 µs (wgpu 30 encodes the deferred passes there). The
render thread spent 0.12 ms submitting three items at about 35 µs each.

Breakdown in game before the split merge (ahead mode, first warm-up minute, `MCR_MOD_PROFILE=1`): Java's frame
took 1.62 ms. Of that, world was 0.93, extract 0.29, GUI 0.24 and ticks 0.06 ms. 0.59 ms was Rust work in its
downcalls, where six `E_SPLIT` calls at 68 µs each (49 µs of that in `finish()`) were the largest item. The render
thread spent 0.48 ms submitting seven items at about 67 µs each.

Breakdown in game now (warm, `MCR_MOD_PROFILE=frame`, which skips JFR): 1379 fps. Java's frame takes 0.68 ms:
world 0.42, GUI 0.14, extract 0.075, ticks 0.011 ms. 0.30 ms of it is Rust work in 62 downcalls, mostly five
command-buffer finishes, 13 mapped-buffer flushes and five pass replays. The render thread waits 0.05 ms and
spends 0.23 ms submitting. Each Java frame has 5 passes, 47 draws and 331 KiB of uploads. JFR sampling with
`MCR_MOD_PROFILE=1` roughly halves the frame rate, so take its shares as relative:

| Owner of render-thread samples (warm, in game) | Share |
|---|---|
| Minecraft (no mod frame on the stack) | 47% |
| Native: Rust GPU calls under vanilla or the bridge | 37–39% |
| fabric-api (all modules) | 4–5% |
| mcr bridge (Java side) | 3% |
| entity_texture_features | 2.2% |
| entity_model_features | 1.4–1.8% |
| lithium | 0.6–1.1% |
| every other mod | under 1% each |

On the title screen, architectury (11%) and modmenu (10%) lead the mods. No single mod dominates in game. The
cost is vanilla's own frame work plus the Rust GPU calls that it makes.

Results (fps, offscreen 1920x1080, three runs each):

| | before (`main`) | after, lockstep | after, automatic (ahead) |
|---|---|---|---|
| No mods, title | 7190 / 7129 | 7188 / 7239 (no JVM) | same |
| No mods, in game | 3615 / 3947 / 3940 | 3884 / 3961 / 3898 (no JVM) | same |
| play-mods, title | 2165 / 2094 / 2003 | 2790 / 2721 / 2779 | 3344 / 3392 / 3347 |
| play-mods, in game | 821 / 815 / 816 | 955 / 972 / 935 | 1618 / 1592 / 1594 |
| play-mods, in game, 15 s after join | 379 | — | 588 (before the split merge) |

The late-frame camera rewrite was checked with `MCR_BENCH_TURN=0.5`, which turns the camera 0.5° every frame for
730 frames. The final frame in ahead mode matches lockstep except for the hand: 30,670 pixels differ by more
than 40 levels. With the rewrite off it does not: 77,304 pixels differ, and the villagers visibly slide against
the terrain.

## Not done

- **Reusing an unchanged GUI.** The GUI is 0.13 ms of the title frame and 0.14 ms of the game frame, about a
  fifth of Java's frame. Java's GUI draws straight into the scene, which mcr redraws every frame, and the menu
  blur samples it. Reusing it would need an offscreen GUI layer, a composite pass and a way to tell that nothing
  changed (animated widgets, mods drawing every frame). That is a larger change than what it would save, now that
  ahead mode overlaps the GUI with mcr's frame.
- **Fewer command buffers.** `finish()` (about 30 µs) and `queue.submit` (about 35 µs) per Java command buffer
  are the largest Rust costs left. The splits that remain separate work mcr submits at different points, so
  merging them further would mean submitting Java's work together with mcr's in one `queue.submit`.

## One frame ahead

Lockstep cost `pre + max(java, opaque encode) + post` per frame. Ahead mode costs `max(java, mcr)`.

- **Posting.** `before_frame` checks the outstanding frame without blocking. A finished frame becomes `ready`
  and is composed in this mcr frame. In ahead mode the next frame is posted right away. When nothing has
  finished yet, `after_opaque` waits for the frame with the same bound as before, composes it and, in ahead
  mode, posts the next one at that point. Lockstep posts at `before_frame` only, and only when no older frame is
  ready to compose. A frame that finishes late is composed in the next mcr frame and never dropped. One posted
  frame at most is outstanding, so `TickBank`, the hang watchdog and the re-attach count behave as before.
- **Ordering.** Queued work now carries its frame index. Every drain takes an upper frame bound (`immediate_upto`,
  `world_upto`, `before_gui_upto`, `frame_upto`) and still pops strictly in FIFO order. The frame Java records
  ahead therefore waits behind the one being composed, even when a frame has no GUI cut to stop at. The render
  thread holds the queue lock only for each pop, so Java keeps queueing while mcr submits. A separate `exec` lock
  keeps the overflow path, where Java submits itself, from interleaving with a drain.
- **Camera.** A frame composed one mcr frame after it was posted was recorded from the previous camera. Java's
  level projection is `flip · P · V · R⁻¹`. Here `R` is Java's view rotation, which every model-view matrix
  carries, and geometry is relative to Java's camera `Cj`. After `RenderSystem.setProjectionMatrix` in
  `renderLevel`, Java reports the projection buffer slice, `Cj` and `R⁻¹` (`D_LEVEL_VIEW`). When mcr composes the
  frame late, it arms a patch with `flip · P_now · V_now · T(Cj − C_now) · R⁻¹`. The patch is a
  `queue.write_buffer` placed just before that frame's first world-slot submit. It lands after the in-stream copy
  that wrote Java's own matrix, and before any world draw. Entities, block entities, particles and outlines then
  sit on mcr's current terrain. Billboards still face the previous camera, and fog distances use it too. Both are
  off by one frame of motion. The hand and the GUI do not use the level projection.
- **Choosing.** `Pipelining::Auto` (the default) runs ahead when the app reports uncapped frames (vsync off and
  no fps limit, `Shell::set_mod_uncapped`), or when Java's mean frame time rises above 8 ms. Under vsync it
  returns to lockstep once Java's mean frame time falls below 4 ms. `MCR_MOD_PIPELINE=lockstep|ahead|auto`
  overrides the mode, and so does `FrameLink::set_pipelining`.
- **Latency.** In ahead mode, Java's GUI reacts to input one mcr frame later. At the uncapped rates where ahead
  mode is used, that is under 1 ms. Capped and vsynced play stays in lockstep, so its latency does not change.
  Mods that read Java's camera see the camera of the frame Java is rendering, as before. Freecam's handoff still
  reaches mcr one frame later.
- **Torn reads.** In ahead mode Java can publish frame state while the shell reads the camera, FOV and weather
  fields at the start of a frame. Each field is atomic, and consecutive frames differ by one frame of motion, so a
  mixed read is harmless.

## Fences and polls

At first, fence waits and polls made up most of the Rust time spent in Java's downcalls on the title screen.
Each frame made about 6 `E_POLL` calls at 6.5 µs, about 5 `E_WAIT` calls at 9 to 13 µs, and two flush submits that
zero-timeout fence polls forced.

- `E_POLL` returns at once when the encoder has no pending readback. `device.poll` is only needed to deliver
  `map_async` callbacks.
- A fence wait on queued work blocks only if a readback recorded at or before that submit is still pending.
  Java's host writes (`flush_mapped` copies, transient blocks, the upload ring) all reach the GPU as copies in
  queue order. A buffer that a fence "frees" early is therefore never overwritten before an earlier draw has
  read it. The in-flight list is trimmed when submits execute, so it stays bounded without waits.
- A zero-timeout wait on the open recording's fence returns "not done" in Java, without a downcall. It used to
  flush the recording and rotate the transient memory. A blocking wait still flushes, as before.

## Fewer command buffers and submits

- Mcr submits `WORLD_SOLID` and `WORLD_TRANSLUCENT` at the same point (`after_opaque`), and `WORLD_AFTER` and
  `POST_WORLD` at the same point too (`after_world`). `FrameSlots.cutTo` therefore only retags the slot between
  them. It cuts the command buffer only when the submit point changes, or when a render pass is open across the
  boundary. The pass targets are resolved per pass from the current slot, so the hand still gets its private
  depth. The cut at `beginWorld` stays. It keeps the in-stream copy of Java's level projection in an earlier
  command buffer than the world draws, so the late-camera rewrite lands between them. This removed one finish
  and one queue submit per game frame.
- A queued item with no command buffer and no readback (Java's end-of-frame submit when the GUI cut already
  took everything) no longer calls `queue.submit`. Its writes stay pending and go out with the next submit,
  still ahead of any later command buffer, and its completion is signalled with `on_submitted_work_done`.
- `MCR_MOD_PROFILE=frame` gives the frame profile without the JFR sampler, which slows Java down.
