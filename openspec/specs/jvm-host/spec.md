# jvm-host Specification

## Purpose
Embeds a HotSpot JVM (Java 25+) in the `mcr` process through `mcr-jvm`. Covers locating and loading `libjvm`, the
dedicated `mcr-java` thread, the versioned native function table and upcall rules, the shared rings and doorbells
in Rust-owned memory, orderly shutdown, and the measured cost of each call path. It holds no Minecraft knowledge.

## Requirements

### Requirement: Runtime JVM loading
The host SHALL load `libjvm` at runtime from a Java 25+ home resolved in the order `MCR_JAVA_HOME`, `.cache/tools/jdk25`, `JAVA_HOME`, `java.home` of `java` on PATH, and SHALL NOT link against `libjvm` at build time. A home whose `release` file reports a major version below 25 SHALL be rejected with an error naming the path.

#### Scenario: Old JDK rejected
- **WHEN** the only Java home found reports `JAVA_VERSION="21.0.4"`
- **THEN** starting the host fails with an error that names that home and the required version 25

#### Scenario: Override wins
- **WHEN** `MCR_JAVA_HOME` points at a valid JDK 25 and `JAVA_HOME` points at another
- **THEN** the `MCR_JAVA_HOME` JDK is used

### Requirement: Dedicated JVM thread
The host SHALL create the JVM on a dedicated thread named `mcr-java` with at least a 16 MiB stack, SHALL make the boot call on that thread, and SHALL refuse a second JVM start in the same process.

#### Scenario: Second start
- **WHEN** a JVM has already been started and destroyed in this process
- **THEN** a new start returns an error instead of calling `JNI_CreateJavaVM`

### Requirement: Native table ABI
Rust SHALL pass Java a versioned table of native function pointers (magic, version, size, count, entries) and Java SHALL refuse to boot on a magic or version mismatch. Entries SHALL be append-only, and the Rust and Java entry lists SHALL be checked equal by a test.

#### Scenario: Version mismatch
- **WHEN** Java boot code compiled for table version 2 receives a version 1 table
- **THEN** boot fails with a message naming both versions and no downcall is made

### Requirement: Upcall policy
Java SHALL register its upcall stubs once at boot. Rust SHALL invoke them only on the `mcr-java` thread while that thread is inside a downcall. Each upcall target SHALL catch all throwables and report them as a status code.

#### Scenario: Java exception in upcall
- **WHEN** an upcall target throws a runtime exception
- **THEN** Rust receives a failure status and the VM keeps running

### Requirement: Shared rings and doorbells
The host SHALL provide single-producer single-consumer byte rings in Rust-owned memory that Java accesses as `MemorySegment`s without copying, plus doorbells whose Java-side wait is a non-critical downcall so the waiting thread never delays a safepoint.

#### Scenario: Ring round trip
- **WHEN** Rust writes 1 MiB of messages of random sizes into a ring and Java echoes them through a second ring
- **THEN** Rust reads back identical bytes in order

### Requirement: Orderly shutdown
`System.exit` and the VM abort path SHALL be intercepted through JNI hooks and turned into an mcr shutdown request. After the boot call returns, the host SHALL call `DestroyJavaVM` and SHALL not block process exit for more than 10 seconds, or 120 seconds while `DestroyJavaVM` writes an AOT recording.

#### Scenario: Java calls System.exit
- **WHEN** Java code calls `System.exit(3)`
- **THEN** the host records exit code 3 and signals shutdown to the Rust side

#### Scenario: Recording session quits
- **WHEN** a session that records an AOT cache quits and `DestroyJavaVM` takes 5 seconds to write it
- **THEN** the shutdown is reported clean and the recording is kept

### Requirement: Measured call costs
The change SHALL ship benchmarks for critical and normal FFM downcalls, FFM upcalls on the JVM thread, and JNI static calls from Rust, and SHALL record the measured costs in its design.

#### Scenario: Benchmarks run
- **WHEN** the benchmark task runs with `MCR_JVM_TESTS=1`
- **THEN** it reports nanoseconds per call for all four paths

### Requirement: Quit on SIGTERM
On Linux, `SIGTERM` SHALL make a running mcr quit through its normal shutdown, so that Java's AOT recording, class list and preload record are written as on a user quit.

#### Scenario: Terminated session
- **WHEN** a windowed mod-mode session receives `SIGTERM`
- **THEN** mcr quits cleanly, `DestroyJavaVM` runs, and the next start does not count a preload strike
