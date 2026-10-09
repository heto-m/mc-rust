# Design

## Pipeline
`ChunkStore` -> `snapshot()` (caller thread: padded 18^3 states, sky/block light, biome window) ->
`MeshPool` workers -> `SectionMesh` + tint table -> caller uploads to the renderer.

## Model cache
One `StateModel` per global state id. The cube fast path requires a single part, first variant, and
full-face quads with standard UV orientation. Random variant rotations are ignored on the fast path
(first variant). Everything else is generic quads, culled by cullface. The layer comes from sprite
transparency (opaque/cutout/translucent) like vanilla's `ChunkSectionLayer.byTransparency`, or
`force_translucent`. Tint kinds: 0 none, 1 grass, 2 foliage, 3 dry foliage, 4 water, 8.. constant
palette (spruce, birch, lily pad, stems, redstone power 0-15, attached stems).

## Quad data
Cube quad: tint kind in w0 bits 26..32. Model quad: w7 bits 8..14 tint kind, 14..18 cell x, 18..22 cell z
(the block column used for the tint lookup). Layer field: `group << 14 | layer` (up to 4 groups).

## Lighting
Light is per quad (sky<<4|block of the cell in front of the face). Ambient occlusion is per corner, 2
bits, vanilla's side/corner rule. The shader maps the level to a brightness approximating vanilla's
shade times blended-light darkening. True per-vertex light would need 16-byte quads; deferred. The
diagonal flip follows vanilla anisotropy. Merging requires identical AO and AO uniformity along the merge
axis so interpolation across a merged quad equals the per-block result.

## Tints
Per-column colours for grass/foliage/dry foliage/water are averaged over (2r+1)^2 columns of biome
colours (default r=2), sampled at the section's middle quart layer (vanilla samples per block Y). Biome
zoom jitter is ignored; swamp grass uses a fixed colour (no noise). Tables are 4 x 256 u32 per section in
a storage buffer indexed by a per-section offset; sections without tinted quads get none.

## Render layers
Translucent geometry is always emitted as model quads (glass-like cubes included), so one pipeline and
one sorted list serve the whole pass. Pipelines use `GreaterEqual` depth so a grass-side overlay quad,
emitted with exactly the same rectangle as its base quad and placed after it, wins the depth tie.
Shading follows vanilla's cardinal lighting (east/west 0.6, north/south 0.8, down 0.5) and corner
shade levels 0.4/0.6/0.8/1.0.

Section meta holds main-pass (opaque + cutout, alpha test) and translucent ranges. Translucent draws are
built on the CPU each frame: frustum test, sort far to near, indirect commands uploaded, blended pipeline
without depth writes. Colour is shaded in gamma space (Unorm target and texture) to match vanilla.

## Fluids
Heights follow vanilla `FluidRenderer` (weighted corner average, full-height source weight 10). Still
sprite for top and bottom, flowing sprite for sides; flow-direction UVs are not implemented. Animated
sprites use their first frame.

## Overlays
A cube face may carry a second full-face quad (grass side overlay). The mesher merges base and overlay
under one key and emits two quads over the same rectangle; a direction's overlays follow its base quads.

## Performance
Per padded section the mesher precomputes attribute and occlusion byte arrays, skips fully enclosed
opaque cells, takes a fast path when a face's 3x3 neighbourhood is open and evenly lit, and walks
occupied slice bits instead of whole slices when merging.

## Threading
A pool of std threads with a priority heap; results return over a channel and are drained with `poll`.
