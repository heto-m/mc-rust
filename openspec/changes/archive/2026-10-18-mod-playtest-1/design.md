## Context

Mod mode mirrors Rust's world and player into Java through the net bridge. Packets reach Java as copies of what the
server sent; anything vanilla predicts on the client without a server echo never reached Java's `LocalPlayer`.

## Decisions

### IME state follows the composition, not `Ime::Enabled`

winit 0.30 on Wayland emits `Ime::Enabled` right after `set_ime_allowed(true)` whenever a `zwp_text_input_v3` object
exists. KWin always offers one, so the old route, which suppressed key text from `Enabled` until `Disabled`, dropped
every character with no input method installed. The route now tracks only whether a preedit is non-empty: key text
flows unless a composition is in progress, a commit ends it, and `Ime::Disabled` clears it.

### Local actions as bridge messages

`Relay::local_action` (default no-op) receives `LocalAction::{Swing, Drop, Destroy, ResetAttackStrength}` from
`mcr-client` at the points where vanilla's `MultiPlayerGameMode` acts locally: the drop prediction, the attack and
use swings, start/continue/stop/finish of mining and the attack-strength reset. The bridge encodes each as a
32-byte `NET_ACTION` (kind, arg, block position, value) on the net ring, ordered with the packet copies. Java
applies it in `PlayerShadow.action` on the render thread:

- swing: `LivingEntity.swing(hand, animation, false)` with the held item's attack or interact animation;
- drop: `removeFromSelected(wholeStack)` and a default swing, matching `MultiPlayerGameMode.dropItem`; the server
  calls `setRemoteSlot`, so no slot update follows;
- destroy: `Tutorial.onDestroyBlock` with the block state at the position and the progress, -1 when aborted;
- reset: `resetAttackStrengthTicker`.

Item use no longer sends a `Punch` from mcr: vanilla's use swing is local only.

### Movement keys

`PLAYER_INPUT` in the player state holds forward/back/left/right/jump/shift/sprint bits from Rust's physics input.
After `ClientInput.tick`, the local player mixin sets `input.keyPresses` from them (move vector stays zero) for
`Tutorial.onInput` and mods, then resets them to `Input.EMPTY` right after, so vanilla movement never uses them.

### Java's section walks

Bridge sections are meshless and see through every face, so Java's visible list is every section in the frustum
within the render distance. Each frame vanilla looked up the dirty state of every one (13% of render thread
samples in an F3-heavy profile) and asked each mesh for block entities (about 10%). The bridge now:

- skips the dirty walk when no section turned dirty or moved and the visible list was not cleared since the last
  walk (version counter bumped by `SectionDirtyState.setDirty/setSectionNode` and `clearVisibleSections`);
- hands the block-entity walk a cached list of the visible sections whose mesh has block entities, rebuilt when
  the visible list is cleared or any section's mesh is set or reset;
- answers `hasAllNeighbors` with true, since a meshless section reads only its own chunk's block entities.

## Risks

- A mod that sets dirty state without `setDirty` would not trigger the walk; vanilla has no such path.
- `NET_ACTION` is posted only in the play phase and dropped by Java for a stale session.
