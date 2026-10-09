# Proposal

## Why

`mcr-protocol` was written by six parallel agents. Some packets use private placeholder types with `todo!()` codecs, several tests are ignored pending shared types, there is no way to turn a raw frame into a typed packet, and nothing has been checked against real server traffic.

## What Changes

- Replace placeholder component, predicate and item-template types with the real ones; remove every `todo!()` and un-ignore every test.
- Remove duplicate definitions and inconsistent naming.
- Add one dispatch enum per (state, direction) with `decode`, `encode` and `id`, backed by a `PacketIds` table built once from `mcr-data`.
- Add a unified `ProtocolError` carrying packet name, state and direction.
- Add a capture-validation test: every captured clientbound frame decodes, consumes all bytes and re-encodes byte-exact. Fix all codec bugs found.
- Add a decode-throughput benchmark over the captured play stream.

Out of scope: connection handling, compression, encryption, other protocol versions.

## Capabilities

### New Capabilities
- `protocol-dispatch`: packet-id dispatch enums, id tables, error context, capture-exactness guarantee.

### Modified Capabilities

## Impact

`crates/mcr-protocol` only, plus small codec fixes in `mcr-buf`/`mcr-data` if needed.
