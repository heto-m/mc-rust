# Design

## Setup

Ryzen 7 5800X, RX 6800 XT (RADV), 15 GiB RAM. Two Fabric packs from `cargo xtask modpack-fetch`: Medieval Immersed
(102 client mods, 16 resource packs) and Echoes of the Sift (12 client mods). Each served by a copy of main's
bench server on port 25584 (peaceful, fixed noon, the bench spawn and rotation, `tick freeze` once loaded), each
client in a copy of the bench game dir (render distance 12, vsync off; heap 2304 MiB chosen for Medieval, 2048 MiB
automatic for Echoes). Main and this branch run from the same root and game-dir copies; only the binary differs.

- **Offscreen**: `mcr --connect … --mods … --size 2560x1440 --after-ticks 600 --bench-timeline 60 --bench 3000`
  with `MCR_MOD_PROFILE=frame`. "bench" is the fixed 3000-frame run after the timeline; "tail" is the frame time
  over the timeline's last 31 seconds.
- **Windowed**: headless gamescope at 2560x1440 (`--backend headless`, private `XDG_RUNTIME_DIR`), MangoHud frame
  log over 60 s after a 30 s settle; RSS is the process's peak (`VmHWM`), join the server's "joined the game"
  minus process start.

Another agent shared the machine (builds and GPU runs), so runs alternate and every run is listed.

## Where the time went

- **Java's frame, Rust side.** wgpu 30 records commands and encodes them only in `CommandEncoder::finish`. With
  `MCR_MOD_PROFILE=frame`, `finish` was the largest Rust cost inside Java's frame (Medieval, frozen: 0.21 of the
  0.29 ms Rust spent in Java's 1.54 ms frame). In ahead mode Java queues its encoders and mcr drains them later into one batch, so nothing
  needs them finished on Java's thread.
- **wgpu-core trackers.** eu-stackprof (sampling every thread, flattened with gprof) put
  `ResourceMetadata::clear` and its drop high in `finish` and in encoder drops. Both walk `resources` up to the
  largest resource index ever allocated: Java's loading releases about 11,000 textures, of which 3,221 stay live
  (3,180 Java's), so every tracker of every encoder walked about 11,000 slots to clear a few dozen.
- **Java itself (also in real Minecraft, not fixed here).** JFR (through `JAVA_TOOL_OPTIONS`; `-Xrs` blocks
  jcmd) on Medieval: Entity Model Features with Fresh Animations about 45% of the render thread, the first-person
  hand alone 11–13%; RPG Attribute System's HUD calls `Class.forName("mezz.jei…")` and throws about five exceptions
  per frame (5.8%); Entity Texture Features reads NBT per entity.
- **Spikes.** The slow-frame profile puts Java's slow frames (over twice the mean, about 1% of frames) in its tick
  (Echoes: 0.36–0.43 of 0.61–0.69 ms), i.e. server-tick-aligned work, not the bridge. Windowed maxima (86–96 ms in
  Medieval) are equal on main and the branch.
- **Present.** Under gamescope's Xwayland (Mesa's X11 WSI), `queue.present` blocks about 0.145 ms per frame on
  the render thread, 0.21 ms with MangoHud; MangoHud costs Echoes about 0.07 ms per frame (1710–1750 fps without
  it, 1520–1555 with it).
- **Join.** At Java's title, Java reported its 16 selected packs and mcr reloaded every resource synchronously
  (about 2 s in Medieval) before connecting.
- **RSS** (Echoes, NMT plus smaps): Java heap 1.88 GB of memfd (ZGC commits the whole `-Xmx`), JVM non-heap
  0.35–0.4 GB, non-JVM anonymous memory about 1.25 GB of which a large share is glibc per-thread malloc arenas
  (16–58 MB each).

## Changes

1. **Finisher** (`gpu::finisher`): queued encoders go to the `mcr-gpu-finish` thread over a 256-deep channel;
   the submission holds an `Encoded` that the render thread takes when it drains. A full or closed channel
   finishes inline. wgpu's error scopes are per thread, so the worker pushes its own validation scope and drops an
   invalid buffer with a warning, as before.
2. **Tracker patch** (vendored `wgpu-core` 30.0.1, `[patch.crates-io]`): `clear` and `Drop` release only the
   members, found through the occupancy bit vector, and keep the table's length. Every change against upstream is
   in `vendor/wgpu-core/MCR_PATCHES.md`.
3. **Pack prediction**: the first resource stack already holds the packs Java's `options.txt` selects, and the
   comparison with Java's selection waits for Java's first render report (its first, empty report used to trigger
   a reload of its own).
4. **SIGTERM** quits through the normal path. Windowed benches stop mcr with SIGTERM; before, every such run left a
   preload strike and two strikes switched preloading off for the pack.
5. **Tools**: the slow-frame profile, the timeline tail, `MCR_JVM_EXTRA`, acquire and present time in the stats
   line.

## Tracker patch alone

Same commit, built with stock and with patched `wgpu-core`, alternating, offscreen with `MCR_MOD_PROFILE=frame`.

Echoes of the Sift (GPU-light, mcr-bound), fixed 3000-frame bench and steady tail p50:

| Run | stock fps (ms) | patched fps (ms) | stock / patched tail p50 (ms) | render-thread submit (ms) |
|---|---|---|---|---|
| 1 | 2530 (0.395) | 2768 (0.361) | 0.387 / 0.353 | 0.129 / 0.115 |
| 2 | 2519 (0.397) | 2823 (0.354) | 0.388 / 0.348 | 0.130 / 0.113 |
| 3 | 2584 (0.387) | 2722 (0.367) | 0.384 / 0.360 | 0.128 / 0.116 |
| mean | 2544 (0.393) | 2771 (0.361) | 0.386 / 0.354 | |

Frame time 8.2% lower, steady p50 8.4% lower.

Medieval Immersed (Java-bound, 1.3 ms Java frames). Runs 1 of stock and 3 of patched overlapped another agent's
build (455 and 490 fps) and are left out:

| Run | fps (ms) | finish CPU on the worker (µs per Java frame) | render-thread submit (ms) |
|---|---|---|---|
| stock 2 | 754 (1.326) | 181.9 | 0.187 |
| stock 3 | 752 (1.329) | 188.6 | 0.184 |
| patched 1 | 767 (1.304) | 144.4 | 0.142 |
| patched 2 | 747 (1.338) | 146.4 | 0.150 |

Frame time unchanged (Java bounds it), CPU time in `finish` 21% lower and in the render thread's submit 22%
lower. The patch is kept on the Echoes result; the Medieval CPU saving is headroom for when Java gets faster.

## Did not pay

| Tried | Result |
|---|---|
| G1 (`MCR_JVM_EXTRA=-XX:+UseG1GC`) | RSS about 1 GB lower, throughput equal, but 5–10 ms young pauses in steady play (138 ms at load) and the title about 10 s later |
| ParallelGC | Throughput equal, long pauses |
| ZGC with `-XX:SoftMaxHeapSize` | No RSS change: ZGC keeps the committed heap |
| Presenter thread (acquire and present off the render thread) | No fps change, and 1 s hitches |
| Acquiring the swapchain image late | No change |
| Mailbox present mode | No change under gamescope |
| `vk_xwayland_wait_ready=false` | No change |
| `MALLOC_ARENA_MAX=2` | Peak RSS about 200 MB lower (Echoes 3125/3139 against 3331/3343 MB), fps 3% lower (2672/2635 against 2731/2760) |
| Reusing Java's previous layers | Java consumes its command buffers and overwrites its buffers every frame |
| Pass replay off Java's thread | About 55 µs per frame to win, with a second copy of every pass |

## Results

Main is this branch's base (83ff0b3); "final" is the branch head. Runs alternate main and final, and a run is
held until the machine's load average is under 3. Runs that overlapped another agent's builds anyway are listed
apart and left out of the comparison.

**Offscreen**, 2560x1440. fps is the fixed 3000-frame bench. p50, p99 and max come from the paced timeline
(chunk loading included, so they are wider than steady play); final's steady 31-second tail is listed below it.

| Pack | Run | fps (ms) | p50 / p99 / max (ms) | peak RSS (MB) | title / ready (s) |
|---|---|---|---|---|---|
| Medieval | main 1 | 654 (1.528) | 2.61 / 6.75 / 27.5 | 4647 | 32.0 / 35.0 |
| Medieval | main 2 | 659 (1.517) | 2.56 / 9.68 / 36.4 | 4671 | 26.0 / 34.1 |
| Medieval | final 1 | 764 (1.308) | 2.14 / 6.66 / 26.1 | 4684 | 25.8 / 27.6 |
| Medieval | final 2 | 749 (1.335) | 2.25 / 5.83 / 25.0 | 4655 | 26.4 / 28.4 |
| Echoes | main 1 | 2582 (0.387) | 0.78 / 2.04 / 6.9 | 3300 | 9.7 / 13.8 |
| Echoes | main 2 | 2474 (0.404) | 0.78 / 2.10 / 12.4 | 3327 | 9.5 / 11.8 |
| Echoes | final 1 | 2846 (0.351) | 0.61 / 1.86 / 14.5 | 3309 | 9.7 / 12.0 |
| Echoes | final 2 | 2738 (0.365) | 0.62 / 1.92 / 13.3 | 3327 | 9.6 / 12.0 |

Final's steady tail: Medieval p50 1.27 / 1.29, p99 2.26 / 2.19, p99.9 3.34 / 3.41, max 100.5 / 39.5 ms; Echoes
p50 0.345 / 0.352, p99 0.484 / 0.485, p99.9 0.85 / 0.86, max 2.95 / 2.51 ms. Steady fps in Echoes: main 2453 /
2387, final 2744 / 2689. Left out: main Medieval 432 fps, final Medieval 745 fps (same bench, timeline disturbed).

**Windowed**, headless gamescope 2560x1440, MangoHud, 60 s:

| Pack | Run | fps | p50 / p99 / p99.9 / max (ms) | peak RSS (MB) | join (s) |
|---|---|---|---|---|---|
| Medieval | main 1 | 641 | 1.51 / 2.58 / 4.27 / 97.0 | 4682 | 30.4 |
| Medieval | main 2 | 622 | 1.53 / 2.79 / 4.26 / 93.3 | 4760 | 29.3 |
| Medieval | final 1 | 755 | 1.28 / 2.24 / 5.76 / 101.9 | 4630 | 28.1 |
| Medieval | final 2 | 722 | 1.28 / 2.79 / 6.19 / 99.9 | 4617 | 28.0 |
| Echoes | main 1 | 1401 | 0.69 / 0.97 / 1.29 / 3.5 | 3290 | 10.4 |
| Echoes | main 2 | 1389 | 0.69 / 1.06 / 1.51 / 19.4 | 3245 | 10.1 |
| Echoes | main 3 | 1407 | 0.68 / 1.14 / 2.45 / 11.5 | 3280 | 10.7 |
| Echoes | final 1 | 1519 | 0.64 / 0.89 / 1.12 / 17.2 | 3320 | 11.1 |
| Echoes | final 2 | 1500 | 0.64 / 0.95 / 1.76 / 89.0 | 3278 | 11.1 |

Left out: Medieval main 505 and final 527 fps, Echoes final 1187 fps (another agent's build). Echoes final 2 ended
at load 4. Without MangoHud, final Echoes runs at 1710–1750 fps.

Summary: Medieval +15% fps offscreen and +17% windowed (p50 1.52 to 1.28 ms windowed), Echoes +10% offscreen and
+8% windowed with p99 0.97–1.14 down to 0.89–0.95 ms. Medieval joins 1.8 s sooner windowed; offscreen it goes from
Java's title to the game in 1.8–2.0 s instead of 3.0–8.1 s; Echoes, which selects no resource packs, joins as before (windowed joins of earlier runs spread
from 10.0 to 11.6 s for both). Peak RSS is unchanged. Real Minecraft with the same mods (`bench-real-minecraft.md`,
ticking server): Medieval 644–730 fps, p50 1.29–1.45, p99 3.0 ms; Echoes 1502–1875 fps, p99 1.23–1.47 ms.

## Remaining gaps

- **RSS** stays 0.9–1.0 GB above real Minecraft in Medieval. Most of it is ZGC committing the whole heap (G1 gives
  back about 1 GB but pauses) and glibc's per-thread arenas (`MALLOC_ARENA_MAX=2` saves 0.2 GB at 3% fps).
- **Medieval's frame** is Java's: Fresh Animations, the RPG Attribute System exception storm and ETF cost the same
  in real Minecraft. A bridge-side fix would be mod-specific.
- **Presenting** through Xwayland costs 0.15 ms per frame, which is 20% of Echoes' frame windowed.
