## Context

Per-frame world rendering in 26.3:

- **Extract.** `GameRenderer.extract` → `extractCamera`, `LevelExtractor.extract(DeltaTracker, Camera, float)`.
  The extractor fills `LevelRenderState`:
  - section updates for dirty visible sections;
  - `extractVisibleEntities` (frustum, plus `LevelRenderer.isSectionCompiledAndVisible(BlockPos, long)`);
  - `extractVisibleBlockEntities`, which iterates `levelRenderer.visibleSections()` and takes
    `section.getSectionMesh().getRenderableBlockEntities()`, so block entities are discovered by the section compiler;
  - outline, destroy animation, weather, sky, particles, clouds, gizmos.
- **Render.** `GameRenderer.renderLevel()` builds the projection (bob, hurt, portal effects) and calls
  `LevelRenderer.render(…)`:
  1. `submitFeatures` (entities, block entities, destroy animation, transient blocks, particles, outline).
  2. `featureRenderDispatcher.prepareFrame` → `PreparedFrame`.
  3. Frame graph passes `clear`, `sky`, `main` (`addMainPass`):
     - `executeSolid` = terrain OPAQUE plus `featureFrame.executeSolid`;
     - classic transparency = `featureFrame.executeTranslucent`, terrain TRANSLUCENT,
       `executeTranslucentAfterTerrain`, clouds, weather, world border;
     - then `executeOutline`, `executeSeeThrough`, `executeAlwaysOnTop`.
  4. Entity outline post chain.
  5. After the graph: `compileSections`, `uploadTerrainBuffersToGpu`, `sectionOcclusionGraph.update`.
- Then `render3dHud` (hand, with a depth clear first), `levelRenderer.blitEntityOutline()`, `applyPostEffects()`.
- Terrain draws go only through `ChunkSectionsToRender.renderGroup`/`renderOit`.
- `SectionMesh` is an interface (`CompiledSectionMesh.UNCOMPILED`/`EMPTY` are instances), and
  `RenderSection.sectionMesh` is an `AtomicReference<SectionMesh>`.
- Depth is reverse-Z `D32_FLOAT`, cleared to 0.0, with `GREATER_THAN_OR_EQUAL`.
- Fabric rendering-v1 and renderer-api mix into `LevelRenderer`, `LevelExtractor`, `FeatureRenderDispatcher`,
  `SectionCompiler`, entity renderers and render states.

mcr: `mcr-render` draws opaque terrain, then `draw_late_layers` (overlay geometry, translucent terrain, world
effects) in one pass. Its camera is reverse-Z infinite.

## Decisions

**Ownership split (mod mode).**

| Java | mcr |
|---|---|
| entities (incl. item frames, paintings, armor stands, players), block entities, nametags/text features, first-person hand, glowing outline, screen post effects, particles whose type namespace is not `minecraft` | terrain (opaque, cutout, translucent, water), sky, clouds, weather, world border, block outline, destroy overlay, vanilla particles, fog |

The split avoids double drawing and keeps Java's CPU off terrain. Particles: a HEAD hook on
`ParticleEngine.createParticle` drops `minecraft:*` types. mcr ignores non-vanilla particle ids, which it cannot
render anyway.

**Meshless sections.**
- A HEAD hook on `SectionRenderDispatcher.RenderSection.compileSync`/`compileAsync` replaces meshing with
  `BridgeSectionMesh.scan(section)`.
- The scan iterates the Java `LevelChunk.getBlockEntities()` inside the section. It keeps those for which
  `BlockEntityRenderDispatcher` has a renderer and returns a `SectionMesh` with those block entities, no layers,
  no translucency state, and `facesCanSeeEachother` = true.
- Cost: one hash-map walk per dirty section, no geometry.
- Vanilla's occlusion graph then degenerates to frustum plus distance (no cave culling). That over-includes some
  entities, which mcr's depth hides correctly. A later refinement can feed mcr's visibility bitset into
  `isSectionCompiledAndVisible`.
- `LevelRenderer.prepareChunkRenders`/`prepareChunkRendersIndirect` return an empty `ChunkSectionsToRender`, and
  `uploadTerrainBuffersToGpu` is cancelled.
- In `LevelExtractor.extract`, skip `skyRenderer.extractRenderState`, `weatherEffectRenderer().extractRenderState`,
  cloud extraction, `extractBlockOutline` and `extractBlockDestroyAnimation`.
- In the frame graph, skip the `clear` and `sky` passes and the cloud, weather and world border renders inside the
  main pass.
- `Options.improvedTransparency` is forced false while in mod mode, so the classic path runs.

**Camera and projection.**
- `Camera.update(DeltaTracker)` is where vanilla calls `alignWithEntity(partialTicks)`, builds the cull frustum
  (`prepareCullFrustum` with `createProjectionMatrixForCulling()`) and calls `setupPerspective(…)`. A
  `@WrapOperation` replaces `alignWithEntity` with Rust's position, yaw and pitch from `FrameBegin`, and the
  projection inputs are replaced with Rust's matrix. That matrix already contains bob, hurt and FOV effects.
- In `GameRenderer.renderLevel`, the level projection is wrapped the same way. Java's culling frustum and
  `Projection` UBO then match mcr exactly.
- Fog uniforms stay Java's (`FogRenderer`). Rust's fog follows vanilla's formulas, and differences are checked
  visually (task 4.1).

**World slots.** Inside the main pass lambda, `@WrapOperation`s around the `PreparedFrame` calls invoke
`McrFrame.cut(slot)` after:
- `executeSolid` → `WORLD_SOLID`
- `executeTranslucent` → `WORLD_TRANSLUCENT`
- `executeAlwaysOnTop`, which closes `executeTranslucentAfterTerrain`, outline and see-through → `WORLD_AFTER`

`render3dHud`, `blitEntityOutline` and `applyPostEffects` run under `POST_WORLD`.

`cut` flushes the open `McrRenderPass` stream and ends the wgpu pass. It finishes the encoder into a command buffer
tagged with the slot and frame. The Java `RenderPass` object stays open: the decoder re-opens a continuation pass on
the same attachments with `Load` ops and re-applies the tracked pipeline, buffers, uniforms and scissor before the
next op.

mcr side: `mcr-render` ends its world pass after opaque geometry at `after_opaque` (submit `WORLD_SOLID`, then
`WORLD_TRANSLUCENT`) and continues with `draw_late_layers` in a new pass. At `after_world` it submits
`WORLD_AFTER`, `POST_WORLD` and then `GUI` (from `mod-frame-compose`).

**Per-slot targets.** In world slots, `MainColor` and `MainDepth` resolve to mcr's scene color and reverse-Z
`Depth32Float`; Java's projection is mcr's, so depth values are comparable. In `POST_WORLD`, `MainDepth` resolves
to a private depth texture. The hand's depth clear and `integrate3DHudDepth` stay local, and the scene depth stays
intact for anything mcr draws later. The `entity_outline` target is Java-owned (non-virtual).

## Risks / spikes

- Pass cutting must restore all state exactly. Unit tests on the decoder: the stream before and after a cut renders
  the same as without the cut (offscreen image equality).
- Mods that hook vanilla terrain meshing (renderer-api users, Continuity) run but have no visible effect.
  The mod manager lists them as partially supported.
- `Camera` subclasses or mods that move the camera (Freecam, third-person mods): Java camera mods have no effect,
  because mcr owns the camera. They are listed as unsupported.
- Entity visibility without cave culling costs Java CPU in dense areas. Measured in task 4.2.
