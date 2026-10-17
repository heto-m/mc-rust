# Proposal

## Why

With the 29 mods in `.cache/play-mods`, Java needs about 11 s to reach the title screen, and mcr joins a
server 16–17 s after launch. Most of that is Java start-up work that repeats on every launch: loading,
verifying and linking about 33,500 classes, many of them through Fabric's Knot loader and transformed by
mixins, and running cold code in the interpreter until the JIT compiles it. JDK 25 can keep much of this in an AOT
cache (JEP 483, 514 and 515): the classes as loaded and linked, and the method profiles of a training run.
Nobody had measured what that cache covers in mcr's setup: an embedded JVM created through
`JNI_CreateJavaVM`, Knot as a custom class loader, mixin-transformed classes and ZGC.

## What Changes

- **AOT cache per mod set.** The first session with a new mod set records its classes and method profiles
  (`-XX:AOTMode=record`). After a clean quit, a detached, low-priority helper (`mcr --jvm-aot-create`) builds
  the cache with `java -XX:AOTMode=create`. Later sessions start from it (`-XX:AOTCache`). The cache key covers
  the JDK, the JVM options, the class path, the content of every mod jar and the bridge ABI. A changed key
  records again. Caches live under `<game dir>/cache/jvm-aot`. At most two are kept, each at most 1 GiB. A key
  whose recording or build fails three times stops trying. `MCR_JVM_AOT=off` turns it off.
- **Never breaks startup.** HotSpot validates the cache and ignores one that does not match (verified for a
  different GC, a different object layout, a truncated file, a missing file and random bytes). On Linux mcr
  checks that the JVM mapped the cache and deletes one it refused, so the next session records a new one.
- **Game jar off the class path.** Knot loads `client.jar` from `fabric.gameJarPath`. On the system class path,
  its 5 MB signed manifest made AOT recording spend 9 s at JVM creation.
- **Shutdown while recording.** `DestroyJavaVM` writes the recording (about 4–5 s with 29 mods), so a
  recording session waits up to 120 s for it instead of 10 s.
- **Measuring.** `mcr --screenshot … --bench-timeline S` runs unpaced frames for S seconds after joining (or
  after the title screen appears) and prints the frame rate of each second, plus the frame rate at 5, 15, 30
  and 60 s. The shell flow prints when Java's title screen appears.
- Rejected after measuring: lower JIT compile thresholds, G1 instead of ZGC, and separate CDS/AppCDS archives.
  The reasons are in the design.
- No ABI change: `BRIDGE_VERSION` stays 12.

## Impact

- `crates/mcr-jvm`: new `aot` module (key, plan, record, build, eviction, helper), `JavaHome::java`,
  `JvmConfig::destroy_timeout`.
- `crates/mcr-modbridge`: `BridgeConfig::aot` (`AotSettings`), the warm-up plan in `Bridge::start`, recording
  and helper start in `Bridge::shutdown`, `LaunchPlan::mod_jars`, game jar off the class path.
- `crates/mcr-app`: the helper entry point in `main`, `AotSettings` from the game dir, status lines, the
  title time and `--bench-timeline`.
- Specs: new `jvm-warmup`; `jvm-host` (shutdown bound while recording); `fabric-boot` (class path).
- Docs: `docs/mod-bridge.md`.
