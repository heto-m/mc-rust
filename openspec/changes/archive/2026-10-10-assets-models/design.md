## Context

Output feeds a wgpu renderer elsewhere; this crate has no GPU code. Block models in 26.3 differ from older versions: `shade` is replaced by `shade_direction_override`, rotations accept `axis`/`angle` or Euler `x`/`y`/`z`, textures may be `{sprite, force_translucent}`, variants carry x/y/z rotation, and pack formats are major.minor.

## Decisions

- **Sprite ids, not layers, in quads.** Baking runs before images are decoded. Quads carry a `SpriteId` from an interner; `TextureArrays` maps it to `{group, first layer, frames, transparency}`. Each sprite owns whole layers, so UVs stay in [0,1].
- **Integer rotation matrices.** Quarter-turn blockstate rotations are exact `i8` 3x3 matrices, making cull remapping and uvlock exact. Element rotations use `f32`.
- **Baked variant cache.** `(model, x, y, z, uvlock)` maps to `Arc<[BakedQuad]>`, shared across states.
- **Shading.** Quads carry `shade_dir` (the override or the quad normal) instead of a boolean.
- **Mutex-guarded zip.** `ZipSource` serialises reads on one archive handle; the baker caches parsed models so reads happen once per file.
- **Animation.** Each frame is its own layer; the renderer interpolates. Per-UV-region transparency is replaced by whole-sprite transparency.

## Risks / Trade-offs

- `builtin/generated` item models produce no quads (block-only scope) → documented.
- Whole-sprite transparency may pick a heavier render layer than vanilla for partially used sprites → acceptable.
