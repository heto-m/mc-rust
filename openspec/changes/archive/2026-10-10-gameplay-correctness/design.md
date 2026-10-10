## Context

The client predicts locally and the server compares: container clicks carry hashed stacks, digging and
placing carry sequence ids. Any mismatch costs a resync, so prediction has to equal vanilla.

## Decisions

- Component hashing lives in `mcr-client::hashing`: a CRC32C `HashOps` port over a small value tree, with
  one encoder per component type mirroring the persistent codec. Components without an encoder are
  counted and hash to 0, which makes the server resync only that click.
- Clicks mirror `AbstractContainerMenu#clicked`; per-menu layouts supply slot ranges and `mayPlace` rules for
  shift-click routing. Menus whose routing depends on recipes send no predicted slots.
- Block prediction records the previous state per sequence id and restores it when the ack arrives without
  the server having confirmed the position.
- Vehicles: the simulated vehicle position is sent with `move_vehicle` and corrected by the server.
- Server data is hostile: every collection fed from packets is capped.

## Risks

Recipe-dependent shift-click routing and rarely used components are not predicted exactly.
