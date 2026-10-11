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
(`JAVA_VERSION="25…"`). An invalid `MCR_JAVA_HOME` is an error, because an explicit override must not silently fall
back. The other candidates are skipped when missing or too old, and the error names every rejected home.
`.cache/tools/jdk25` is resolved against a cache dir the embedder passes (`HomeSearch::from_env`). Tests and
benches use `dev_cache_dir()`, the repository `.cache`.

**Boot thread.** `JNI_CreateJavaVM` runs on a spawned thread `mcr-java` with a 16 MiB stack. HotSpot discourages the
primordial thread (stack guard pages), and Minecraft's render thread recurses deeply. The same thread then calls
`McrBoot.main(long tableAddr, String[] args)` via `FindClass`/`GetStaticMethodID`/`CallStaticVoidMethod`. That is
the only JNI call. When it returns, the thread calls `DestroyJavaVM` with a 10 s watchdog. If the watchdog fires,
the host reports a leak and lets process exit proceed. The watchdog lives in `JavaVm::join`, which waits at most
`DESTROY_TIMEOUT` after the boot call returns. `libjvm` is never unloaded. A failed library load or option check releases the single-start latch,
so the start can be retried; once `JNI_CreateJavaVM` has been called, the latch stays set.
`JNI_GetCreatedJavaVMs` also refuses a start when another component already created a VM. A second `JavaVm::start` in the same process is an error
(HotSpot cannot re-create a VM).

**VM options.** `JavaVMInitArgs` version `JNI_VERSION_21`, `ignoreUnrecognized = false`. Base options:
- `-Djava.class.path=…`
- `--enable-native-access=ALL-UNNAMED`: needed for `Linker.downcallHandle`, `upcallStub` and
  `MemorySegment.reinterpret`, and silences JEP 472 warnings for LWJGL.
- `-Xrs`: SIGINT and SIGTERM stay ours.
- `-XX:+UseZGC`: generational by default in 25, sub-millisecond pauses.
- `-XX:+UseCompactObjectHeaders`.
- `-Xss` is not used, because the thread stack is ours.
- `-Xmx` comes from the caller (default 2G; tests 128m). No `AlwaysPreTouch` (RAM budget).
- `-XX:-UsePerfData` (added during implementation): no `hsperfdata` mmap file in `/tmp`, and no perf-memory
  `atexit` cleanup running while the exit hook holds the VM thread.

Extra options (`-D…`) are caller-supplied. The JNI `exit` hook is set to an `extern "C"` function that records the
code and starts mcr's orderly shutdown; it does not return to the VM until Rust decides. `abort` logs and aborts.
HotSpot calls the exit hook from `VM_Exit` on the VM thread at a safepoint, so every Java thread is frozen while it
blocks: `JavaVm::join` returns `VmOutcome::Exited { code }`, and Rust either exits the process itself or calls
`release_exit()` so HotSpot terminates with that code. A VM that took the exit path can never be destroyed cleanly.
Signals: the JVM installs its own SIGSEGV/SIGBUS handlers (implicit null checks, safepoint polls). Rust's
stack-overflow handler is superseded, so a Rust stack overflow becomes an `hs_err` crash. That is accepted and
documented.

**Native table ABI.** Rust passes the address of `McrNativeTable { magic: u32 = 'MCRB', version: u32, size: u32,
count: u32, fns: [*const c_void; N] }`. Java maps it with `MemorySegment.ofAddress(addr).reinterpret(size)` and
builds `MethodHandle`s with `Linker.nativeLinker().downcallHandle(MemorySegment.ofAddress(fn), FunctionDescriptor)`.
Function pointers avoid exporting symbols from the executable, which needs `-rdynamic` on Linux and a `.def` on
Windows. Indices are append-only. Java checks `version` and fails fast. Java and Rust each keep one list of entries
(name, index, signature, flavour): a `native_entries!` macro in `table.rs` builds both the descriptor list and the
static table from one source, and `Abi.java` holds the Java enum. They are hand-maintained, not generated; Rust unit
tests parse `Abi.java` and compare the entry list, the upcall list, the header constants and the ring constants.

Version 1 entries: `register_upcalls (JI)I`, `host_call (IJ)J`, `doorbell_wait (JIJ)I`, `doorbell_notify (J)V`
(critical), `nop_critical (J)J` (critical), `nop_normal (J)J`. Upcalls: `selftest_ping ()I`, `selftest_add (JJ)I`,
`selftest_throw ()I`. `host_call(op, arg)` dispatches to an optional embedder closure (`JvmConfig::host_call`) on the
calling thread inside a downcall, so it may make upcalls. It is for rare control operations (the self-test and the
benchmark use it), not data. A re-entrant call returns `HOST_CALL_UNAVAILABLE`, and a panic returns
`HOST_CALL_PANICKED` instead of unwinding into Java. Embedder-specific entries (the bridge's) are not in this
table yet; see Risks.

**Upcalls.** At boot, Java creates `Linker.upcallStub(handle, descriptor, Arena.global())` for each Rust→Java entry
and passes the stub addresses back through the `register_upcalls(ptr, count)` downcall. Rules:
- Upcalls are only invoked on the `mcr-java` thread, inside a downcall. Other threads would be attached and
  detached per call, which costs microseconds and changes GC root scanning. Rust enforces it: the typed upcall
  wrappers return `UpcallError::WrongContext` unless a thread-local marks the `mcr-java` thread and a
  `host_call` downcall is on the stack.
- Every Java upcall target wraps its body in `try/catch(Throwable)` and returns a status code. An exception
  escaping an upcall terminates the VM.
- All other Rust→Java traffic is ring messages.

**Rings and doorbells.**
- `Ring` is an SPSC byte ring: a power-of-two capacity and 64-byte-separated head/tail atomics, in memory
  allocated and owned by Rust.
- Messages are `[u32 len][u16 kind][u16 0][payload]`, padded to 8 bytes, and never wrap. A padding record (kind
  `0xFFFF`) skips to the start. The reserved `u16` (a deviation from the original 6-byte header) keeps payloads
  8-aligned for typed `MemorySegment` access. Layout: `magic 'MCRR' u32 | capacity u32` at 0, write position `u64`
  at 64, read position `u64` at 128, data at 192. The maximum payload is `capacity / 2 - 8`, which guarantees that
  any record fits once the ring drains. The reader validates every length before use, since the other side is not
  trusted.
- Java reads and writes through a `MemorySegment` with `VarHandle` acquire/release on head and tail. Payload
  views are slices, not copies.
- `Doorbell` is a sequence word behind `wait(seen, timeout)` and `notify()`, with a waiter count so `notify` makes no
  syscall when nobody sleeps. Linux uses `futex` (private). Other platforms use a `Mutex` + `Condvar` fallback
  instead of `os_sync_wait_on_address`/`WaitOnAddress`. Those platforms are unverified here, and the fallback
  is obviously correct; switch when they are measured. Java reads the word directly and waits through a normal
  (non-critical) downcall, so the thread is "in native" and never delays a safepoint.

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
print results as one line. The JVM shares the test process's stdout, so the line also goes back to Rust as a
`KIND_RESULT` ring message, which the test parses. Ring addresses and the Rust-side loops (upcall and JNI cost)
travel through `host_call` ops.

The opt-in test (`MCR_JVM_TESTS=1 cargo test -p mcr-jvm --test jvm_round_trip`) is one test because HotSpot allows
one VM per process. It covers boot, the 1 MiB echo, upcalls, an upcall exception, the version-mismatch check,
call costs, a clean `DestroyJavaVM` and the refused second start. `System.exit` freezes the VM for good, so the
exit-hook path runs in a child re-exec of the same test binary (`MCR_JVM_CHILD=exit`) after the parent's VM is
gone. The child asserts that it saw exit code 3 and then exits with it.

**JNI lookups from a downcall.** `FindClass` inside an FFM downcall resolves against the boot loader, because the
nearest Java frame is a java.base stub. `StaticVoidMethod::lookup` (diagnostics only) therefore loads application
classes through `ClassLoader.getSystemClassLoader()`. The boot call itself runs with no Java frames, so `FindClass`
uses the system loader there.

**Java build.** `cargo xtask java-build` runs `javac --release 25 -d … @sources` and `jar --create` per module into
`.cache/java/<module>.jar`, and records a content hash so it is incremental. There is no dependency on Mojang
jars here. Later modules add `-cp` entries from `.cache/`.

## Risks

- HotSpot signal handling versus Rust: covered by the documented crash behaviour. Using `libjsig` is a later
  option if chaining is ever needed.
- macOS: JNI on a non-main thread is fine. SDL/Cocoa concerns come later (`mod-fabric-boot`).
- Upcall stub leaks: they live in `Arena.global()` and are created once at boot.
- The table is a static in `mcr-jvm` and holds only host entries. `mod-fabric-boot` needs bridge entries
  (`awaitFrame`, …) that live in another crate. It must either let the embedder append entries (table built at
  start, Java checking an extension version) or hand Java a second table through `host_call`. Decide before
  adding the first bridge entry.
- Fabric's Knot loader is not the system loader. Any Rust-side JNI lookup of mod or Minecraft classes has to go
  through Knot's loader that Java hands over, not `FindClass` or the system loader.

## Benchmark results

AMD Ryzen 7 5800X, Linux 7.2, Temurin 25.0.4.1+1, `-XX:+UseZGC -XX:+UseCompactObjectHeaders -Xmx128m`.
The Java loops make 10^7 calls after 2·10^6 warm-up calls. The Rust numbers come from criterion
(`MCR_JVM_TESTS=1 cargo bench -p mcr-jvm --bench jvm_calls`, release), which runs on `mcr-java` inside a
`host_call` downcall. The "test" column is the opt-in test's 10^6-call loop at `opt-level = 1`.

| Path | Expected | Measured | Test loop |
|---|---|---|---|
| FFM downcall, critical (`nop_critical(long)`) | 2–5 ns | **3.2 ns** | 3.2 ns |
| FFM downcall, normal (`nop_normal(long)`) | 8–20 ns | **3.9 ns** | 3.9 ns |
| FFM upcall from Rust on `mcr-java` (`selftest_ping`) | 20–60 ns | **19.7 ns** | 22.6 ns |
| JNI `CallStaticVoidMethodA` from Rust (cached IDs) | 50–150 ns | **54.7 ns** | 56.6 ns |

Upcalls and JNI land at the low end of the estimates. Normal downcalls are far cheaper than estimated, only about
0.7 ns above critical ones, so `critical` buys almost nothing for ordinary calls. Use it only where heap access
without copying matters. A downcall per message would still be affordable, but the ring and batching shape stays:
the cost that dominates is the cross-thread handoff (doorbell wake-ups), not the call. The 1 MiB echo through two
64 KiB rings, including JVM boot and teardown, finishes in under 0.5 s.
