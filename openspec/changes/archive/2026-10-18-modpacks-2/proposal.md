# Proposal

## Why

modpacks-1 got Echoes of the Sift and Medieval Immersed playable, with gaps: modded fluids (Echoes' ichor,
Medieval's eleven fluids) did not render in mcr's terrain, blocks with code-generated or loader-specific models
(Easy Anvils, Visual Workbench, Kaleidoscope Tavern's string lights and pressing tub) were invisible, packets
carrying modded data components or particles were dropped, Medieval's singleplayer join showed "Mods stopped
drawing" for about two seconds, and the join rebaked Rust's model cache for 1 to 1.3 s. Medieval also enables
resource packs (Alacrity 32x and others) that Rust's terrain ignored. The owner also wants the JVM heap to grow
with the mod set instead of a fixed 2048 MiB.

## What Changes

- **Render report from Java (ABI 22).** Java exports its fluid models (still, flowing and overlay sprites, layer,
  tint) at the title screen and, on request (`MSG_MODELS`), the baked quads of blocks Rust cannot bake, plus the
  resource packs it has selected. Rust meshes modded fluids with its own fluid mesher and these blocks with its
  own quad path, cached per state.
- **Modded wire shapes.** Java describes the stream codecs of modded data component types, particle types and
  entity data serializers structurally (built from vanilla's factories). Rust steps over those values and keeps
  their bytes, so item stacks, particles and entity data with modded types decode instead of dropping the packet.
- **Loading screens.** Java flags frames drawn under loading screens; a slow one keeps the last composite instead
  of detaching (the JEI start on join takes about 2 s in one frame).
- **Prebake at the title.** Rust bakes the model cache for Java's title registries in the background and re-indexes
  it for the server's (Fabric's registry sync renumbers modded blocks), so the join skips the bake.
- **Java's resource packs in Rust's terrain**, inside the game dir only.
- **Smaller model cache.** Identical compiled quad runs are stored once; sprites of sizes beyond the renderer's four
  texture groups are resampled instead of rendering as missing.
- **Automatic JVM heap** from the enabled client mod count, calibrated on both packs' live heap, at least
  2048 MiB, at most 12288 MiB and what the machine holds next to mcr's process; a chosen heap still wins.

## Out of scope

Codecs written as a mod's own class (Kaleidoscope Cookery's three component types, Trinkets' attribute
modifiers, Better Combat's twelve slash particles): those packets are still dropped and counted. A UI switch back
to the automatic heap once a heap was chosen. Big-pack runs (300+ mods) with a large heap.

## Capabilities

- `mod-registries`: Java models and fluids, wire shapes, prebake, Java's resource packs
- `mod-frame-compose`: loading screens keep the last frame
- `model-baking`: shared quad runs, sprite size classes
- `mod-manager`: automatic heap
- `launcher`: profiles keep an automatic heap
