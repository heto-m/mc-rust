## 1. Measure

- [x] 1.1 Baseline title time with and without the AOT cache on the curated mod set
- [x] 1.2 Baseline and preload join runs (`--connect … --bench-timeline`), first-frame hitches

## 2. Build

- [x] 2.1 `mcr_jvm::preload`: plan, log option, log parsing with markers, merge, list format, strikes, eviction, unit tests
- [x] 2.2 AOT eviction leaves class lists alone
- [x] 2.3 Bridge: preload plan next to the AOT plan, list merged at shutdown, strikes from `preload_stopped`
- [x] 2.4 Java: `McrPreload`, milestones (`Main.main`, game loop, title screen, login), exclusions, lock order
- [x] 2.5 App status line; `MCR_JVM_PRELOAD` switch
- [x] 2.6 JVM test `jvm_preload` (`MCR_JVM_TESTS=1`): record, preload, off

## 3. Finish

- [x] 3.1 Numbers before and after in design.md; default decided from them; `docs/mod-bridge.md`
- [x] 3.2 Merge main, full gate: fmt, clippy, test, java-build, JVM tests
