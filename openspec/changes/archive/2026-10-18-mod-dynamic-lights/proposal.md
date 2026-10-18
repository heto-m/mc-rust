# Proposal

## Why

In mod mode mcr meshes and lights the terrain itself, so LambDynamicLights only lights the entities Java draws: a
held torch, a dropped glowstone item or a burning mob leaves the ground dark. The compat scoreboards list the mod
as partial for that reason.

## What Changes

- **Java reports the mod's sources.** Each Java frame the bridge reads LambDynamicLights' own active light sources
  (entities, particles and its deferred behaviours: guardian beams, beacon and end gateway beams), so the mod's
  config decides which items and entities glow, how bright, and whether dynamic lighting is on at all. Sources
  are points or segments with a light level 0–15, in world coordinates, interpolated to Java's partial tick.
- **ABI 20.** A Rust-owned shared block (`SHARED_LIGHTS`) holds up to 256 sources behind a sequence lock, plus
  flags that say a provider is present and enabled. Rust treats it as untrusted: counts are bounded, values
  clamped, non-finite entries dropped.
- **Terrain shader term.** mcr keeps the nearest 64 sources whose reach touches the view frustum, camera-relative,
  in a small uniform buffer. Variant terrain pipelines (cube, model, translucent) raise each fragment's block light
  to `max(block, level − distance · 15 / 7.75)`, LambDynamicLights' own falloff, before the lightmap lookup.
  No section is remeshed.
- **Zero cost without it.** The variant pipelines exist only in mod mode and are bound only in frames with at
  least one source; otherwise the terrain path is the same as today.
- **Setting.** Video Settings shows "Mod Dynamic Lights" while a provider is loaded (`mcrDynamicLights` in
  `options.txt`, on by default); off, mcr ignores the sources.
- LambDynamicLights leaves the partial list.

## Out of scope

Coloured light, other dynamic light mods (the channel is generic, only LambDynamicLights is read), lighting of
particles, clouds and other mcr-drawn effects, light occlusion (the mod has none either), the vanilla client
without mods (vanilla has no dynamic lights), and suppressing the mod's Java-side section rebuilds.

## Capabilities

### New Capabilities
- `mod-dynamic-lights`: dynamic light sources from a Java mod lighting mcr's terrain.

### Modified Capabilities

## Impact

`java/mcr-bridge` (`McrDynamicLights`, ABI constants), `mcr-modbridge` (ABI 20, shared light block, frame link),
`mcr-render` (dynamic light buffer, variant pipelines, shader), `mcr-settings` and `mcr-app` (setting, shell
wiring), mod policy and docs.
