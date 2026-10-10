# section-culling Specification

## Purpose
Decides which terrain sections are drawn each frame beyond frustum culling: a section visibility graph
searched from the camera (vanilla-style) and two-phase Hi-Z occlusion culling that keeps the image unchanged.

## Requirements

### Requirement: Section visibility graph
The renderer SHALL draw a section only if it is reachable from the camera's section by stepping between
adjacent sections, where each step moves away from the camera section along its axis and leaves a section
only through a face connected to a face it was entered by. Sections whose connectivity is unknown SHALL
count as fully open, and the search SHALL be skipped while the camera is inside an opaque full block.

#### Scenario: Sealed wall
- **WHEN** a layer of sections with no connected faces lies between the camera and other sections
- **THEN** the wall sections are drawn and the sections behind them are not

#### Scenario: Opening reconnects
- **WHEN** one wall section is replaced by a section whose faces all connect
- **THEN** sections behind it are drawn again in the next frame

#### Scenario: Camera in a block
- **WHEN** the camera is inside an opaque full block
- **THEN** every frustum-visible section is drawn as if the graph were disabled

### Requirement: Two-phase occlusion culling
The renderer SHALL draw the sections that passed the occlusion test in the previous frame, build a
conservative depth pyramid from that depth, then test every other frustum-visible section's bounds against it
and draw the ones that may be visible in the same frame, recording the result for the next frame.

#### Scenario: Identical images
- **WHEN** the same scene and camera path are rendered with and without occlusion culling
- **THEN** every frame is pixel-identical

#### Scenario: Camera cut
- **WHEN** the camera turns 180 degrees or teleports between two frames
- **THEN** the first frame after the cut already shows every section the unculled renderer shows

#### Scenario: Hidden sections skipped
- **WHEN** sections lie entirely behind nearer opaque geometry for two consecutive frames
- **THEN** they produce no draws from the second frame on

### Requirement: Translucent culling
Translucent section draws SHALL keep their back-to-front order and SHALL be skipped when their section is
unreachable in the visibility graph or failed the occlusion test this frame.

#### Scenario: Water behind a hill
- **WHEN** a section with water lies behind a hill
- **THEN** its translucent quads are not rasterized and the image is unchanged

### Requirement: Culling switches
Visibility-graph and occlusion culling SHALL each be configurable at creation and switchable at runtime,
and frame statistics SHALL report the GPU time of the whole world frame and the CPU time of the graph search.

#### Scenario: Toggle at runtime
- **WHEN** occlusion culling is switched off between frames
- **THEN** the next frame draws every frustum- and graph-visible section
