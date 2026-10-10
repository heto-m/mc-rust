## 1. Crate skeleton

- [x] 1.1 Create `crates/mcr-settings` with dependencies `mcr-nbt`, `rustc-hash`, `serde_json`, `tracing`
- [x] 1.2 Add the GLFW key table generated from `InputConstants.java`

## 2. Options

- [x] 2.1 Define `OptionValue` and the `options!` macro with vanilla defaults and bounds
- [x] 2.2 Implement named and legacy enum encodings and the normalized FOV encoding
- [x] 2.3 Implement `options.txt` read with unknown-key preservation, warnings and clamping
- [x] 2.4 Implement `options.txt` write in vanilla order

## 3. Key bindings

- [x] 3.1 Define `InputKey` and the vanilla `KeyMapping` table with defaults
- [x] 3.2 Implement `KeyBindings` lookup, conflicts and toggle state

## 4. Server list

- [x] 4.1 Implement `servers.dat` read and write with hidden entries and bounded NBT limits

## 5. Tests

- [x] 5.1 Round-trip `options.txt` including unknown keys, and a vanilla-style import fixture
- [x] 5.2 Defaults, clamping and malformed-value fallback
- [x] 5.3 Parse every default key binding
- [x] 5.4 Toggle and hold semantics and conflict detection
- [x] 5.5 `servers.dat` round trip, hidden entries and hostile input
