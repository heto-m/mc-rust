# Proposal

## Why

Mod compatibility design B (docs/mod-bridge.md) runs the stock client and Fabric inside the `mcr` process. That
first needs a JVM embedded in Rust with fast, measured call paths in both directions. Every later `mod-*` change
builds on this host, so its threading, memory sharing and shutdown rules must be fixed and benchmarked first.

## What Changes

- New crate `mcr-jvm` (no Minecraft knowledge):
  - JDK 25 location (`MCR_JAVA_HOME`, `.cache/tools/jdk25`, `JAVA_HOME`, `java.home` of `java` on PATH).
  - `libjvm` loaded at runtime with `libloading`.
  - `JNI_CreateJavaVM` on a dedicated thread with a 16 MiB stack.
  - The boot call `McrBoot.main(long, String[])`, and `exit`/`abort` hooks.
- A versioned `#[repr(C)]` native function table handed to Java. Java builds FFM downcall handles from it and
  registers upcall stubs back through it.
- Shared-memory primitives: SPSC byte rings over Rust-owned memory, viewed from Java as `MemorySegment`, and a
  doorbell (blocking wait downcall plus notify).
- A Java source tree `java/mcr-boot/` (our own code) and `cargo xtask java-build`, which compiles it with the
  JDK's `javac`/`jar` into `.cache/java/`. No Gradle.
- A hello-world round trip, run as a test: Java calls Rust, Rust calls back through an upcall stub, and the
  two exchange 1 MiB through a ring.
- Benchmarks of the four call paths, with results recorded in design.md.

Out of scope: Fabric, Minecraft classes, mod loading, any rendering/network/input bridging, Windows/macOS CI
(code stays portable, only Linux is verified), JVM restart (impossible in-process), log routing beyond stdout.

## Capabilities

### New Capabilities
- `jvm-host`: locating, starting and stopping an embedded JVM, the native table ABI, upcall registration,
  shared rings and doorbells, and threading rules.

### Modified Capabilities
- `dev-tooling`: adds `java-build`.

## Impact

- New crate `crates/mcr-jvm` (deps: `jni-sys`, `libloading`, `thiserror`; dev: `criterion`).
- New dir `java/mcr-boot/` (Java 25 sources, our code only). `xtask` gains `java.rs`.
- Tests that start a JVM are `#[ignore]`d unless `MCR_JVM_TESTS=1`, cap the heap at `-Xmx256m`, and run one at a
  time (machine-limits).
