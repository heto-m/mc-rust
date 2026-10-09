# Design

## Context

Model layers are code (`LayerDefinitions.createRoots()`), not data, so extraction runs the client jar
headlessly. Layer construction touches no rendering state. Renderer metadata is spread over ~140 renderer
classes that need a live render context, so only static facts are scraped.

## Goals / Non-Goals

**Goals:** a complete data dump; exact vanilla cube/UV geometry; cheap per-frame posing; one draw per
model type.

**Non-Goals:** wiring into `mcr-render`, entity render layers (armor, held items), dynamic textures
(variants chosen from entity data), per-mob `setupAnim` beyond biped/quadruped/head-look.

## Decisions

- **Reflection over bake**: the extractor reads `LayerDefinition` -> `MeshDefinition` -> `PartDefinition`
  and `CubeDefinition` via reflection, keeping inflate and the unbaked cube origin that `ModelPart` loses.
- **Client classpath**: client.jar plus the datagen libraries plus a few client-only jars downloaded to
  `.cache/libraries` by script; no LWJGL natives are loaded.
- **Renderer scan**: regex over the decompiled sources (reference only) yields textures, model layers
  and shadow radius per entity type, walking the superclass chain. Anything computed at runtime is flagged
  `dynamic_texture`.
- **Skinning**: every part is a bone; vertices carry a bone index, and the CPU writes one world-space
  matrix per bone per instance into a storage buffer. The vertex shader multiplies bone matrix by
  position. Vertex data is shared per model.
- **Pose evaluation**: per bone local pose (offset, ZYX Euler rotation, scale) in sixteenths, composed down
  the parent chain exactly like `ModelPart.translateAndRotate`.
- **Textures**: entity textures are copied into one array whose layer size is the largest texture; UVs
  are normalized by each model's own texture size and rescaled by the layer fill fraction.

## Risks / Trade-offs

- Regex scraping misses renderers that compute textures at runtime -> flagged, caller supplies the texture.
- Texture array layers share a size -> smaller textures occupy the top-left corner, UVs rescaled.
