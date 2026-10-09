## Context

`mcr-protocol` has the `packet!` macro and placeholder shared types. Ground truth is the vanilla stream codecs.

## Decisions

- Packets are structs deriving `Encode`/`Decode`; irregular codecs (player_info_update, commands, boss events, recipe displays) use manual impls.
- Polymorphic registries (slot display, recipe display, argument parsers, boss event operations) are Rust enums with the registry id as tag.
- Packets holding placeholder types (Component, ItemStack, HashedStack) have `#[ignore]` round-trip tests until those types land.
- Fields are owned; these packets are infrequent and not hot paths.

## Risks / Trade-offs

- Registry ids (argument types, display types) are transcribed from registration order; a wrong ordinal would desync. Mitigated by fixtures and cross-checking against the generated registries report.
