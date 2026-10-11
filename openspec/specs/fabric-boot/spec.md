# fabric-boot Specification

## Purpose
Boots the stock client under Fabric Loader inside mcr's embedded JVM. The bridge mod keeps vanilla headless
(no window, GPU, audio device or sockets), Rust paces its frames, and lifecycle, crashes and `System.exit`
are reported to Rust so the native client survives them.

## Requirements

### Requirement: Fabric launch inside mcr
`mcr-modbridge` SHALL start the stock `client.jar` through Fabric Loader's Knot client entrypoint inside the embedded JVM, with mods supplied only through `fabric.addMods`, the boot jar as a Fabric system library, the shared launcher-layout asset store as `--assetsDir`, and `--offlineDeveloperMode`.

#### Scenario: Fabric API and Mod Menu boot
- **WHEN** the bridge boots with Fabric API 0.162.0+26.3 and Mod Menu 21.0.0
- **THEN** both mods are reported loaded, their client initializers run, and Java reports `TitleScreen` as the current screen

### Requirement: Bridge native table
The bridge's Java→Rust entries SHALL live in a second, embedder-owned native table with the host table's header layout, its own magic and its own version, handed to Java as the `mcr.bridge.table` system property. Java SHALL refuse a table whose magic, version or entry count does not match. Rust SHALL reach Java only through rings and upcall stubs that Java registers, never by looking up Minecraft or mod classes by name.

#### Scenario: Class loaders
- **WHEN** the bridge mod initialises
- **THEN** it reports that it was loaded by Knot's class loader and that the boot classes it calls were loaded once, by the application class loader

### Requirement: Headless vanilla
The bridge mod SHALL prevent vanilla from creating an OS window, a GL or Vulkan device, an OpenAL device, a native message box or a network socket. SDL3 SHALL run on its offscreen video driver and the GPU backend SHALL be the bridge's own backend. Java SHALL NOT resolve remote host names: Rust owns DNS and every socket.

#### Scenario: Nothing external opened
- **WHEN** Java has reached the title screen and idled for three seconds
- **THEN** no OS-visible window exists, no OpenAL device was opened and the process holds no TCP socket created by Java

### Requirement: Frame pacing
Java's main loop SHALL wait at the start of each `Minecraft.runTick` for a frame message from Rust, and the wait SHALL be a non-critical downcall. Queued frames SHALL collapse into the newest. Vanilla's framerate limiter SHALL be disabled.

#### Scenario: Rust stops posting frames
- **WHEN** Rust posts no frame for one second
- **THEN** the Java render thread stays blocked in the wait and uses no CPU

### Requirement: Lifecycle reporting
The bridge SHALL report loaded mods (id and version), every screen change (class name) and crashes (crash report path) to Rust. On a crash it SHALL park the Java thread instead of exiting the process, and it SHALL never close the process's standard output.

#### Scenario: Mod crash
- **WHEN** a client mod initializer throws during boot
- **THEN** Rust receives a crash report path, the `mcr` process keeps running and its standard output still works

#### Scenario: System.exit from a mod
- **WHEN** a client mod initializer calls `System.exit`
- **THEN** Rust reports the VM as exited with that code, treats the bridge as dead and tells the caller to end the process

### Requirement: Orderly stop
On a shutdown message, Java SHALL stop the game loop and close the client, and the JVM SHALL be destroyed within 10 seconds.

#### Scenario: Shutdown at title screen
- **WHEN** Rust requests shutdown while Java shows the title screen
- **THEN** the boot call returns and the JVM is destroyed in under 10 seconds
