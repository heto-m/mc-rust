## MODIFIED Requirements

### Requirement: Frame lockstep
Each mcr frame in mod mode SHALL post at most one frame message to Java, and at most one SHALL be outstanding. Each finished Java frame SHALL be composed at most once, in the mcr frame that posted it or in a later one. A frame is late after 100 ms once Java has loaded, or 50 ms while it loads; mcr SHALL then present without it and compose it later. The mcr render thread SHALL never call into Java, and it SHALL only wait, bounded, for a signal from the Java thread.

#### Scenario: Java frame per mcr frame
- **WHEN** mcr renders 600 paced frames in mod mode in lockstep
- **THEN** Java completes exactly 600 frames with matching indices

#### Scenario: Frames ahead
- **WHEN** mcr renders 600 unpaced frames one frame ahead
- **THEN** Java completes at least 599 of them, and each completed frame is composed exactly once

### Requirement: Slot-ordered submission
Java command buffers SHALL be tagged with a slot and frame index, and submitted on the render thread in Java's order, never ahead of queued work of earlier frames. Work Java records ahead SHALL stay queued until its frame is composed. Immediate work between Java frames SHALL be submitted before mcr's frame work. A frame's GUI work SHALL be submitted after mcr's world passes and before mcr's native overlays. A fence wait on work that is still queued SHALL NOT block Java.

#### Scenario: Texture upload before use
- **WHEN** Java uploads a texture during a resource reload and draws it in the next GUI slot
- **THEN** the upload is submitted before the GUI draw

#### Scenario: Next frame recorded during compose
- **WHEN** Java records frame N+1 while mcr submits frame N's world slots
- **THEN** none of frame N+1's work is submitted in that mcr frame

## ADDED Requirements

### Requirement: Frame pipelining
In ahead mode, mcr SHALL post Java's next frame as soon as Java has finished the current one, and SHALL compose each frame in the mcr frame after the one that posted it. In lockstep mode, mcr SHALL post each frame at the start of an mcr frame and compose it in that frame. Ticks, slot order, hang detection and bounded waits SHALL behave the same in both modes.

#### Scenario: Uncapped title screen
- **WHEN** mcr renders the title screen with 29 mods, unpaced, in ahead mode
- **THEN** every composed frame is one frame ahead, and the render thread's mean wait for Java is under 5% of the frame time

#### Scenario: Ticks while ahead
- **WHEN** a session runs one frame ahead for 60 seconds with variable frame times
- **THEN** Java's client tick count equals Rust's tick count

### Requirement: Pipelining mode
The default automatic mode SHALL run ahead while mcr's frame rate is uncapped (no vsync and no fps limit), or while Java's mean frame time is above 8 ms. It SHALL use lockstep while the rate is capped and Java's mean frame time is below 4 ms. Between those times it SHALL keep its current mode. `MCR_MOD_PIPELINE` set to `lockstep`, `ahead` or `auto` SHALL select the mode.

#### Scenario: Vsync
- **WHEN** vsync is on and Java's frames take 2 ms
- **THEN** mcr composes each Java frame in the mcr frame that posted it

#### Scenario: Slow Java under vsync
- **WHEN** vsync is on and Java's frames take 12 ms
- **THEN** mcr runs Java one frame ahead

### Requirement: Late frame camera
When mcr composes a Java frame in a later mcr frame than the one that posted it, mcr SHALL rewrite that frame's level projection before its first world-slot work runs. Geometry that Java placed relative to its camera SHALL then project where mcr's current camera puts the same world position. A frame composed in the mcr frame that posted it SHALL keep Java's projection.

#### Scenario: Turning camera
- **WHEN** the camera turns between the mcr frame that posted a Java frame and the mcr frame that composes it
- **THEN** a world point on an entity lands on the same pixel as the same point on mcr's terrain, within 1e-4 in normalized device coordinates

### Requirement: Frame profile
Java SHALL publish, with every frame, the time spent on input and packets, ticks, extraction, world rendering, GUI rendering, the whole render frame and the frame in total. mcr SHALL measure its wait for Java and its submits of Java's work in each mcr frame. While `MCR_MOD_PROFILE` is set, mcr SHALL also measure the time Java's GPU downcalls spend in Rust, and it SHALL log the per-frame means once a second.

#### Scenario: Profile line
- **WHEN** mcr runs in mod mode with `MCR_MOD_PROFILE=1`
- **THEN** a line with fps, wait, submit, the Java phase means and the Rust time in downcalls is logged every second

### Requirement: Per-mod profile
While `MCR_MOD_PROFILE` is set to a value other than `frame`, Java SHALL sample its render thread and log each mod's share of the samples every 10 seconds. A sample SHALL count for the innermost mod frame on its stack, including mixin handlers. A sample with no mod frame SHALL count for Minecraft. Samples taken while the thread waits for a frame SHALL NOT count.

#### Scenario: Per-mod share
- **WHEN** a HUD mod draws every frame and `MCR_MOD_PROFILE=1` is set
- **THEN** within 10 seconds Java logs that mod's share of the render thread
