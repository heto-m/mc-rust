## 1. Measure

- [x] 1.1 `--bench-timeline S` (fps per second after joining or after the title) and the title time in the shell flow
- [x] 1.2 Baselines: title time, join time and fps at 5/15/30/60 s with `.cache/play-mods`, ZGC, no cache
- [x] 1.3 Where start-up goes (JFR on the main thread: Java, mixin, class loading, frame waits)

## 2. Evaluate

- [x] 2.1 AOT cache with the embedded JVM: what it covers (Knot classes, mixin-transformed classes, lambdas, signed jars), training data, ZGC versus G1
- [x] 2.2 Invalid caches: other GC, other object layout, truncated, missing, random bytes
- [x] 2.3 Record-mode overhead; game jar off the class path
- [x] 2.4 JIT flags (compile thresholds, C1 only) and verification, with numbers
- [x] 2.5 CDS/AppCDS: covered by the AOT cache, which is a static CDS archive with AOT extensions

## 3. Build

- [x] 3.1 `mcr_jvm::aot`: key, plan, begin/end recording, job, helper build with lock, size bound, eviction, attempts, with unit tests
- [x] 3.2 `JvmConfig::destroy_timeout`, `JavaHome::java`
- [x] 3.3 Bridge: `AotSettings`, plan in `Bridge::start`, refused-cache check, recording and helper start in `shutdown`; game jar off the class path
- [x] 3.4 App: helper entry point (`--jvm-aot-create`), cache dir under the game dir, status lines
- [x] 3.5 JVM test (`MCR_JVM_TESTS=1`): record, build, use (mapped), truncated cache refused while the JVM still starts

## 4. Finish

- [x] 4.1 Final numbers (no cache, recording session, with cache; page cache cold and warm) in design.md; `docs/mod-bridge.md`
- [x] 4.2 Merge main, full gate: fmt, clippy, test, java-build, JVM tests
