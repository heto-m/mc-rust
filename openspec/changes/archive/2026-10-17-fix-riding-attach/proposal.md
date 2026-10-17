# Proposal

## Why

The local player was placed 0.4 blocks below every vehicle's position, which puts the eye inside a horse's
head and below a boat's rim. Remote riders used a partial table that ignored baby vehicles, second seats and
most mounts.

## What Changes

- One seat model in `mcr-client` (`attach`): the vehicle's passenger attachment for the rider's seat, minus the
  rider's own vehicle attachment, both scaled by age and the `scale` attribute.
- Boats and rafts (one or two seats, chest boats), camels (two seats, sitting), minecarts, happy ghasts (four
  seats), horses and llamas (adult and baby overrides) and the per-type attachments follow vanilla 26.3.
- The local player and remote riders use it; the seat is the rider's index in `set_passengers`.

## Impact

- Affected specs: `entity-rendering`.
- Not covered: the horse rearing offset, the strider walk bob and camel pose transitions.
