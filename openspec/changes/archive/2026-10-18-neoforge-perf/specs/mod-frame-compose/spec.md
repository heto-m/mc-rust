## ADDED Requirements

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
