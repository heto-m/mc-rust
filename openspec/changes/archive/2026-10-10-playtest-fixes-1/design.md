## Context

See proposal.md. Vanilla's client owns a light engine and spawns break particles locally; the server excludes the breaker from the destroy-block level event.

## Decisions

- The look-to view uses an up vector derived from yaw and pitch (the pitch derivative of the forward vector), so +-90 degrees never degenerates. Alternative: clamp just below 90, rejected as not exact vanilla.
- Light: a propagation engine in `mcr-world` operating on the chunk store, updating sky and block light incrementally on block changes (increase/decrease BFS, vanilla opacity and emission rules). Server light packets still overwrite sections. Affected sections plus neighbours are remeshed.
- Particles: the app emits destroy particles when the local break completes and crack particles per dig tick, as vanilla does client-side.
- Picking follows `Entity.isPickable` via an entity-type allow-list.
