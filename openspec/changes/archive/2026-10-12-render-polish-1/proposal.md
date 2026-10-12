## Why

Playtest round 3 left three vanilla rendering mismatches open: non-living entity models other than minecarts, boats and arrows were never checked for the living-entity model drop that made them float, creepers show no swell flash, and leaf mips do not use vanilla's darkened blend.

## What Changes

- Non-living renderers: leash knot, llama spit, shulker bullet, wind charges, trident and wither skull are posed by their own vanilla pose stacks instead of the living model drop. Shulker bullets tumble with a translucent shell, wind charges spin and scroll, wither skulls follow their yaw and pitch, and firework rockets lose a stray 0.125 lift.
- Creepers: client-side swell progress drives vanilla's scale wobble and the white overlay on alternate tenths.
- `dark_cutout` textures (leaves) use `MipmapGenerator.darkenedAlphaBlend` and `fillEmptyAreasWithDarkColor`.

## Impact

- Affected specs: `entity-rendering`, `texture-arrays`.
- Code: `mcr-entity-model` (plans, `Plan.pre_offset`, `Part.scale`, part flash), `mcr-client` (creeper swell), `mcr-app` (view input), `mcr-assets` (mip blend).
