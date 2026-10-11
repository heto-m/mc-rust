## ADDED Requirements

### Requirement: Fabric launch inside mcr
`mcr-modbridge` SHALL start the stock `client.jar` through Fabric Loader's Knot client entrypoint inside the embedded JVM, with mods supplied only through `fabric.addMods`, the shared launcher-layout asset store as `--assetsDir`, and `--offlineDeveloperMode`.

#### Scenario: Fabric API and Mod Menu boot
- **WHEN** the bridge boots with Fabric API 0.162.0+26.3 and Mod Menu 21.0.0
- **THEN** both mods are reported loaded, their client initializers run, and Java reports `TitleScreen` as the current screen

### Requirement: Headless vanilla
The bridge mod SHALL prevent vanilla from creating an OS window, a GL or Vulkan device, an OpenAL device or a network socket. SDL3 SHALL run on its offscreen video driver and the GPU backend SHALL be the bridge's own backend.

#### Scenario: Nothing external opened
- **WHEN** Java has reached the title screen
- **THEN** no OS-visible window exists, no OpenAL device was opened and the process holds no TCP socket created by Java

### Requirement: Frame pacing
Java's main loop SHALL wait at the start of each `Minecraft.runTick` for a frame message from Rust, and the wait SHALL be a non-critical downcall. Vanilla's framerate limiter SHALL be disabled.

#### Scenario: Rust stops posting frames
- **WHEN** Rust posts no frame for one second
- **THEN** the Java render thread stays blocked in the wait and uses no CPU

### Requirement: Lifecycle reporting
The bridge SHALL report loaded mods (id and version), every screen change (class name) and crashes (crash report path) to Rust. On a crash it SHALL park the Java thread instead of exiting the process.

#### Scenario: Mod crash
- **WHEN** a client mod initializer throws during boot
- **THEN** Rust receives a crash report path and the `mcr` process keeps running

### Requirement: Orderly stop
On a shutdown message, Java SHALL stop the game loop and close the client, and the JVM SHALL be destroyed within 10 seconds.

#### Scenario: Shutdown at title screen
- **WHEN** Rust requests shutdown while Java shows the title screen
- **THEN** the boot call returns and the JVM is destroyed in under 10 seconds
