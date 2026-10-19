# Design

## Setup

Ryzen 7 5800X, RX 6800 XT (RADV), 15 GiB RAM, another agent building at times. Copies of main's Fabric bench
servers on port 25584 (peaceful, noon, `tick freeze`), copies of main's bench game dirs. Main (cc83303) runs from
its own root (`run/main-root`, its own `.cache/java` build) and game dirs, the branch from the worktree, so each
keeps its own AOT cache and class list. Windowed: headless gamescope 2560x1440, MangoHud, 60 s after a 30 s
settle; join is the server's "joined the game" (whole seconds) minus process start, RSS from `/proc`. Gamescope
kills mcr's detached AOT helper when the game exits, so caches are built by hand (`run/bench-real/aot.sh`).

## Where the time went (Medieval, windowed join 28 s on main)

Startup trace (seconds after launch): mod scan 0.35, window 0.40, renderer 0.49, AOT key and JVM creation to 1.13.
Java: Fabric and mod init to 9.6, resource reload 9.7 to 25.2, then Java's title screen.

- **2.0 s: the loading overlay's fade.** mcr connected only once `gui.overlay()` was null, which vanilla's
  `LoadingOverlay` clears two seconds after the reload finished. Vanilla's quick play connects when the reload
  finishes, as did real Minecraft in the comparison.
- **Pack zips listed entry by entry.** JFR: worker threads spent most of the reload in
  `FilePackResources.listResources`, which walks every entry of a pack zip for every directory listed (Alacrity
  has 27,518 entries; sprite listers, JSON loaders and EMF's resource scans list hundreds of directories).
- **Before the JVM:** the mod scan and the AOT key's content hashes ran one jar at a time (0.35 s and 0.16 s).

## Where the memory went (Medieval, 4.5 GB steady)

NMT: Java heap 2.3 GB committed (ZGC), JVM non-heap 0.66 GB (226 MB AOT class space). smaps: 284 MB `[heap]`,
1.67 GB other anonymous memory. glibc's `mallinfo2` at join + 60 s: 1.2 GB in arenas of which 0.69 GB free;
`malloc_trim(0)` took 62 ms and dropped RSS from 4643 to 4034 MB. A sampling heap profiler (`--features
heap-profile`, `MCR_HEAP_PROFILE=<secs>:<file>`) found 784 MB of live Rust allocations, among them two complete
block model caches (127 MB each): the HUD's style reader and the skin store held the join-time `Assets`.

## Changes

1. Java reports itself loaded once the reload is done (`LoadingOverlay.fadeOutStart` set), not when the overlay
   is gone; the render report waits the same way.
2. `FilePackResources.listResources` reads a per-zip index sorted by name (`ZipIndex`), in the zip's order.
3. The mod scan and the AOT key's jar hashes run on up to eight threads.
4. `HeapTrim`: `malloc_trim(0)` on a short-lived thread once loading activity (sections meshed, Java's title, the
   title assets) stopped for 2 s, at most every 20 s.
5. The HUD and skin readers hold the resource manager, not `Assets`.

## Results so far (windowed Medieval, interleaved)

| Run | join (s) | fps | p50 / p99 / p99.9 / max (ms) | peak RSS (MB) | steady RSS (MB) |
|---|---|---|---|---|---|
| main 1 | 28.3 | 826 | 1.15 / 2.35 / - / 94.0 | 4585 | 4483 |
| main 3 | 28.5 | 839 | 1.14 / 2.27 / 5.48 / 34.3 | 4689 | 4584 |
| branch 2 | 16.6 | 798 | 1.14 / 2.81 / 7.17 / 101.4 | 4477 | 4045 |
| branch 4 | 17.0 | 821 | 1.16 / 2.31 / 5.38 / 91.1 | 4440 | 4025 |

Main 2 and branch 3 overlapped another agent's build (505 and 501 fps) and are left out.

### ZGC soft maximum (change 6)

`-XX:SoftMaxHeapSize` at two thirds of `-Xmx` and `-XX:ZUncommitDelay=5`: ZGC collects to stay under the soft
maximum and gives back what it did not use for five seconds. Offscreen startup unchanged (title 15.36 s against
15.57 s). Windowed Medieval:

| Run | join (s) | fps | p50 / p99 / p99.9 / max (ms) | peak RSS (MB) | steady RSS (MB) |
|---|---|---|---|---|---|
| main 4 (busy machine) | 28.9 | 553 | 1.78 / 3.87 / 8.27 / 101.3 | 4588 | 4523 |
| branch 6 | 17.0 | 839 | 1.14 / 2.28 / 6.19 / 32.2 | 4368 | 3177 |
| branch 5 (busy machine) | 17.2 | 681 | 1.18 / 3.66 / 6.66 / 31.4 | 4328 | 3154 |

### mimalloc for Rust's allocations (dropped)

Windowed Medieval, alternating: mimalloc 818 / 819 fps, steady RSS 3311 / 3312 MB, peak 4361 / 4161 MB; glibc
with the trims 828 / 843 fps, steady 3212 / 3176 MB, peak 4335 / 4367 MB. Higher steady RSS and 1–3 % fewer
frames, so the global allocator stays glibc's.

## Echoes of the Sift

Offscreen warm start: title 9.58 s and join 10.29 s on main, 7.32 s and 8.32 s on the branch. Windowed: main
joined in 10.2–10.6 s at 1525–1531 fps (p99 0.84–0.86 ms), steady RSS 3237–3246 MB, peak 3281–3283 MB; the branch
joined in 8.5–8.8 s at 1512–1522 fps (p99 0.86–0.89 ms), steady 2328–2365 MB, peak 2536–2679 MB. Real Minecraft:
join 9.6 s, 1502–1875 fps, p99 1.23–1.47 ms, peak 2.87–3.06 GB.

What is left of the boot (JFR, main thread): mixin transforms and class loading to about 2.5 s, then vanilla's
`Bootstrap` builds every block state's shape cache (`BlockStateBase.initCache` from `Blocks.<clinit>`) serially
for about 2 s. Real Minecraft pays the same; running it in parallel would race mods' lazily filled shape maps, so
it stays.

## Present path

mcr never forces X11: winit takes native Wayland whenever `WAYLAND_DISPLAY` is set, so on a Wayland desktop it
already presents natively. Under gamescope with `--expose-wayland`, Echoes windowed ran at 1772–1816 fps (p50
0.54–0.55 ms, p99 0.69–0.77 ms, present 0.135 ms) against 1512–1522 fps through Xwayland (present 0.21 ms). The
bench keeps Xwayland so its numbers compare with earlier rounds and with real Minecraft under the same gamescope.

## What did not pay or was not ours

- mimalloc (above).
- Exceptions under JFR: a HUD mod (RPG Attribute System) probes JEI by reflection every frame and throws
  `ClassNotFoundException` for `mezz.jei.api.runtime.JeiRuntime`; real Minecraft throws the same. About 120
  `NullPointerException`s in `McrRegistries.writeItems` at the title and the preloader's misses are off the
  critical path.
- `MALLOC_ARENA_MAX=4` was not retried: mod-perf-3 measured 200 MB less RSS for 3 % fewer frames, and the trims
  return more without the cost.

## Final A/B (interleaved, 2560x1440, render distance 12)

Windowed is headless gamescope with MangoHud over 60 s; offscreen is `--bench-timeline` (join, a 3000-frame burst
right after it, then the steady rate). Runs marked busy overlapped other agents' builds and benches.

| Run | title (s) | join (s) | fps | p50 / p99 / p99.9 / max (ms) | peak RSS (MB) | steady RSS (MB) |
|---|---|---|---|---|---|---|
| Medieval windowed main 1 | | 28.1 | 834 | 1.15 / 2.34 / 4.95 / 36.4 | 4644 | 4559 |
| Medieval windowed main 2 (busy) | | 28.8 | 786 | 1.20 / 2.49 / 6.30 / 101.9 | 4519 | 4448 |
| Medieval windowed branch 1 | | 16.9 | 797 | 1.13 / 3.26 / 11.44 / 103.7 | 4440 | 3116 |
| Medieval windowed branch 2 | | 16.9 | 834 | 1.15 / 2.29 / 3.63 / 40.4 | 4377 | 3144 |
| Medieval offscreen main 1 / 2 | 26.7 / 30.4 | 29.5 / 31.8 | 454 / 492 burst, 780 / 577 steady | | 4557 / 4640 | 4428 / 4548 |
| Medieval offscreen branch 1 / 2 | 16.1 / 18.7 | 17.4 / 19.5 | 856 / 849 burst, 843 / 840 steady | | 4378 / 4228 | 3229 / 3229 |
| Echoes windowed main 1 | | 10.9 | 1510 | 0.65 / 0.92 / 1.38 / 12.3 | 3284 | 3231 |
| Echoes windowed main 2 | | 11.2 | 1523 | 0.64 / 0.84 / 1.19 / 4.1 | 3365 | 3310 |
| Echoes windowed main 4 | | 12.3 | 1507 | 0.64 / 0.93 / 1.36 / 2.9 | 3286 | 3232 |
| Echoes windowed branch 3 | | 8.4 | 1517 | 0.64 / 0.87 / 1.05 / 2.9 | 2685 | 2307 |
| Echoes windowed branch 4 (busy) | | 9.3 | 1423 | 0.65 / 1.38 / 4.12 / 85.8 | 2731 | 2294 |
| Echoes offscreen main 1 / 2 | 10.2 / 10.7 | 11.4 / 11.5 | 1797 / 1880 burst | | 3266 / 3317 | 3222 / 3265 |
| Echoes offscreen branch 1 / 2 | 8.1 / 11.4 | 9.3 / 12.1 | 2039 / 2791 burst | | 2697 / 2748 | 2304 / 2398 |

Medieval windowed branch 1 had a one-second dip every ten seconds: ZGC's major cycles (1.2–1.6 s, concurrent), which
the soft maximum brings round every 10–11 s instead of about 15 s on main; minor cycles run every half second on
both. Echoes windowed branch runs 1 and 2 (976 and 1003 fps) had every stage slower, GPU passes included (draw
0.14 ms against 0.10), while criterion benches of another agent ran; the same binary gave 1511 and 1522 fps an
hour earlier and 1517 in run 3. Screenshots match main: Echoes differs in 0.002 % of pixels, Medieval in 0.5–0.8 %,
all in the bobbing first-person hand, against 0.35 % between two main runs.

`fabric_boot` passes in a network namespace with only loopback. Online, it found an HTTPS tunnel to
`discovery.minecraftservices.com` (vanilla's service discovery through the proxy) still open at the title screen;
that request is the same on main.
