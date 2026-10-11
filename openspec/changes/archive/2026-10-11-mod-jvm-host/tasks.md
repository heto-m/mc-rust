## 1. Tooling
- [x] 1.1 `xtask java-build`: discover modules under `java/`, compile them with the JDK's `javac --release 25`, jar them into `.cache/java/`, and skip unchanged modules by source hash
- [x] 1.2 Unit test: module discovery and hash-based skipping, using a fake javac path

## 2. Host
- [x] 2.1 Scaffold `mcr-jvm` (errors with `thiserror`, workspace wiring)
- [x] 2.2 Java home location and `release` version check, with unit tests on fixture directories
- [x] 2.3 `libjvm` loading and `JNI_CreateJavaVM` on the `mcr-java` thread, VM options builder, and the `exit`/`abort` hooks
- [x] 2.4 Boot call `McrBoot.main(long, String[])`, `DestroyJavaVM` with watchdog, and an error on a second start

## 3. Bridge plumbing
- [x] 3.1 `McrNativeTable` ABI, shared entry list, and a unit test that the Rust and Java lists match
- [x] 3.2 Java `Native` class: table mapping, downcall handles, upcall stub registration
- [x] 3.3 SPSC `Ring` and `Doorbell` (Rust side, with unit tests under loom-style stress on two Rust threads)
- [x] 3.4 Java `Ring` reader and writer over `MemorySegment`

## 4. Verification
- [x] 4.1 Round-trip test (`MCR_JVM_TESTS=1`): boot the JVM, ring echo of 1 MiB, upcall counter, exit-hook path, clean `DestroyJavaVM`
- [x] 4.2 Benchmarks: a criterion bench for upcall cost from Rust plus the Java-side loop for downcall (critical and normal) and JNI call cost; record results in design.md
- [x] 4.3 fmt, clippy, tests, archive
