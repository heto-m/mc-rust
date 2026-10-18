## ADDED Requirements

### Requirement: Block lighting fidelity
Ambient occlusion SHALL use vanilla's per-block shade brightness, per-element `light_emission` SHALL light
its quads, and the emissive list SHALL match vanilla including the sculk sensor phase.

#### Scenario: Glass window
- **WHEN** a glass block stands next to a lit floor
- **THEN** it casts no ambient occlusion shadow

### Requirement: Translucent and pane culling
Translucent quads SHALL be sorted by camera distance within a section, and faces between connected panes
and bars SHALL be culled.

#### Scenario: Stacked panes
- **WHEN** two glass panes are stacked
- **THEN** the faces between them are not drawn
