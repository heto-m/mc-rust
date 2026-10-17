## ADDED Requirements

### Requirement: Soak harness
`cargo xtask soak` SHALL start a vanilla server with a fresh world in its own run directory (port 25567 by default, refusing to start while another `server.jar` runs) and run `mcr --soak` offscreen in its own memory-capped scope for the requested duration (20 minutes by default), sampling mcr's RSS and thread count from `/proc` and its printed GPU counters into a CSV every 10 seconds.

#### Scenario: Another server running
- **WHEN** a `server.jar` is already running
- **THEN** the soak refuses to start

### Requirement: Soak churn
Through the server console the soak SHALL churn entity waves, far teleports and dimension changes, `/reload`, chat, titles, boss bars, scoreboards and teams, items with components, signs and banners, weather and time, particles and sounds, and kicks and transfers.

#### Scenario: A cycle
- **WHEN** one 60-second churn cycle has run
- **THEN** it summoned and killed a wave, removed the previous boss bar, objective and team, and teleported the player

### Requirement: Soak verdict
The soak SHALL fail when the RSS or GPU memory slope after warm-up exceeds the limit (1 MiB per minute by default), when the thread count grows, when mcr exits early, or, with `--mods`, when the JVM heap after collection grows faster than the limit.

#### Scenario: Leak detected
- **WHEN** RSS grows 2 MiB per minute after warm-up
- **THEN** the soak fails and names the slope

#### Scenario: Flat run
- **WHEN** RSS, GPU memory and threads stay level after warm-up
- **THEN** the soak passes and writes `samples.csv` and `summary.txt`

### Requirement: Offscreen soak mode
`mcr --connect <addr> --soak <secs>` SHALL play offscreen for the given time, rejoin the last server three seconds after a session ends, respawn a dead player, and print a `soak-stats` line every 10 seconds with wgpu's live buffer and texture bytes, bind groups, the entity renderer's live texture layers, and the entity and chunk counts.

#### Scenario: Kicked during a soak
- **WHEN** the server kicks the player
- **THEN** mcr joins again and the next stats line counts one more join
