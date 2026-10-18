# Design

## Setup

Ryzen 7 5800X (16 threads), RX 6800 XT, Temurin 25.0.4.1, `.cache/play-mods` (29 jars), ZGC with compact object
headers, `-Xmx2048m`, offscreen 1920x1080, warm page cache, the AOT cache in use unless noted. Every run is one
`mcr --mods .cache/play-mods --game-dir … --screenshot …` command line; join runs add `--connect
localhost:25569 --bench-timeline 30` against a fresh vanilla 26.3 world at noon (`advance_time` off). Runs waited
for a load average under 2.5, and preload-on and preload-off runs alternate, so both modes share the machine's
noise. Another agent ran its own `mcr` during part of the series.

- **Title**: the `title:` line, process start to Java's title screen.
- **Join**: the `ready:` line, from the start of the shell loop to Rust's and Java's player in the world.
- **On**: a class list from an earlier session exists (the default). **Off**: `MCR_JVM_PRELOAD=off`.

## Results

Title time (s), title-screen runs:

| | runs | median |
|---|---|---|
| AOT cache, preload off | 10.84 / 10.79 / 10.67 / 11.91 / 11.53 | 10.84 |
| AOT cache, preload on | 9.01 / 8.98 / 9.09 / 17.87* / 9.45 | 9.09 |
| no AOT cache, preload off | 12.57 / 12.38 / 11.96 | 12.38 |
| no AOT cache, preload on | 10.29 / 9.72 / 9.95 | 9.95 |

\* The whole process stalled for 9 s during `preLaunch` (before preloading starts); the machine was busy.

An earlier series on the same build gave 10.56 / 10.30 / 10.48 s off against 8.78 / 8.68 / 8.74 s on, and `main`
before this change 10.14 / 10.32 s (AOT cache) and 11.12 / 10.95 s (no cache).

Join runs (`--connect … --bench-timeline 30`):

| | title (s) | join (s) | fps at 5 s | Java frame max over 30 s (ms) |
|---|---|---|---|---|
| preload off | 11.62 / 10.99 / 11.35 / 14.84* | 12.35 / 11.85 / 12.07 / 16.89* | 2533 / 2478 / 2469 / 1385* | 14.0 / 37.7 / 46.3 / 24.1 |
| preload on | 9.05 / 9.10 / 9.57 / 9.36 | 9.84 / 9.97 / 11.46 / 10.07 | 2275 / 2466 / 2532 / 2464 | 22.2 / 11.9 / 19.4 / 6.4 |

\* A slow run (load from another agent).

First seconds after joining (`MCR_MOD_PROFILE=frame`, two runs each): mcr fps, the render thread's longest wait for
Java, and Java's mean frame, per second.

| | second 1 | second 2 | second 3 |
|---|---|---|---|
| off | 110 / 94 fps, 100 / 100 ms, 8.4 / 10.1 ms | 399 / 421 fps, 36 / 48 ms, 1.9 / 2.3 ms | 965 / 860 fps, 10 / 8.5 ms, 0.96 / 1.05 ms |
| on | 175 / 229 fps, 73 / 26 ms, 4.1 / 2.7 ms | 550 / 1032 fps, 5.5 / 5.8 ms, 1.0 / 0.75 ms | 2046 / 2024 fps, 3.5 / 4.9 ms, 0.45 / 0.46 ms |

Logging alone (a session that records but has no list yet) against off: 12.53 / 10.70 / 11.05 s against
10.66 / 11.82 / 10.79 s, within the noise.

What the preload thread did, per segment (cumulative, a join run): after `main` 11,630 classes loaded and 74
found already loaded, in 3.1–3.6 s; after the game loop starts 12,430; after the title screen 14,480; after login
15,230–15,300, 8–9.5 s after `main`. 430–441 classes were left to the game (MixinExtras and Mixin), none for
plugins: no plugin in this mod set has a non-empty `preApply` or `postApply`. One class went missing (a class a
mod defines itself). The thread ran ahead of the render thread for almost the whole list: only 1–2% of the
classes were already loaded when it reached them.

What this means:

- Preloading saves 1.4–1.8 s to the title screen with the AOT cache (about 16%) and 2.4 s without it, and about
  2 s to joining. With both, Java reaches the title screen in about 9 s instead of about 11 s on the same
  machine state, and about 10.2 s on `main`'s earlier measurement.
- The first second after joining halves Java's mean frame and the longest wait. The worst Java frame of a 30 s
  join session tends to be lower; the steady frame rate does not change.
- The gains add to the AOT cache's: the cache skips parsing and verification, preloading moves Knot's reads,
  the mixin transformation and definition off the render thread.
- Preloading is on by default.

## How it works

- **Recording.** `-Xlog:class+load=info:file="<key>.<pid>.classlog":none:filecount=0` makes HotSpot write
  `<name> source: <where>` per class, about 0.5 MB for a join session. The option is added after the AOT key is
  computed and is not part of it. `ClassList::finish` parses at most 64 MiB, drops names before the first marker,
  hidden classes (`/`), `$$` names, `org.spongepowered.asm.synthetic.` and JDK packages, keeps the first load of
  each name, and merges: per segment this session's order first, then the old list's classes it did not load.
  The list is written to `.part` and renamed. It holds at most 200,000 names of at most 512 bytes in at most 16
  segments; Java applies the same bounds when it reads it.
- **Milestones.** `McrPreload.milestone(k)` loads `McrPreload$Mark<k>` (and the markers of skipped milestones)
  under a lock: `Main.main` HEAD (0), `Minecraft.runTick` HEAD (1, after every client entrypoint), `TitleScreen.init`
  (2), `ClientPacketListener.handleLogin` TAIL (3). The marker's load is the log line that splits the list. Classes
  the last session loaded between marker k and k+1 wait for milestone k. This keeps a class from being
  transformed before the state that existed when the game last loaded it (mod initialisation, the title screen,
  the world) exists again.
- **Why at `main`.** Fabric runs every `preLaunch` entrypoint before it loads the game's main class, and Mixin
  selects new configs when the next class is transformed. When `Main` is transformed all configs are visited, so
  the preload thread never selects configs on its own thread. It still checks that no config was added before
  each segment and stops if one was.
- **Loading.** `Class.forName(name, false, knot)` with Knot as the initiating loader, as the game's own references
  resolve. Already loaded classes are skipped (`FabricLauncher.isClassLoaded`). `ClassNotFoundException` and
  `LinkageError` count and continue. Anything else (a mixin apply failure surfaces as a `RuntimeException` from
  Knot) stops preloading and reports `preload_stopped`, which Rust counts as a bad session.
- **No initialization.** `forName(…, false, …)` defines and loads superclasses and interfaces but runs no static
  initializer, and Knot's transformers (Mixin, access wideners, environment stripping) run no game code. Linking
  was tried (`getDeclaredConstructors` links without initializing): title 8.97 / 8.91 / 8.85 s against
  8.84 / 8.80 / 9.22 s without, so it was dropped.

## Locks and the deadlock

Knot is parallel capable and holds a per-name lock while it loads a class, including during the transformation.
Mixin's `MixinTransformer.transformClass` is `synchronized`, so all transformation is serialized, and Knot calls
it for every class. During a transform Mixin can load classes through Knot: MixinExtras' injector classes (a
nested jar of the loader, defined by Knot) and plugin code. The first prototype hung at the first class:

- the render thread transformed a class inside `SharedConstants.<clinit>` while holding Mixin's lock, and
  MixinExtras' `FactoryRedirectWrapperInjectionInfo.<init>` loaded another MixinExtras class through Knot;
- the preload thread held that class's Knot lock (it was next in the list) and waited for Mixin's lock.

Holding Mixin's lock around the preload of Mixin's own classes avoids that cycle but leaves a race with a class
the render thread loads at run time. The design leaves those classes to the game instead: the preload thread
never loads `org.spongepowered.`, `com.llamalad7.mixinextras.`, `com.bawnorton.mixinsquared.` or the package of
any registered Mixin extension, so it never holds a lock a transform waits for. With that rule it takes locks in
the order every game thread does (a class's Knot lock, then Mixin's lock), which is what Fabric already does
with the game's worker threads. Plugins whose `preApply`/`postApply` do anything may load their mod's classes
during a transform, so their mod's package (the plugin's package without its last segment) and their targets are
skipped too. A plugin method counts as empty when its body is a single `RETURN` (read with ASM from the declaring
class); an unreadable one counts as acting.

## Safety net

`ClassList::begin` writes a strike before a preloading session starts, and `finish` removes it after a clean
shutdown without `preload_stopped`. A session that crashes, hangs (the bridge detaches after 2 s, the VM cannot be
destroyed) or is killed therefore leaves its strike. Two strikes turn preloading off for the key until the key
changes. Logs of dead sessions for the key are deleted at the next start. Lists are evicted by recency (four
other keys kept); the AOT eviction no longer deletes them as stale leftovers.

## Rejected

- **Instrumentation or JFR for the class list.** `-javaagent` adds a JVMTI agent with class-file hooks, which can
  change what the AOT cache may use; JFR costs hundreds of milliseconds at start-up. HotSpot's log costs nothing
  measurable and records every loader's classes in order.
- **Caching transformed bytes across sessions.** Plugins and mod state at transform time make it unsafe, and the
  AOT cache already skips parsing and verification of identical bytes.
- **More preload threads.** Mixin's lock serializes the transformation, and the single thread already stays
  ahead of the render thread.
- **Aging the list.** Once a class is preloaded the game's own load of it cannot be observed, so the list cannot
  tell needed classes from stale ones. It is the union of the sessions of one key, bounded, and starts over when
  the key changes (any mod, JDK, option or bridge change).

## Not measured

Cold page cache, and mod sets with plugins that act at transform time (none in the curated set). The `jvm_preload`
JVM test covers the record, preload and off paths with Fabric API and Mod Menu.
