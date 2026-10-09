# Proposal

## Why

The client cannot move, chat, click containers or interact with the world until every serverbound play packet of protocol 777 has a codec. This is the send side of the play state.

## What Changes

- Add codecs for every serverbound play packet of 26.3 that is not shared with configuration (shared ones live in `common`), in `mcr-protocol::play::serverbound`.
- Movement packets are allocation-free and carry the on-ground and horizontal-collision flag byte.
- Chat packets carry signatures, the last-seen-messages update and the profile public key data.
- Ergonomic constructors for the packets sent constantly: movement, use item on, player action, container click.
- A criterion bench for encoding `move_player_pos_rot`.

## Capabilities

### New Capabilities
- `protocol-play-serverbound`: wire format of the serverbound play packets.

### Modified Capabilities

## Impact

- `crates/mcr-protocol/src/play/serverbound.rs` and submodules; `crates/mcr-protocol/Cargo.toml` gains a criterion dev-dependency and one bench target.
- Out of scope: packets shared with configuration, the `Component`, `ItemStack` and `HashedStack` codecs (other changes), packet dispatch tables.
