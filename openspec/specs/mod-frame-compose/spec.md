# mod-frame-compose Specification

## Purpose
Java's frames run in lockstep with mcr's: Rust decides every tick, and Java's GUI and HUD are composed into
mcr's frame image between its world passes and its native overlays. A slow or stuck Java frame never freezes mcr;
the bridge detaches from rendering instead.

## Requirements

### Requirement: Frame lockstep
Each mcr frame in mod mode SHALL post at most one frame message to Java, and at most one SHALL be outstanding. Each finished Java frame SHALL be composed at most once, in the mcr frame that posted it or in a later one. A frame is late after 100 ms once Java has loaded, or 50 ms while it loads; mcr SHALL then present without it and compose it later. The mcr render thread SHALL never call into Java, and it SHALL only wait, bounded, for a signal from the Java thread.

#### Scenario: Java frame per mcr frame
- **WHEN** mcr renders 600 paced frames in mod mode in lockstep
- **THEN** Java completes exactly 600 frames with matching indices

#### Scenario: Frames ahead
- **WHEN** mcr renders 600 unpaced frames one frame ahead
- **THEN** Java completes at least 599 of them, and each completed frame is composed exactly once

### Requirement: Tick alignment
Java's client SHALL run exactly the number of ticks Rust's accumulator decides for each frame, with Rust's partial tick.

#### Scenario: Tick counters agree
- **WHEN** a mod-mode session runs for 60 seconds with variable frame times
- **THEN** Java's client tick count equals Rust's tick count

### Requirement: Slot-ordered submission
Java command buffers SHALL be tagged with a slot and frame index, and run on the queue in Java's order, never ahead of queued work of earlier frames. Work Java records ahead SHALL stay queued until its frame is composed. Immediate work between Java frames SHALL run before mcr's frame work. A frame's GUI work SHALL run after mcr's world passes and before mcr's native overlays. A fence wait on work that is still queued SHALL NOT block Java.

#### Scenario: Texture upload before use
- **WHEN** Java uploads a texture during a resource reload and draws it in the next GUI slot
- **THEN** the upload runs before the GUI draw

#### Scenario: Next frame recorded during compose
- **WHEN** Java records frame N+1 while mcr submits frame N's world slots
- **THEN** none of frame N+1's work is submitted in that mcr frame

### Requirement: Virtual main target
Textures created for Java's main render target SHALL be marked virtual. In the GUI slot, a pass into Java's main color SHALL draw into mcr's `Rgba8Unorm` scene after mcr's world passes, with a private depth. Java clears of the virtual color target SHALL be ignored.

#### Scenario: HUD over world
- **WHEN** Java draws its HUD while joined to a server
- **THEN** mcr's world remains visible behind the hotbar, hearts and crosshair

#### Scenario: Menu blur
- **WHEN** Java opens a screen that blurs the background
- **THEN** the blur samples mcr's rendered world

### Requirement: Top-down GUI composition
In the GUI slot, projected geometry SHALL land top-down and scissor rectangles SHALL be flipped to match, so no copy of the scene is made. A copy that Java reads from its main color SHALL see the current scene in Java's row order.

#### Scenario: No flip copies
- **WHEN** a mod-mode frame composes Java's GUI in game
- **THEN** the compositor records no fullscreen copy of the scene

### Requirement: Size follows the window
Window size changes SHALL reach Java's `Window` and trigger its display resize before the next Java frame renders.

#### Scenario: Resize
- **WHEN** the mcr window is resized to 1600×900
- **THEN** Java's next frame renders its GUI at 1600×900

### Requirement: Hang detection
If Java does not finish a frame within 2 seconds, mcr SHALL detach the bridge from rendering, show its native HUD and notify the user, without blocking further frames. After such a stall, mcr SHALL compose Java again once 30 frames in a row each finish within 250 ms, at most three times per session. Java stopping, Java losing its connection or a fourth stall SHALL detach for good.

#### Scenario: Java thread stuck
- **WHEN** a mod blocks the Java render thread indefinitely
- **THEN** mcr keeps rendering natively and shows a bridge-hung notification within 3 seconds
- **AND** no mcr frame waits for Java longer than 100 ms

#### Scenario: Java stalls once
- **WHEN** a mod blocks the Java render thread for 3 seconds and then runs normally
- **THEN** mcr detaches, then composes Java's GUI again and tells the user

### Requirement: Field of view from Java
While Java draws the world, mcr SHALL scale its camera's field of view by Java's computed field of view over vanilla's for the same frame, clamped to 0.02–4, and scale mouse sensitivity down by it when it is below 1.

#### Scenario: Zoom mod
- **WHEN** a mod halves Java's field of view
- **THEN** mcr's terrain is drawn with half its field of view and Java reports a scale of 0.5

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
Java SHALL publish, with every frame, the time spent on input and packets, ticks, extraction, world rendering, GUI rendering, the whole render frame and the frame in total. mcr SHALL measure its wait for Java and its submits of Java's work in each mcr frame. While `MCR_MOD_PROFILE` is set, mcr SHALL also measure the Rust time of Java's GPU work by kind, and it SHALL log the per-frame means once a second.

#### Scenario: Profile line
- **WHEN** mcr runs in mod mode with `MCR_MOD_PROFILE=1`
- **THEN** a line with fps, wait, submit, the Java phase means, the Rust time in downcalls and its split by kind is logged every second

### Requirement: Per-mod profile
While `MCR_MOD_PROFILE` is set to a value other than `frame`, Java SHALL sample its render thread and log each mod's share of the samples every 10 seconds. A sample SHALL count for the innermost mod frame on its stack, including mixin handlers. A sample with no mod frame SHALL count for Minecraft. Samples taken while the thread waits for a frame SHALL NOT count.

#### Scenario: Per-mod share
- **WHEN** a HUD mod draws every frame and `MCR_MOD_PROFILE=1` is set
- **THEN** within 10 seconds Java logs that mod's share of the render thread

### Requirement: One submit per frame
In mod mode, mcr SHALL put its own command buffers and Java's queued work of one mcr frame on the queue with one submit, in slot order, unless a host write must follow batched work. Bytes Java stages for one submit SHALL NOT overwrite bytes staged earlier in the same submit. A Java command buffer that failed to record SHALL NOT reach the queue. After a rejected batch, Java's work SHALL be submitted on its own.

#### Scenario: Title screen
- **WHEN** mcr renders the title screen with 29 mods, unpaced
- **THEN** each mcr frame makes one `queue.submit`, and the screen matches the golden composition tests

#### Scenario: Two splits with uploads
- **WHEN** Java stages uniform writes before and after a slot split within one submit, and both are batched
- **THEN** each split's draws read their own bytes

### Requirement: Turn latency
mcr SHALL record each look the mouse produces with its time, and after each frame is submitted and presented it SHALL match the look its rendered camera shows against the recorded ones. A turn's latency SHALL run from the oldest unshown look that frame is the first to show to the end of that frame. A frame showing a look older than every unshown one SHALL count as stale.

#### Scenario: Stale camera
- **WHEN** a frame renders a camera from before the newest turns, as a camera handed back from an older Java frame does
- **THEN** the turns it does not show wait for the frame that does, their latency includes the stale frames, and the frame counts as stale

### Requirement: Turn latency reports
`--bench` SHALL print the p50, p99 and maximum turn latency with the turn and stale-frame counts and the frames in which Java handed a camera back. Windowed runs SHALL add the last second's turn latency to the once-a-second stats line. `MCR_BENCH_MOUSE=<pixels>` SHALL move the mouse by that many pixels per bench frame through mcr's mouse path, Java's input included.

#### Scenario: Bench through the mouse path
- **WHEN** `mcr --screenshot out.png --bench N` runs in game with `MCR_BENCH_MOUSE=3`
- **THEN** every frame turns the player through mcr's mouse handling, Java receives the motion, and the bench prints the turn latency percentiles, the handoff frames and the stale frames

### Requirement: Loading screens keep the last frame
Java SHALL flag frames drawn under loading, connecting, message and progress screens and mcr's world screen. While Java's frames are flagged, a late Java frame SHALL keep the last composed image as while Java loads, and SHALL not detach the bridge before the loading limit.

#### Scenario: JEI starts on join
- **WHEN** a modded join spends 2.3 seconds in one Java frame under the level loading screen
- **THEN** mcr keeps showing the last composed frame and does not report that mods stopped drawing

### Requirement: Java's encoders finish off its thread
In mod mode, a command encoder Java queues for a later submit SHALL be finished on a worker thread, not on Java's render thread. The render thread SHALL wait for the finished buffer only when it drains that submission. When the worker is unavailable or its queue is full, the encoder SHALL be finished on the calling thread. A buffer whose recording failed SHALL be dropped with a warning and SHALL NOT reach the queue.

#### Scenario: Queued work
- **WHEN** Java queues a frame's encoders while mcr composes the previous frame
- **THEN** Java's frame does not include the encoders' finish, and mcr's batch submits the finished buffers in slot order

#### Scenario: Invalid encoder
- **WHEN** a queued encoder recorded a validation error
- **THEN** its buffer is dropped with a warning and the rest of the batch is submitted

### Requirement: Slow frame profile
While `MCR_MOD_PROFILE` is set and after the first 100 Java frames, mcr SHALL collect the Java frames that took more than twice the running mean, and the bench summary SHALL print their count and their phase means. The `--bench-timeline` summary SHALL print the mean, p50, p99, p99.9 and maximum frame time over its last 31 seconds.

#### Scenario: Spike phases
- **WHEN** a bench runs with `MCR_MOD_PROFILE=frame` and some Java frames take over twice the mean
- **THEN** the summary prints how many there were and which Java phases they spent their time in
