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

## As built

Differences from the decisions above:

- **Hooks.** MixinExtras is not on the bridge's compile class path, so plain `@Inject`/`@Redirect`/`@ModifyArg`
  replace the planned `@WrapOperation`s. Cuts: `LevelRenderer.render` HEAD begins `WORLD_SOLID`; `executeSolid`
  RETURN cuts to `WORLD_TRANSLUCENT`; after `PreparedFrame.executeTranslucent` in `executeClassicTransparency`
  cuts to `WORLD_AFTER`; `GameRenderer.render3dHud` HEAD cuts to `POST_WORLD`. Slots only move forward.
- **Meshless sections.** `compileSync`/`compileAsync` set a `BridgeSectionMesh` and cancel. `LevelExtractor`'s
  region copy is skipped (`createRegion` → null), `extractSectionDrawGroups` returns 0 and
  `uploadTerrainBuffersToGpu` is cancelled. `addSkyPass`, `SkyRenderer.extractRenderState`, weather and world
  border extract/render are cancelled. `ClientLevel.addDestroyBlockEffect` is cancelled (mcr spawns those).
- **Camera.** `Camera.alignWithEntity` takes Rust's position and rotation (`yRot = 180 - yaw°`,
  `xRot = -pitch°`) and `FRAME_DETACHED` (third person) drives `detached` and `options.cameraType`. The level
  projection is `flipY(P_mcr) · V_mcr · V_javaᵀ`: Java keeps its own view rotation (billboards, nametags) and the
  product equals mcr's view-projection, roll included. Culling uses a conventional perspective with mcr's fov and
  aspect, near 0.05 and Java's `depthFar`: with mcr's reverse-Z infinite matrix,
  `Frustum.offsetToFullyIncludeCameraCube` never terminated and hung Java's render thread (found by the scene
  test; mcr kept running and detached Java as designed).
- **Orientation.** World slots draw top-down with a Y-flipped projection; pipelines whose `Projection` uniform is
  live get a counter-clockwise front-face variant (`VariantKey.flip`). Fullscreen and post passes are unchanged, so
  the two compositor flips are gone for world slots. The GUI slot still uses them: scissors and picture-in-picture
  textures assume bottom-up.
- **Targets.** In world slots, Java's virtual main color/depth resolve to the scene's; sampling a virtual target
  reads the scene, or a copy when the same pass also renders into it. `POST_WORLD` gets a private
  `Depth32Float` at scene size, created on first use.
- **Staging.** Buffer writes copy into pooled byte vectors (at most 64 of up to 16 MiB) instead of a fresh
  allocation per submit.
- **mcr-app.** While Java's world is active, native entities, block entities, nametags and the hand are skipped
  (particles, block outline and destroy overlay stay native); the perspective is sent each frame.
- **Proxy.** `proxy.rs` in `mcr-modbridge`: a thread per connection, `CONNECT` and absolute-form requests
  (rewritten to origin form, hop-by-hop headers and `Host` replaced, `Connection: close`). Allow-list: Mojang's
  hosts plus the policy's `http_hosts`; resolved addresses must be global; ports 80/443; 16 connections, 8 KiB
  head, 30 s idle and head deadline. `-Dhttp(s).proxyHost/Port` plus a `GameConfig.UserData` mixin, because
  Minecraft's skin and pack downloads use its own `Proxy` (`NO_PROXY` by default).
- `BRIDGE_VERSION` 7 (pass descriptor gained a flags word).

Gaps:
- Vanilla particles that a mod spawns client-side are dropped with the rest of `minecraft:*`.
- `executeAlwaysOnTop`'s depth clear of the scene depth is dropped, so always-on-top gizmos depth-test.
- Scissor rectangles are not flipped in world slots (no vanilla world draw scissors).
- Java's fog comes from Java's own `renderDistance` option, which nothing syncs with mcr's yet.
- `SectionRenderDispatcher` still allocates its upload buffers, unused.

## Measurements

1920x1080 offscreen on a Radeon RX 6800-class GPU (Navi 21), NoAI zombies on a stone platform (`entity_scene` test,
`MCR_ENTITY_LOAD=1`), 30 s per run, timings from mcr's compose stats:

| zombies | Java frame p50 / p99 | render-thread wait p50 / p99 | mcr frame p50 / p99 |
|---|---|---|---|
| 0 | 0.89 / 8.8 ms | 0.66 / 7.7 ms | 1.96 / 12.7 ms |
| 100 | 1.35 / 7.0 ms | 1.18 / 6.4 ms | 2.56 / 10.9 ms |
| 500 | 6.66 / 17.0 ms | 6.45 / 16.7 ms | 9.52 / 24.9 ms |

Java's entity path costs about 11 µs per zombie per frame on the render thread, which waits for it; 500 visible
mobs fit a 60 Hz frame at p50 but not at p99.

Meshless section scans during a join (Java render distance 12 and 16, platform at y 150 around spawn): 247–585 scans
totalling 0.45–0.8 ms, 1.1–1.8 µs per scan, all on Java's render thread.

Fog (3.3), ghast at 112 blocks next to a stone pillar: at render distance 8 the ghast and the pillar show the same
light haze; at 16 neither is fogged. Smaller mobs vanish at vanilla's entity distance (≈ 64 blocks for a zombie)
long before fog matters, and the server's view distance (10) bounds tracking, so a ghast at 232 never arrived.
No seam between Java's entity fog and mcr's terrain fog was visible while both use the same render distance.
