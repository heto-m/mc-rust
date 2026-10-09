# Proposal

## Why

Entity metadata, particles and sound events appear in many play packets (add entity, set entity data, level particles, sound, explosion). Their codecs are still placeholders, so those packets cannot be decoded.

## What Changes

- `Particle`: registry-ordered type id plus the options each particle type carries.
- `EntityDataValues`: indexed values terminated by 0xFF, with one `EntityDataValue` variant per serializer in 26.3 registration order.
- `SoundEvent`: inline sound definition (identifier plus optional fixed range).
- Registry-order tests against the generated `registries.json` when the cache exists.

Out of scope: data component patches inside item particles beyond the empty patch, registry lookups (ids stay raw), and non-empty item stack and component codecs owned by other changes.

## Capabilities

### New Capabilities
- `protocol-entity-data`: wire codecs for particles, entity metadata lists and sound events.

### Modified Capabilities

## Impact

`crates/mcr-protocol/src/types/{particle,entity_data,sound}.rs` only. No new dependencies.
