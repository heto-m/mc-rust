## ADDED Requirements

### Requirement: Frame lockstep
Each mcr frame in mod mode SHALL post at most one frame message to Java, with at most one outstanding, and SHALL submit that frame's Java GUI work before presenting unless Java's frame is late. A frame is late after 100 ms once Java has loaded, or after 50 ms while it loads. The mcr render thread SHALL never call into Java and SHALL only wait, bounded, for a signal from the Java thread.

#### Scenario: Java frame per mcr frame
- **WHEN** mcr renders 600 frames in mod mode
- **THEN** Java completes exactly 600 frames with matching indices

### Requirement: Tick alignment
Java's client SHALL run exactly the number of ticks Rust's accumulator decides for each frame, with Rust's partial tick.

#### Scenario: Tick counters agree
- **WHEN** a mod-mode session runs for 60 seconds with variable frame times
- **THEN** Java's client tick count equals Rust's tick count

### Requirement: Slot-ordered submission
Java command buffers SHALL be tagged with a slot and submitted on the render thread in Java's submission order. Immediate work queued between Java frames SHALL be submitted before mcr's frame work. A frame's GUI work SHALL be submitted after mcr's world passes and before mcr's native overlays. A fence wait on work that is still queued SHALL NOT block Java.

#### Scenario: Texture upload before use
- **WHEN** Java uploads a texture during a resource reload and draws it in the next GUI slot
- **THEN** the upload is submitted before the GUI draw

### Requirement: Virtual main target
Textures created for Java's main render target SHALL be marked virtual. Before the GUI slot runs, Java's main color SHALL hold mcr's `Rgba8Unorm` scene, after mcr's world passes and in Java's row order. Afterwards, the result SHALL be copied back into mcr's scene. Java's main depth SHALL stay private to Java. Java clears of the virtual color target SHALL be ignored.

#### Scenario: HUD over world
- **WHEN** Java draws its HUD while joined to a server
- **THEN** mcr's world remains visible behind the hotbar, hearts and crosshair

#### Scenario: Menu blur
- **WHEN** Java opens a screen that blurs the background
- **THEN** the blur samples mcr's rendered world

### Requirement: Size follows the window
Window size changes SHALL reach Java's `Window` and trigger its display resize before the next Java frame renders.

#### Scenario: Resize
- **WHEN** the mcr window is resized to 1600×900
- **THEN** Java's next frame renders its GUI at 1600×900

### Requirement: Hang detection
If Java does not finish a frame within 2 seconds, mcr SHALL detach the bridge from rendering, show its native HUD and notify the user, without blocking further frames.

#### Scenario: Java thread stuck
- **WHEN** a mod blocks the Java render thread indefinitely
- **THEN** mcr keeps rendering natively and shows a bridge-hung notification within 3 seconds
- **AND** no mcr frame waits for Java longer than 100 ms
