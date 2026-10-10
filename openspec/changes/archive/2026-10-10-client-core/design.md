## Context

`mcr-net` gives a threaded `Connection` with an inbound hook for compression; `mcr-protocol` decodes packets by state through `PacketIds`; `mcr-world::ChunkStore` stores chunks; `mcr-physics::PlayerPhysics` simulates the local player and decides which movement packets to send.

## Decisions

- **Synchronous, caller-driven.** `Client::tick` drains the network queue and advances one game tick; the caller owns the clock. No threads beyond mcr-net's.
- **State is plain data** in `ClientState`, mutated only by packet handling and local simulation; consumers read it and drain `ClientEvent`s. Events carry ids and positions, not copies of large data.
- **Registries**: `registry_data` entries are stored in arrival order (index = network id) with optional NBT. The dimension type's `min_y`/`height` come from the NBT when present; when the server skipped data (known pack accepted) the four vanilla dimension types are resolved from a built-in table. `ClientConfig::known_packs` selects between replying `minecraft:core` (default) and an empty list.
- **Hostile input**: every collection fed by packet data is bounded; unknown ids are ignored, never panic; positions are sanitised for NaN and range.
- **Chunk batches**: vanilla's calculator (aggregated ns/chunk, weight 49, clamp x3, 7e6 / ns) produces `chunk_batch_received.desired_chunks_per_tick`.
- **Movement**: `PlayerPhysics::tick` output maps to `move_player_*`, `player_input` and `player_command`; `client_tick_end` closes every tick; `player_loaded` is sent once the chunk under the player is present.
- **Digging/placing**: block predictions use sequence numbers; `block_changed_ack` clears them.
- **Container clicks**: `HashedStack` is built from the client's predicted slots; stacks with an empty component patch hash exactly; non-empty patches are a documented gap.
- **Re-configuration**: `start_configuration` -> `configuration_acknowledged` -> configuration handling; registries are replaced and the world cleared at the next `login`.

## Risks

Packet semantics are validated only against the vanilla server; a physics mismatch shows up as corrective `player_position` packets, which the live test counts.
