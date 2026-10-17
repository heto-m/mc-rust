## ADDED Requirements

### Requirement: World-scoped state follows its world
When the client's world is replaced (respawn into a dimension, reconfiguration and the login after it) or the session ends, the renderer SHALL drop every section and visibility node of every height range, and the app SHALL empty its loaded-column tracking, mesh queues, block entities, entity tracks and effects.

#### Scenario: Overworld to the Nether
- **WHEN** the player moves from the overworld (y -64..320) to the Nether (y 0..256)
- **THEN** no overworld section remains in the renderer, including those below y 0 and above y 256

#### Scenario: Disconnect with columns pending unload
- **WHEN** the session ends while columns are queued for unloading
- **THEN** the renderer handed back to the menu holds no section and no visibility node

### Requirement: Client state reset with the world
A new world SHALL drop world clocks, block predictions and the controlled vehicle. Reconfiguration SHALL also drop the camera entity, game rules, cooldowns and gameplay state (recipe book, advancements, maps, stats, waypoints).

#### Scenario: Reconfiguration while spectating
- **WHEN** the server reconfigures the client while a camera entity is set
- **THEN** the camera entity is cleared and the player can move after the next login

### Requirement: Server-keyed tables are bounded
Tables whose keys the server chooses SHALL have a total bound: world clocks (64), game rules (1024, values cut to 256 characters), unread dialog requests (64), unread code-of-conduct requests (4), remembered unresolvable entity textures (4096) and unknown equipment assets (1024). Chunk columns outside the storage radius SHALL be dropped whenever the centre or the radius changes, however far the centre jumped.

#### Scenario: Clock spam
- **WHEN** a server sends 64000 distinct clock ids
- **THEN** at most 64 clocks are held

#### Scenario: Centre jump without forget packets
- **WHEN** the chunk centre moves 10000 chunks and the server sends no forget packets
- **THEN** every column of the old area is unloaded

### Requirement: Bounded threads
Streaming sounds SHALL use at most 8 decoder threads at once. Server resource packs SHALL download on one thread with at most 16 batches waiting; results of an earlier session SHALL be ignored. Server-list pings SHALL keep at most 8 threads running across refreshes, and opened URLs and folders SHALL be reaped.

#### Scenario: Stream spam
- **WHEN** a server plays 200 music events at once
- **THEN** at most 8 decoder threads run

#### Scenario: Refresh spam
- **WHEN** the server list is refreshed repeatedly while pings are running
- **THEN** no more than 8 ping threads run at once

### Requirement: Runtime sprites are returned
UI sprites added at runtime SHALL return to the atlas when their owner is dropped: server-list favicons when the screen closes, HUD player heads and the vignette and spyglass overlays when the session ends.

#### Scenario: Reopening the server list
- **WHEN** the multiplayer screen is opened and closed 1000 times
- **THEN** the UI atlas does not grow

### Requirement: Pooled and staged memory is bounded
Idle pooled memory SHALL be bounded in bytes: network buffer pools (8 MiB each), the decoded sound cache (64 MiB, least recently used evicted first), the mod bridge's byte pool (64 MiB), and idle mod-bridge upload blocks larger than the standard 4 MiB. Completed mod-bridge submits SHALL leave the in-flight list when the next submit is recorded. Crack decals of breakers that stopped updating SHALL expire after 400 ticks, and flat-renderer groups unused for a frame SHALL be dropped.

#### Scenario: Large chunk burst
- **WHEN** a thousand 1 MiB packets pass through a connection
- **THEN** the pool keeps at most 8 MiB of idle buffers

#### Scenario: Fast GPU
- **WHEN** 100000 mod-bridge submits complete as fast as they are made
- **THEN** the in-flight list holds a handful of entries

#### Scenario: Many distinct sounds
- **WHEN** thousands of different 1 MiB sounds are decoded
- **THEN** the sound cache holds at most 64 MiB and keeps the most recently played
