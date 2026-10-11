## Decisions

- **Entities** keep the existing rule: distance to the entity position against mean box edge (`(w+h+w)/3`) x 64 x `entityDistanceScaling`, then the box inflated by 0.5 against the frustum. It moves into the pure `in_view` function so it is unit tested and also covers item pickups, which previously bypassed culling. Culled entities are never fed to the scene, so no mesh is built.
- **Block entities** use `render_bounds(kind)`: unit block inflated by 0.5, a 3 block cube for pistons, and +-1e7 in Y for beacons and end gateways (a finite stand-in for vanilla's infinite box that avoids `inf * 0` in plane tests). Distance uses `getViewDistance` per kind (64, piston 68, gateway 256, beacon render distance); beam kinds measure horizontal distance only. This replaces the 1.8 radius sphere and the beacon frustum bypass. Culled block entities emit no geometry or text.
- **Sections beyond the fog**: the terrain shader treats a fragment as fully fogged when `max(horizontal distance, |dy|)` reaches the render-distance fog end, so a section whose nearest point (padded box) is at or past it cannot show terrain. `beyond_render_fog` (CPU, translucent sort) and `beyond_fog` (WGSL, early and late passes) apply that exact metric; it is exact rather than a plain sphere, so images inside the fog are unchanged. A non-positive end disables it.
- **Hi-Z for entities and block entities: not done.** The pyramid is built on the GPU from the previous draw and tested in compute; entities and block entities are culled on the CPU before their instance buffers are written. Using it would need a readback with a frame of latency or moving their culling to the GPU, and a popped-in entity is worse than the few draws saved. Revisit if entity counts dominate.

## Risks

- Beyond the fog end the sky shows through the culled terrain instead of fog-mixed terrain; the two only differ where sky and fog colours differ at the horizon, as in vanilla.
