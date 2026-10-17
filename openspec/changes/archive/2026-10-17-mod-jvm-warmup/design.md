# Design

## Setup

Ryzen 7 5800X (16 threads), RX 6800 XT, Temurin 25.0.4.1, `.cache/play-mods` (29 jars, 100 mod containers),
ZGC with compact object headers, `-Xmx2048m`. Offscreen 1920x1080. In game the client is on a local vanilla
server at noon with time frozen, in front of 6 villagers, 6 cows, 4 sheep and 2 armor stands (`NoAI`). Every
run is one `mcr --mods .cache/play-mods --screenshot … ` command line. Runs waited for a load average under 4,
because another agent was building on the same machine.

- **Title time**: from process start until Java's title screen is shown and loaded (new `title:` line).
- **Join time**: from shell start until Rust's player is in the world (`ready:` line, `--connect`).
- **fps at T**: `--bench-timeline 62` runs unpaced frames (ahead mode) from the moment of joining and prints the
  frames of every second. The value at T is the mean of seconds T−1 and T. `--after-ticks T --bench 2000` (the
  method from mod-frame-pipeline) plays T ticks paced and then measures 2000 unpaced frames.
- **Warm/cold**: warm runs had the OS page cache filled by the previous run. Cold runs first evicted every jar,
  asset, the JDK, the `mcr` binary and the game dir from the page cache (`posix_fadvise(DONTNEED)`,
  1.3–1.5 GB in 44,700 files). The Mesa shader cache stays warm in both.
- **No cache**: `MCR_JVM_AOT=off`. **Recording**: the first session of a mod set. **With cache**: later sessions.

## Results

Title time (s), title-screen runs:

| | warm | cold |
|---|---|---|
| no cache | 11.10 / 11.12 / 11.12 | 11.86 / 11.64 |
| recording session | 11.87 / 11.96 | — |
| with cache | 10.35 / 10.37 / 10.13 | 10.58 / 10.72 |

Join runs (`--connect … --bench-timeline 62`): title time, join time, and fps at 5/15/30/60 s after joining:

| | title (s) | join (s) | 5 s | 15 s | 30 s | 60 s |
|---|---|---|---|---|---|---|
| no cache, warm (3 runs) | 11.19 / 11.26 / 11.06 | 16.22 / 17.03 / 16.74 | 990 | 1154 | 1199 | 1195 |
| no cache, cold (2) | 11.92 / 11.94 | 17.59 / 17.95 | 945 | 1143 | 1189 | 1181 |
| recording session (2) | 12.22 / 12.75 | 17.50 / 18.16 | 945 | 1108 | 1185 | 1180 |
| with cache, warm (3) | 10.80 / 10.66 / 10.78 | 16.41 / 16.16 / 16.01 | 979 | 1179 | 1197 | 1200 |
| with cache, cold (2) | 20.66* / 10.67 | 22.05* / 16.21 | 969 | 1118 | 1158 | 1156 |

\* One outlier; the machine was shared with another agent's builds.

The mod-frame-pipeline method (`--after-ticks T --bench 2000`), fps:

| T | 100 (5 s) | 300 (15 s) | 600 (30 s) | 1200 (60 s) |
|---|---|---|---|---|
| no cache | 927 | 1152 | 1158 | 1202 |
| with cache | 873 | 1048 | 1200 | 1198 |

Quitting takes 0.35–0.4 s, or 3.9 s (title) to 5.2 s (in game) in a recording session. The helper builds the
cache in 3–4 s; the cache is 160–190 MB.

What this means:

- The cache saves 0.75–0.85 s to the title screen when the page cache is warm (7%), 1.1 s when cold (9%), and
  about 0.5 s to joining. An earlier, noisier series measured 10.5–10.9 s against 11.8 s to the title and 15.7 s
  against 17.6–17.9 s to joining.
- It does not change the frame rate after joining: every column is within run-to-run noise. The "380 fps
  15 s after joining" from mod-frame-pipeline was measured before its ahead mode; on today's `main` the same
  measurement gives 1150 fps, so the warm-up the owner saw is mostly gone already. What remains is JIT warm-up
  in Java's frame. The cache's method profiles do not speed that up with ZGC on JDK 25.
- The first session of a mod set pays about 0.8 s more to the title screen and 4–5 s more to quit.

## Where start-up goes

A JFR recording of a title-screen start (no cache) shows the Java main thread busy for the whole load. The JVM
is created 0.27 s after launch, so nearly all of the 11 s is Java. In 5.9 s of samples in Java code, the main
thread spent about 1.8 s in mixin transformation (ASM), 0.6 s defining and loading classes, 0.5 s in
Lithium's voxel-shape code and the rest in game and mod initialisation. 26% of those samples were in
interpreted code. JFR does not sample the rest of the main thread's time, which goes to class parsing, linking
and verification inside the VM: turning verification off alone saved 0.7–0.8 s. The main thread hardly blocks
on the worker threads, and it does not wait for mcr's frames until the title screen is up.

## What the AOT cache covers here

- **Embedding.** `-XX:AOTMode=record` and `-XX:AOTCache` work through `JNI_CreateJavaVM`. The recording is
  written in `DestroyJavaVM` (`before_exit`), 4–5 s for this mod set, so a recording session needs a clean
  JVM shutdown. A `System.exit` or crash leaves no recording. The one-step `-XX:AOTCacheOutput` workflow builds the cache in a child JVM that the exiting VM waits for, which would hold
  up quitting, so mcr uses the two-step workflow and its own detached helper instead.
- **Knot.** Classes defined by a custom loader are stored as "unregistered" classes and served from the cache
  when the bytes passed to `defineClass` match the recorded ones (checked with a toy loader and with Knot:
  16,100 of 33,500 classes came from the cache at the title screen). Mixin-transformed classes are recorded in
  their transformed form and hit as long as the transformation is deterministic, so Knot still reads the jar
  and Mixin still transforms, but HotSpot skips parsing and verification.
- **Not cached:** Knot's lambdas and other hidden classes (about 1,400), Fabric Loader's classes (431, signed
  jar), 234 old (pre-Java-6) classes that need the old verifier, and JFR event classes.
- **ZGC.** JDK 25 dumps heap objects only with G1, so a ZGC cache has no AOT-linked classes and no archived
  module graph (`aot-linked = 0`). JDK 26's JEP 516 removes that limit.
- **Profiles (JEP 515).** Recorded by default in record mode. Disabling their use at run time
  (`-XX:-AOTReplayTraining`) made no measurable difference to join time or to fps at 5–60 s.
- **Validation.** A cache from another GC, with another object-header setting, truncated, missing or random
  bytes is reported on stdout and ignored; the JVM starts normally (checked with the `java` launcher and, for a
  truncated cache, through `JNI_CreateJavaVM` in the `jvm_aot` test).
- **The game jar.** With `client.jar` on the system class path, `JNI_CreateJavaVM` in record mode took 9 s
  longer (title 21.9 s instead of 11.6 s), and the build step 9 s longer as well. The cause is that jar alone (its
  signed 5 MB manifest); other jars cost nothing. Knot loads the game from `fabric.gameJarPath` anyway, so the
  class path no longer includes it. Recording now costs about 1 s at start-up and the build takes 3–4 s.

## Decisions

- **Cache per key, under the game dir.** `<game dir>/cache/jvm-aot/<sha1>.aot`. Mod jars are hashed by content
  (30 MB: 15 ms with a warm page cache, 39 ms cold); class path entries by size and modification time, which is also
  what HotSpot checks. `-D` properties stay out of the key because they change per run (table address, proxy
  port).
- **Detached helper.** On a clean quit after recording, mcr renames `<key>.aotconf.part` to `<key>.aotconf`
  and spawns `mcr --jvm-aot-create <dir> <key>` in its own process group. The helper takes the key's file
  lock, lowers its priority, runs the `java -XX:AOTMode=create` command line that the recording session wrote
  to `<key>.job`, and renames `<key>.aot.part` into place. A start that finds the lock held, or a recording that
  waits for its build, runs without a cache and records nothing.
- **Bounds.** Two caches (the least recently used goes; a used cache's mtime is refreshed), 1 GiB per cache,
  three failed recordings or builds per key, other keys' leftovers removed after an hour. Typical sizes: cache
  160–190 MB, recording 190–220 MB (deleted once built).
- **Refused caches.** On Linux, mcr reads `/proc/self/maps` after the JVM starts and deletes a cache that is
  not mapped, so the next session records a new one.
- **UX.** Nothing to do for the user. The first session with a new mod set logs that it records, starts about
  1 s slower and takes 4–5 s longer to quit; the cache is ready a few seconds later. Changing, adding or
  removing a mod, updating the JDK or the bridge starts a new recording. `MCR_JVM_AOT=off` opts out.

## Rejected

- **G1 instead of ZGC.** With a cache, G1 reached the title in 9.2–9.5 s instead of 10.3–11.0 s and joined
  1.5 s sooner, because G1 caches can AOT-link classes. Without a cache G1 was 0.5 s faster. But Java's
  worst frame in a quiet 60 s scene rose from 2.9–3.1 ms to 4.5–5.0 ms, and the render thread waited up to
  12 ms once. mcr chose ZGC for frame pacing; trading it for start-up needs its own study under chunk loading.
  JEP 516 (JDK 26) gives ZGC the same cache features.
- **JIT thresholds.** `-XX:CompileThresholdScaling=0.5` and `2`: title time within noise (11.2–11.35 s against
  11.1–11.4 s), and no gain in game. `-XX:TieredStopAtLevel=1` reached the title 0.5 s sooner but gives up C2
  for the whole session. The default 12 compiler threads on 16 cores were left alone.
- **Verification off.** 0.7–0.8 s faster, but only with a diagnostic flag, and the cache already skips
  verification for the classes it holds.
- **CDS/AppCDS archives.** The JDK 25 AOT cache is a static CDS archive with AOT additions. A separate AppCDS
  or dynamic archive would cover the same classes with fewer features.

## Follow-ups

- Preloading the classes the last session loaded on background threads, so mixin transformation and class
  definition (about 2.4 s of the main thread at start-up) leave the critical path. This needs care with mixin
  configurations added during `preLaunch` and with mixin plugins that look at state at transform time.
- G1 or JDK 26 (JEP 516) for AOT-linked classes, measured against frame pacing.
