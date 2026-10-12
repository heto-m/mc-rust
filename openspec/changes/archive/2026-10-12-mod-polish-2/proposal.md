# Proposal

## Why

The compatibility sweep (`mod-compat-sweep-1`) and earlier changes left gaps between mcr and the Java shadow
client: Controlling and Java's Controls screen rebind keys that mcr ignores, Java-rendered entities and GUI miss
server resource packs, Java's fog and entity distance follow its own render distance, vanilla particles that mods
spawn client-side are dropped, Java's pause buttons peek around the native "mcr" menu, lock keys start off
whatever the keyboard says, and Mod Menu and IPN print stack traces when the HTTP proxy refuses their update
checks.

## What Changes

- Vanilla key mappings sync both ways: mcr's `options.txt` is the one persisted copy; Java's vanilla
  `KeyMapping`s follow it, and rebinding in Java (Controls, Controlling) rewrites mcr's bindings. Mod mappings
  stay Java-only.
- When mcr has applied a server resource pack set, Java loads the same files as local server packs and drops them
  on pop; nothing is downloaded or answered twice.
- `FrameBegin` carries mcr's render distance and entity distance scaling; Java's effective render distance (fog,
  entity view scale, culling) follows them.
- Vanilla-type particles are dropped only inside the vanilla code paths mcr mirrors (packet handling, entity,
  block entity, level, weather and particle ticks, ambient block ticks); mod code outside them keeps its vanilla
  particles in Java.
- The native "mcr" menu over Java's GUI covers the whole screen behind its panel.
- Caps Lock and Num Lock start from the keyboard LEDs on Linux.
- Requests refused by mcr's HTTP allow-list log one quiet line per logger instead of stack traces.
- ABI: `BRIDGE_VERSION` 9 (`FrameBegin` fields, `MSG_KEYS`, `MSG_PACKS`, `REPORT_KEYS`).
