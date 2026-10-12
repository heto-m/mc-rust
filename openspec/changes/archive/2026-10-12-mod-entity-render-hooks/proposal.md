# Proposal

## Why

Over half of the scanned mods change world rendering, mostly entity models, layers and block entities. In mod
mode these draws must come from Java's entity renderers, composed with mcr's terrain so that depth and
translucency are correct.

## What Changes

- In mod mode Java renders entities, block entities, the first-person hand, entity outlines,
  screen post effects and particles of non-`minecraft` types. mcr renders terrain, sky, clouds, weather, water,
  world border, block outline and destroy overlay, and vanilla particles. mcr's native entity, block-entity and
  nametag rendering is off.
- Section bookkeeping without meshing:
  - `SectionRenderDispatcher.RenderSection` compilation produces a `BridgeSectionMesh` (the vanilla `SectionMesh`
    interface). It has no geometry, lists the section's renderable block entities and reports all faces mutually
    visible.
  - Vanilla's occlusion graph, `visibleSections()`, `isSectionCompiledAndVisible` and `extractVisibleBlockEntities`
    keep working unmodified, so mods hooking them still apply.
- Java's terrain draw, sky, clouds, weather, world border and their extraction are disabled, along with
  `uploadTerrainBuffersToGpu`.
- Camera and projection: Java's `Camera` and level projection come from Rust's `FrameBegin`.
- World slots `WORLD_SOLID`, `WORLD_TRANSLUCENT`, `WORLD_AFTER` and `POST_WORLD`:
  - The bridge cuts Java's main pass at vanilla's phase boundaries
    (`FeatureRenderDispatcher.PreparedFrame.executeSolid`, `executeTranslucent`, `executeTranslucentAfterTerrain`
    and the rest).
  - mcr splits its world pass into opaque, then translucent terrain plus world effects, so Java's buffers
    interleave in vanilla's order.
  - Main depth in world slots is mcr's reverse-Z scene depth. `POST_WORLD` (hand, outline blit, post effects) uses a
    private depth, so the hand's depth clear never touches the scene.
- OIT is forced off in Java for v1.

Out of scope:
- Mod terrain models (connected textures, Continuity, Indigo/FRAPI meshes).
- Java-side block outline.
- Per-entity-type native/Java split.
- OIT.
- Shader packs.

Acceptance test, against a local vanilla server with Fabric API plus a test mod that adds an armor-stand render
layer:
- With `/summon`ed zombie, armor stand, item frame, chest, sign and a glowing pig, a captured frame shows each one.
- A zombie behind a wall is not visible (pixel probe), and the zombie and armor stand show their nametags.
- The pig shows its glowing outline, and the test layer is visible.
- Java frame time with 100 zombies at 1080p is recorded in design.md.

## Capabilities

### New Capabilities
- `mod-entity-render`: world-space Java rendering in mod mode, meshless sections, camera sharing, world slots.

### Modified Capabilities

## Impact

`mcr-render`, `mcr-app`, `mcr-modbridge` (slot cuts, per-slot depth), `java/mcr-bridge`.
