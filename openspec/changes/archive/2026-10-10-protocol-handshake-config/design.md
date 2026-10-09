## Context

See proposal.md. Packets use `#[derive(Encode, Decode)]` from `mcr-buf`; irregular codecs are manual.
Sources of truth are the 26.3 stream codecs.

## Decisions

- Packets present in both configuration and play live in `common.rs`. Directional name clashes
  (`keep_alive`, `custom_payload`) get `Clientbound`/`Serverbound` prefixes.
- `show_dialog` differs by state: play sends a holder (`IdOr`), configuration the bare dialog NBT, so
  `ShowDialog` and `ConfigShowDialog` are separate types sharing one packet name. Dialog contents stay
  as raw NBT until a dialog model is needed.
- `Component` is modelled on `mcr_nbt::Value`: `to_nbt`/`from_nbt`. Decoding follows vanilla's
  compact-or-typed content matching; unknown style keys are ignored and malformed parts degrade to
  empty text in lenient mode. Encoding collapses plain unstyled text to a bare string tag.
- Login disconnect carries JSON on the wire; it stays a raw JSON `String` because the crate has no JSON
  dependency.
- Custom payload data is borrowed (`RestBytes`) to avoid copying.

## Risks / Trade-offs

- Out-of-range enum values (chat visibility, particle status) are rejected rather than wrapped as
  vanilla does; only servers decode them.
- Opaque NBT for player sprites, dialogs and item hover data defers modelling to later changes.
