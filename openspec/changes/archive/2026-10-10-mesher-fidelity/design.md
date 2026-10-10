# Design

## Quad formats

Cube quad, 16 bytes (`vec4<u32>`): `w0` as before (position, face, size, tint); `w1` = layer `[0,16)`,
four 3-bit AO levels `[16,28)` (0..4 = number of unoccluded terms, brightness `0.2 + 0.2 * level`), UV
orientation `[28,31)` (quarter turns plus mirror, so mirrored and rotated variants merge); `w2,w3` = four corner
lights, each `sky << 8 | block` bytes in 1/16-level units (0..240), corner order (0,0) (1,0) (0,1) (1,1).

Model quad, 40 bytes (`array<u32, 10>`): corners are 27-bit offsets from a per-quad base cell at 1/128 block
(range -1..3), the base cell (4 bits per axis) lives in the spare bits and doubles as the tint column; UVs
as before; `w6` layer, shade face, flags, tint; `w7,w8` corner lights; `w9` six-bit AO per corner.

Greedy merging requires equal layer, tint, rotation and all four corner values, and extends along an axis
only if the corner values are uniform along it, so merged quads interpolate exactly like their cells.

## Light and AO

Per vertex, exactly vanilla's `BlockModelLighter`: four adjacent samples and four corner samples (corner
substituted by the first adjacent when both sides block light), `smoothBlend` with the dark-neighbour
substitution, and the shade term average. Light stays as separate sky/block values (sum of four * 4 =
0..240). Partial model faces bilinearly weight the four unit-corner results by vertex position, which equals
vanilla's weight tables for axis-aligned quads. Quad vertex order puts the diagonal on v0-v2 like vanilla.

The vertex shader evaluates lightmap(sky, block) by manual bilinear lookup in a 256-entry uniform computed on
the CPU with vanilla's `lightmap.fsh` formula, multiplies by AO and directional shade, and passes a colour.

## Variants and offsets

`ModelCache` keeps every weighted alternative. At mesh time the section position and cell give
`Mth.getSeed`; `JavaRandom` reproduces `SingleThreadedRandomSource` (`nextInt(total)`, and `nextLong` +
reseed per part for multipart). Cube states whose alternatives are all standard faces (possibly rotated UV)
stay cubes; the rotation per face is stored, so only rotation-distinct faces fail to merge. Offset blocks use
a static name table with vanilla's per-block maximum offsets.

## Animation

All frames of an animated sprite already occupy consecutive layers. The renderer owns a table indexed by the
packed layer value giving `(current layer, next layer, mix)`; per tick it is rewritten only where it changed.
The fragment shader samples twice and lerps when `mix > 0`. `Renderer::set_animations` supplies the sprites;
`set_animation_time` the tick clock (default: wall clock at 20 TPS).

## Fluids

Port of vanilla `FluidRenderer` including flow vector from `FlowingFluid.getFlow`, flowing sprite rotation,
overlay sprite, back faces, and occlusion-mask based culling. Fluid quads use one light (max of cell and cell
above) and vanilla's fixed side/bottom shading, so shade face carries it and AO is flat.

## Biomes

`MeshInput` carries a quart-resolution biome grid (x, z with margin, 6 quart layers) and the hashed world
seed. The mesher resolves biomes per block column with the zoom algorithm, then blends. The y used per column
is the topmost tinted block of that column in the section.

## Occlusion masks

Each state gets per-face 16x16 masks from its occlusion shape (interned). A face renders when
`mask(self) & !mask(neighbour_opposite) != 0`, vanilla's `joinIsNotEmpty(ONLY_FIRST)` at 1/16 resolution.

## Environment

`Environment` (defaults: overworld noon) holds sky/fog colours, fog range, lightmap inputs and cardinal
lighting. `Environment::overworld(day_time)` interpolates the vanilla day timeline keyframes.
`FrameInputs::sky_light` keeps its meaning (multiplies the sky factor).

## Simplifications that remain

Block light flicker, boss/darkness effects, cloud and sky-body rendering, rain fog, underwater fog and Hi-Z.
