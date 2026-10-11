## 1. Java world path
- [ ] 1.1 `BridgeSectionMesh` and the `compileSync`/`compileAsync` hooks, with a test that a chest placed by the server appears in `visibleSections()`' renderable block entities
- [ ] 1.2 Disable terrain draws/uploads, sky, clouds, weather, world border, Java block outline/destroy extraction; force classic transparency
- [ ] 1.3 Vanilla particle filter in `ParticleEngine.createParticle`
- [ ] 1.4 Camera and projection from `FrameBegin`

## 2. Slots
- [ ] 2.1 `McrFrame.cut(slot)` hooks in the main pass and `POST_WORLD` scoping
- [ ] 2.2 Rust decoder pass continuation with state re-application; unit test for image equality with and without a cut (offscreen wgpu)
- [ ] 2.3 Per-slot `MainDepth` resolution (scene vs private)
- [ ] 2.4 `mcr-render` split at `after_opaque`/`after_world`; mod-mode toggles for native entities, block entities, nametags

## 3. Verification
- [ ] 3.1 Test mod: armor-stand render layer via Fabric's `LivingEntityRenderLayerRegistrationCallback`
- [ ] 3.2 Integration scene against `xtask server` (summoned entities, chest, sign, glowing pig, occluded zombie) with pixel probes per object
- [ ] 3.3 Visual check of fog on entities versus terrain at render distances 8 and 16; note differences in design.md

## 4. Measurement
- [ ] 4.1 Java frame time and render-thread wait with 0, 100 and 500 entities at 1080p; record in design.md
- [ ] 4.2 Java CPU of meshless section scans during chunk loading; record in design.md
- [ ] 4.3 fmt, clippy, tests, archive
