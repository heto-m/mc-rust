## MODIFIED Requirements

### Requirement: Slot-ordered submission
Java command buffers SHALL be tagged with a slot and frame index, and run on the queue in Java's order, never ahead of queued work of earlier frames. Work Java records ahead SHALL stay queued until its frame is composed. Immediate work between Java frames SHALL run before mcr's frame work. A frame's GUI work SHALL run after mcr's world passes and before mcr's native overlays. A fence wait on work that is still queued SHALL NOT block Java.

#### Scenario: Texture upload before use
- **WHEN** Java uploads a texture during a resource reload and draws it in the next GUI slot
- **THEN** the upload runs before the GUI draw

#### Scenario: Next frame recorded during compose
- **WHEN** Java records frame N+1 while mcr submits frame N's world slots
- **THEN** none of frame N+1's work is submitted in that mcr frame

### Requirement: Frame profile
Java SHALL publish, with every frame, the time spent on input and packets, ticks, extraction, world rendering, GUI rendering, the whole render frame and the frame in total. mcr SHALL measure its wait for Java and its submits of Java's work in each mcr frame. While `MCR_MOD_PROFILE` is set, mcr SHALL also measure the Rust time of Java's GPU work by kind, and it SHALL log the per-frame means once a second.

#### Scenario: Profile line
- **WHEN** mcr runs in mod mode with `MCR_MOD_PROFILE=1`
- **THEN** a line with fps, wait, submit, the Java phase means, the Rust time in downcalls and its split by kind is logged every second

## ADDED Requirements

### Requirement: One submit per frame
In mod mode, mcr SHALL put its own command buffers and Java's queued work of one mcr frame on the queue with one submit, in slot order, unless a host write must follow batched work. Bytes Java stages for one submit SHALL NOT overwrite bytes staged earlier in the same submit. A Java command buffer that failed to record SHALL NOT reach the queue. After a rejected batch, Java's work SHALL be submitted on its own.

#### Scenario: Title screen
- **WHEN** mcr renders the title screen with 29 mods, unpaced
- **THEN** each mcr frame makes one `queue.submit`, and the screen matches the golden composition tests

#### Scenario: Two splits with uploads
- **WHEN** Java stages uniform writes before and after a slot split within one submit, and both are batched
- **THEN** each split's draws read their own bytes
