## Why

The client needs its user settings before any UI or input work can start. Vanilla keeps them in
`options.txt`, key bindings in the same file, and the multiplayer server list in `servers.dat`. An
existing `.minecraft` directory must import cleanly, so the formats have to match vanilla exactly.

## What Changes

- New crate `mcr-settings` with a typed `Options` struct covering every 26.3 client option, with
  vanilla defaults and value ranges.
- Read and write `options.txt` in vanilla's `key:value` format. Unknown keys are preserved, malformed
  values fall back to defaults with a warning, and parsing never panics.
- All vanilla `KeyMapping`s with defaults, stored as `key_<name>:key.keyboard.<name>` (or
  `key.mouse.<name>`). An `InputKey` enum covers GLFW keyboard codes and mouse buttons. A
  `KeyBindings` lookup maps a pressed key to actions, reports conflicts, and applies toggle or hold
  semantics for sneak, sprint, attack and use.
- Read and write `servers.dat` (vanilla NBT server list) through the `mcr-nbt` disk format.

## Capabilities

### New Capabilities

- `client-options`: typed client options and the `options.txt` file format.
- `key-bindings`: key mappings, input keys, lookup, conflicts and toggle state.
- `server-list`: the `servers.dat` multiplayer server list.

### Modified Capabilities

None.

## Impact

- New crate `crates/mcr-settings` depending on `mcr-nbt`, `rustc-hash`, `serde_json`, and
  `tracing`.
- No other crate changes.
- Out of scope: applying options to the renderer or audio, keyboard event dispatch from the window
  system, resource pack lists and the language files, graphics presets that change other options.
