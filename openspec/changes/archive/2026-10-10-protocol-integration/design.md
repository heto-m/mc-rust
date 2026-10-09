## Context

Packet types live in per-state modules and implement `Packet` (name, direction). `mcr-data` maps names to ids per state and direction.

## Decisions

- A declarative macro takes `"name" => Variant(Type)` lists and emits the enum, `decode`, `encode`, `id` and a `NAMES` list. Variants are matched by name, so ids never appear in code.
- `PacketIds` holds name-to-id maps and a flat `id -> variant index` array per (state, direction); dispatch is an array lookup then a match on the index.
- Shared configuration/play packets are one type used in both enums.
- `ProtocolError` wraps `DecodeError` with packet name, state and direction; unknown ids and trailing bytes are distinct variants.
- Capture validation is a test that skips when `.cache/captures` is absent. Byte-exactness is pursued everywhere; deviations are documented in the spec or tasks.
- The bench decodes the captured play stream through the enum and reports bytes/s.
