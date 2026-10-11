## 1. Entity event effects

- [x] 1.1 Table every `handleEntityEvent` override of the 26.3 client against the class hierarchy (design table)
- [x] 1.2 Particles, local sounds and animation clocks in `mcr-app` `entity_events`, ticked like `aiStep`
- [x] 1.3 Renderer inputs: sheep grazing, goat ram, ravager stun and roar, hoglin attack, warden tendrils and keyframes, creaking keyframes, armadillo peek, rabbit hop, armor stand wiggle, evoker fangs bite, squid resync, wolf wet coat
- [x] 1.4 Client state: swap hands, ravager roar knockback, fishing pull, debug info, use-item completion
- [x] 1.5 Fireworks: starter, coloured sparks with fade, trail and twinkle, flash, blast and twinkle sounds
- [x] 1.6 Sniffer digging sound
- [x] 1.7 Tests: dispatch table, clocks, real-particle taming and firework runs, client state, rigs

## 2. Tamed pet collars

- [x] 2.1 Check the synched data indices against 26.3 `defineSynchedData`
- [x] 2.2 Default the wolf and cat collar to red when the server omits it; test
- [x] 2.3 Dump real metadata of tamed wolves, cats and parrots with the headless client

## 3. Elytra glide pose

- [x] 3.1 Fall-flying body pitch and roll, riptide and swim rotations for players and mannequins
- [x] 3.2 Limb swing divided by `speedValue`; wings layer with `ElytraAnimationState` on every humanoid
- [x] 3.3 Tests: glide rotation, limb damping, wing mirroring and crouch drop, animation targets; offscreen render

## 4. Elytra and creative flight

- [x] 4.1 No glide while creative flying; abilities packet on every flight toggle; tests

## 5. Thrown items

- [x] 5.1 Point the entity billboard at the camera (it was never given one)
- [x] 5.2 Vanilla scale, full bright and near-camera hiding; dragon fireball quad; tests and offscreen render

## 6. Primed TNT

- [x] 6.1 Client fuse countdown, event 70 minecart priming, smoke
- [x] 6.2 Swell and white overlay for primed TNT and TNT minecarts; minecart display blocks; tests
- [x] 6.3 Minecarts, boats and arrows without the living model drop
- [x] 6.4 Flint and steel on a live server plays `entity.tnt.primed`; summoned TNT stays silent

## 7. Kelp blinking

- [x] 7.1 Fixed and moving frame sequences over an ocean copy (`--frames`, `--frame-step`)
- [x] 7.2 Coverage-preserving cutout mips with `mipmap_strategy` and `alpha_cutoff_bias`; tests

## 8. Underwater detail

- [x] 8.1 Compare sky light, water alpha and tint with vanilla; read sky light of data-less sections like vanilla; tests

## 9. Item drop prediction

- [x] 9.1 Tests for single item, whole stack, empty hand and spectator
