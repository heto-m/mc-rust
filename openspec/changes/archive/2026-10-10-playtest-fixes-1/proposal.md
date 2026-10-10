## Why

The first hands-on playtest and a benchmark run exposed eight defects in the playable client: a flipped view looking straight down, wrong light after block changes and on item entities, missing local break particles, targetable non-pickable entities, split mob heads rotating apart, a placement that did not show, drift when standing on leaves or water, and a render distance default that is too low.

## What Changes

- Make the view matrix well defined at pitch +-90 degrees.
- Run a client-side light engine for block changes and remesh every section whose light changed; sample entity light at the entity position.
- Spawn destroy particles for the local player's breaks and crack particles while digging.
- Restrict entity picking to vanilla pickable entities and order block/entity hits by distance.
- Rotate head, beak and wattle (and other split-head parts) together.
- Fix placement against replaceable blocks and its prediction/remesh.
- Remove idle drift on leaves and in water.
- Default render distance becomes 32.

## Capabilities

### New Capabilities

### Modified Capabilities

- `playable-client`: view at the pitch limits, client-side light, local break particles, entity picking, placement, idle stability.
- `client-options`: render distance default 32.

## Impact

`mcr-render` (camera), `mcr-world`/`mcr-client`/`mcr-app` (light, picking, particles, placement), `mcr-entity-model` (split heads), `mcr-physics` (idle), `mcr-settings`, `mcr-app` CLI.
