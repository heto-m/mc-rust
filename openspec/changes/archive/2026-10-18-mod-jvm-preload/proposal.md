# Proposal

## Why

With the 29 mods in `.cache/play-mods` and the AOT cache from `mod-jvm-warmup`, Java still needs about 10.2 s to
reach the title screen. The render thread spends about 2.4 s of that loading classes through Fabric's Knot loader:
reading them from the jars, running Mixin's transformation on them and defining them. Nothing else runs in
parallel with it until the title screen is up. The same classes load in the same order on every launch with the
same mod set, so this work can start early on another core.

## What Changes

- **Class list per mod set.** Every mod-mode session logs its class loads with HotSpot's `class+load` log into a
  per-process file next to the AOT cache. At shutdown mcr merges it into `<key>.classes` (header
  `mcr-jvm-preload 1`), under the AOT key, so a changed mod, JDK, option or bridge starts a new list. Classes the
  session did not load stay in the list.
- **Milestones.** Java loads a marker class when it reaches the game's `main`, the first frame of the game loop,
  the title screen and the login packet. The markers split the list into segments; a class the last session
  first loaded after milestone k waits until this session reaches k.
- **Preloading.** When a list exists, a daemon thread (`mcr-preload`) in the bridge mod starts at the game's
  `main` (all `preLaunch` entrypoints and their mixin configs are in by then) and loads each listed class with
  `Class.forName(name, false, knot)`: defined and mixin-transformed, never initialized.
- **Left to the game:** classes of Mixin and its extensions (MixinExtras), which Mixin loads while it holds its
  transformer lock: the first prototype deadlocked holding one of their class locks while the render thread
  transformed. Also classes in mixin packages, and the targets and mod packages of mixin plugins whose
  `preApply` or `postApply` does anything (they may look at state at transform time). Preloading stops for the
  session if the mixin configs change after `main` or a class fails to transform.
- **Safety net.** A preloading session counts as a strike until it ends cleanly, so a crash, hang or kill
  counts. Two strikes turn preloading off for that key. `MCR_JVM_PRELOAD=off` turns recording and preloading off;
  `on` preloads where the default would not.
- **Measured** on the curated mod set, with numbers in the design: title time, join time and the first frames.
- No ABI change: `BRIDGE_VERSION` stays as on `main`.

## Impact

- `crates/mcr-jvm`: new `preload` module (plan, log option, log parsing, merge, list format, strikes, eviction);
  the AOT eviction leaves class lists alone.
- `crates/mcr-modbridge`: the preload plan next to the AOT plan in `Bridge::start`, the list merged in
  `Bridge::shutdown`, strikes from `preload_stopped` reports; JVM test `jvm_preload`.
- `java/mcr-bridge`: `McrPreload`, a `Main.main` mixin and milestone calls in the game loop, title screen and login.
- `crates/mcr-app`: a status line.
- Specs: `jvm-warmup` gains preloading requirements.
- Docs: `docs/mod-bridge.md`.
