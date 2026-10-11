## ADDED Requirements

### Requirement: Frame sequences
`--screenshot` SHALL accept `--frames N` to write N further frames (`<stem>-fNNN.png`), one game tick apart, and `--frame-step dx,dy,dz,dyaw,dpitch` to move the camera before each, so temporal artifacts can be diffed.

#### Scenario: Moving sequence
- **WHEN** a screenshot is taken with `--frames 16 --frame-step -0.15,0,0.05,0.1,0.05`
- **THEN** sixteen numbered PNGs follow the main one along that path
