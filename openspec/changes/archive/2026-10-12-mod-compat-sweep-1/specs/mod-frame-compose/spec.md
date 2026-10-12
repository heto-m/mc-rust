## MODIFIED Requirements

### Requirement: Hang detection
If Java does not finish a frame within 2 seconds, mcr SHALL detach the bridge from rendering, show its native HUD and notify the user, without blocking further frames. After such a stall, mcr SHALL compose Java again once 30 frames in a row each finish within 250 ms, at most three times per session. Java stopping, Java losing its connection or a fourth stall SHALL detach for good.

#### Scenario: Java thread stuck
- **WHEN** a mod blocks the Java render thread indefinitely
- **THEN** mcr keeps rendering natively and shows a bridge-hung notification within 3 seconds
- **AND** no mcr frame waits for Java longer than 100 ms

#### Scenario: Java stalls once
- **WHEN** a mod blocks the Java render thread for 3 seconds and then runs normally
- **THEN** mcr detaches, then composes Java's GUI again and tells the user

## ADDED Requirements

### Requirement: Field of view from Java
While Java draws the world, mcr SHALL scale its camera's field of view by Java's computed field of view over vanilla's for the same frame, clamped to 0.02–4, and scale mouse sensitivity down by it when it is below 1.

#### Scenario: Zoom mod
- **WHEN** a mod halves Java's field of view
- **THEN** mcr's terrain is drawn with half its field of view and Java reports a scale of 0.5
