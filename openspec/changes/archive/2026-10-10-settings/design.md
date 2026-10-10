## Context

Vanilla 26.3 `Options.java` loads `options.txt` line by line, splitting on the first `:`. Each value is
parsed by an option-specific codec: booleans and numbers as plain text, named enums as quoted JSON
strings, legacy enums as integer ids. Key bindings are stored as `key_<name>` with
`InputConstants` names (`key.keyboard.w`). Server list entries live in `servers.dat`, a gzip-compressed
NBT file. See proposal.md for motivation.

## Goals / Non-Goals

**Goals**

- Byte-compatible reads and writes of vanilla `options.txt` for every option in `processOptions`.
- Lossless round trip of keys this crate does not model.
- Pure library: no file I/O in `Options` or `KeyBindings`; the caller supplies the text or bytes.

**Non-Goals**

- Data fixers for old `options.txt` versions (`dataFix`). The `version` line is kept as an unknown key.
- Context-aware key conflicts (vanilla `isConflictContext`). Conflicts are same-key only.
- The `fullscreenResolution` value is kept as an unknown key, not interpreted.

## Decisions

- **Option storage**: one `options!` macro generates the `Options` struct, its defaults and a
  key-to-field table. Each field has a value type implementing `OptionValue` (parse, format,
  clamp). Numeric fields declare inclusive bounds; out-of-range values are clamped, unparsable values
  keep the default and emit a `tracing` warning.
- **Enum encoding**: named enums are written quoted (`"fancy"`), legacy int enums bare (`1`), matching
  vanilla's `Options.GSON.toJson` output.
- **FOV**: stored as a normalized double `(fov - 70) / 40` as vanilla does, so `fov:0.0` imports as 70.
- **Unknown keys**: `options.txt` is held as the ordered parsed lines; known keys are replaced in place
  and written in vanilla order, unknown pairs are appended in their original order.
- **Key lookup**: `KeyBindings` keeps a `FxHashMap<InputKey, Vec<MappingId>>` rebuilt on every change.
  Keys are `Copy`, so lookup allocates nothing.
- **Toggle state**: `ToggleState` tracks the logical held value for toggle-able mappings. A press
  flips it when the toggle option is on, otherwise it follows the physical state.
- **servers.dat**: decoded with `mcr_nbt::read_disk` using `Limits::UNCOMPRESSED`; the icon is kept as
  its base64 string and decoded only on demand.

## Risks / Trade-offs

- The GLFW table is generated from `InputConstants.java`; if vanilla adds keys, the table needs a
  refresh. Unknown key names are kept as `InputKey::Unknown` instead of failing the whole file.
