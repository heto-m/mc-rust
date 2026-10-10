## MODIFIED Requirements

### Requirement: Command line
`mcr --world <path>` SHALL render that save with `--render-distance`, `--no-vsync` and `--jar` options; `--synthetic` SHALL keep the generated terrain mode; `--connect` SHALL start the playable client.

#### Scenario: Screenshot
- **WHEN** `--screenshot out.png --camera x,y,z,yaw,pitch` is given
- **THEN** the viewer renders offscreen once every column in range is loaded and meshed, writes a PNG and exits

#### Scenario: Connect mode
- **WHEN** `--connect host:port` is given
- **THEN** the playable client starts instead of the viewer
