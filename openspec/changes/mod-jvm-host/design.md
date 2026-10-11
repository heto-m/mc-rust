## Context

Design B (docs/mod-bridge.md): one process, one JVM, Rust owns the main thread and the window. Java 25 gives us
FFM (`java.lang.foreign`, final since 22), which is faster and safer than JNI for steady-state calls. JNI is
needed only for the invocation API (`JNI_CreateJavaVM`) and the first call into Java.

## Decisions

**Loading the JVM.** Do not link `libjvm` at build time. `mcr-jvm` resolves `lib/server/libjvm.so` (Linux),
`lib/server/libjvm.dylib` (macOS) or `bin/server/jvm.dll` (Windows) under the chosen Java home. It loads it
with `libloading` and resolves `JNI_CreateJavaVM` and `JNI_GetCreatedJavaVMs`. Raw `jni-sys` types are enough
for the handful of calls. Java home order: `MCR_JAVA_HOME`, `.cache/tools/jdk25` (from the existing `xtask jdk`),
`JAVA_HOME`, then `java -XshowSettings:properties -version` → `java.home`. Major version is checked from `release`
(`JAVA_VERSION="25…"`).

**Boot thread.** `JNI_CreateJavaVM` runs on a spawned thread `mcr-java` with a 16 MiB stack. HotSpot discourages the
primordial thread (stack guard pages), and Minecraft's render thread recurses deeply. The same thread then calls
`McrBoot.main(long tableAddr, String[] args)` via `FindClass`/`GetStaticMethodID`/`CallStaticVoidMethod`. That is
the only JNI call. When it returns, the thread calls `DestroyJavaVM` with a 10 s watchdog. If the watchdog fires,
the host reports a leak and lets process exit proceed. A second `JavaVm::start` in the same process is an error
(HotSpot cannot re-create a VM).

**VM options.** `JavaVMInitArgs` version `JNI_VERSION_21`, `ignoreUnrecognized = false`. Base options:
- `-Djava.class.path=…`
- `--enable-native-access=ALL-UNNAMED`: needed for `Linker.downcallHandle`, `upcallStub` and
  `MemorySegment.reinterpret`, and silences JEP 472 warnings for LWJGL.
- `-Xrs`: SIGINT and SIGTERM stay ours.
- `-XX:+UseZGC`: generational by default in 25, sub-millisecond pauses.
- `-XX:+UseCompactObjectHeaders`.
- `-Xss` is not used, because the thread stack is ours.
- `-Xmx` comes from the caller (default 2G; tests 256m). No `AlwaysPreTouch` (RAM budget).

Extra options (`-D…`) are caller-supplied. The JNI `exit` hook is set to an `extern "C"` function that records the
code and starts mcr's orderly shutdown; it does not return to the VM until Rust decides. `abort` logs and aborts.
Signals: the JVM installs its own SIGSEGV/SIGBUS handlers (implicit null checks, safepoint polls). Rust's
stack-overflow handler is superseded, so a Rust stack overflow becomes an `hs_err` crash. That is accepted and
documented.

**Native table ABI.** Rust passes the address of `McrNativeTable { magic: u32 = 'MCRB', version: u32, size: u32,
count: u32, fns: [*const c_void; N] }`. Java maps it with `MemorySegment.ofAddress(addr).reinterpret(size)` and
builds `MethodHandle`s with `Linker.nativeLinker().downcallHandle(MemorySegment.ofAddress(fn), FunctionDescriptor)`.
Function pointers avoid exporting symbols from the executable, which needs `-rdynamic` on Linux and a `.def` on
Windows. Indices are append-only. Java checks `version` and fails fast. Java and Rust each keep one generated
list of entries (name, index, signature). A unit test compares the two lists.

**Upcalls.** At boot, Java creates `Linker.upcallStub(handle, descriptor, Arena.global())` for each Rust→Java entry
and passes the stub addresses back through the `register_upcalls(ptr, count)` downcall. Rules:
- Upcalls are only invoked on the `mcr-java` thread, inside a downcall. Other threads would be attached and
  detached per call, which costs microseconds and changes GC root scanning.
- Every Java upcall target wraps its body in `try/catch(Throwable)` and returns a status code. An exception
  escaping an upcall terminates the VM.
- All other Rust→Java traffic is ring messages.

**Rings and doorbells.**
- `Ring` is an SPSC byte ring: a power-of-two capacity and 64-byte-separated head/tail atomics, in memory
  allocated and owned by Rust.
- Messages are `[u32 len][u16 kind][payload]` and never wrap. A padding record skips to the start.
- Java reads and writes through a `MemorySegment` with `VarHandle` acquire/release on head and tail. Payload
  views are slices, not copies.
- `Doorbell` is a futex word (Linux `futex`, macOS `os_sync_wait_on_address`, Windows `WaitOnAddress`) behind
  `wait(timeout)` and `notify()`. Java waits through a normal (non-critical) downcall, so the thread is "in
  native" and never delays a safepoint.

**Downcall flavours.** Use `Linker.Option.critical(false)` for calls that are short (under about 1 µs), never
block and never call back. `critical(true)` (heap access) is allowed only for passing small `byte[]`/`int[]`
without copying. Everything that may block or upcall is a normal downcall.

**Expected costs (to be replaced by measurements).**

| Path | Expected | Notes |
|---|---|---|
| FFM downcall, critical | 2–5 ns | no thread-state transition |
| FFM downcall, normal | 8–20 ns | in-native transition, safepoint poll on return |
| FFM upcall on an attached thread | 20–60 ns | stub plus Java→native transitions |
| JNI `CallStaticVoidMethod` from Rust | 50–150 ns | method lookup cached, local frame |
| Upcall from an unattached thread | microseconds | forbidden by policy |

These numbers set the API shape. Per-draw or per-packet work batches into rings, and a call is a doorbell or a
batch boundary.

**Hello-world.** `java/mcr-boot/src/dev/mcr/boot/McrBoot.java` and `Native.java`, plus a test-only
`dev/mcr/boot/selftest/RoundTrip.java`, sum a Rust-filled ring, echo it back, call an upcall that increments a
Rust counter, and run the Java-side micro-benchmark loops (warm-up, then 10^7 calls, `System.nanoTime`). They
print results as one line that the Rust test parses.

**Java build.** `cargo xtask java-build` runs `javac --release 25 -d … @sources` and `jar --create` per module into
`.cache/java/<module>.jar`, and records a content hash so it is incremental. There is no dependency on Mojang
jars here. Later modules add `-cp` entries from `.cache/`.

## Risks

- HotSpot signal handling versus Rust: covered by the documented crash behaviour. Using `libjsig` is a later
  option if chaining is ever needed.
- macOS: JNI on a non-main thread is fine. SDL/Cocoa concerns come later (`mod-fabric-boot`).
- Upcall stub leaks: they live in `Arena.global()` and are created once at boot.

## Benchmark results

To be filled by task 4.2.
