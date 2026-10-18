# Design

## Survey

### mcr's terrain light

`mcr-world` runs the client light engine and the mesher (`mcr-mesh`) bakes light into every quad corner: a packed
`u16` with sky and block light in 1/16-level units (smooth lighting already averaged). Cube quads are greedy-merged
across cells of equal light, so one quad can span 16 blocks. `terrain.wgsl` decodes the corner light in the vertex
shader and looks it up in a 16×16 lightmap uniform (`lightmap[sky * 16 + block]`, rebuilt by `Environment` when the
sky or gamma changes), bilinear in 1/16 steps like sampling vanilla's filtered lightmap texture, then multiplies AO
and the directional face shade. The colour is interpolated to the fragment. Light changes remesh the section.

### LambDynamicLights 4.13.0

One multi-loader jar (`fabric.mod.json` and `neoforge.mods.toml`) with the API, SpruceUI and Yumi nested.

- **Sources.** `LambDynLights.dynamicLightSources` is a set of `DynamicLightSource`. Entities and particles
  implement `EntityDynamicLightSource` through mixins (`getDynamicLightX/Y/Z`, the eye position for entities,
  and `getLuminance` 0–15). Luminance comes from the mod's JSON item and entity light sources and its config
  (held and worn items, burning entities, glowing entities, dropped items, fireflies). `DeferredDynamicLightSource`
  wraps an API `DynamicLightBehavior`: `LineLightBehavior` (guardian beams), `BeaconLightBehavior` and the end
  gateway's beam.
- **Ticking.** `ClientLevel.tickEntities` HEAD/TAIL and per-entity tick hooks update luminance, then the end of
  the level tick rebuilds a spatial hash of 8³ cells and asks a `ChunkRebuildScheduler` to mark the sections near
  moved sources dirty.
- **Injection.** One mixin on `LightCoordsUtil.BrightnessGetter`'s default lambda: for a position whose block is
  not solid-render, the packed light's block field becomes `max(block, dynamic) · 16`. `getDynamicLightLevel`
  takes the maximum over nearby sources of `luminance − distance · 15 / 7.75` (zero past 7.75 blocks), measured
  from the block's centre; a line takes the distance to the segment, a beacon the horizontal distance to the beam
  (plus the vertical distance below its base). Vanilla's section rebuild then bakes it into the mesh.
- **Toggles.** The config's mode (`off`, fastest, fast, fancy, realtime) gates everything: with the mode off the
  engine returns 0 and ticking stops. Self, entity, block entity, water sensitivity and per-source toggles show up
  as luminance 0.

In mod mode Java's sections are meshless, so the rebuilds draw nothing. Entities Java draws do light up, because
their packed light goes through the same `BrightnessGetter`.

## Decisions

### Java reports the mod's sources over the bridge

A native implementation would have to replicate the item and entity luminance tables, data-driven JSON from
resource packs, and every config toggle. Reading the sources the mod already computed keeps all of that the mod's,
and the shader only needs positions and levels. The mod has no public read API (the API registers sources and
behaviours), so `McrDynamicLights` reaches its internals by reflection, once: the `LambDynLights` instance, the
`dynamicLightSources` set, the config's mode, and the methods of `EntityDynamicLightSource` and the API
behaviours. Both loaders load the mod as an open module or unnamed, so private access works. Any failure turns
the channel off for the session with one log line; mcr then simply has no sources.

Per Java frame, in `McrFrame.frameDone` before the frame state is published:

- entity sources: the entity's position lerped to Java's partial tick plus the mod's eye offset; particles their
  tick position; luminance from `getLuminance`, skipped at 0;
- `LineLightBehavior`: the segment and its luminance; `BeaconLightBehavior`: a vertical segment through the block
  centre from its base (or the world bottom) to the world top; the end gateway beam: the same column with the
  gateway's light emission; any other behaviour: a point at the centre of its bounding box with the level the
  behaviour gives there;
- nothing at all while the mode is off.

At most 256 sources are written, nearest to the camera first when there are more.

### ABI 20: a shared light block

`SHARED_LIGHTS` (id 20) is a Rust-owned block of 64-bit words (`lights.rs`), written by Java and read by the
render thread when it completes Java's frame. Layout: `LIGHT_SEQ` (u32 sequence, odd while Java writes),
`LIGHT_FLAGS` (`LIGHTS_PROVIDER`: a dynamic light mod was found and hooked; `LIGHTS_ENABLED`: its mode is on),
`LIGHT_COUNT`, then `LIGHT_MAX` = 256 entries of 40 bytes: start `f64×3`, end minus start `f32×3`, level `f32`.
The read happens after Java signalled the frame done and before the next frame is posted, so it never races in
practice; the sequence lock covers the case anyway (a torn read keeps the previous list). Rust drops non-finite
entries, clamps the level to 0–15 and drops it at 0, clamps the segment to 4096 blocks per axis and keeps at most
`LIGHT_MAX`. `FrameLink::dynamic_lights` returns the list only while Java draws the world.

### Renderer: nearest 64 in a uniform buffer, binned in a grid

`Renderer::set_dynamic_lights` takes world-space sources. At render time it makes them camera-relative (in `f64`,
then `f32`), drops those whose reach box (`level · 7.75 / 15` around the segment) misses the view frustum or the
light grid, and keeps the 64 nearest to the camera. The GPU gets a 2 KiB uniform (a count and 64 entries of
`(start.xyz, level)`, `(end.xyz, 0)`) and a 32 KiB storage buffer: a 16³ grid of 16-block cells centred on the
camera, each with two mask words of the sources whose reach box touches the cell. Both are written only in frames
with sources. A first version looped over every source in every fragment; with the grid a fragment only
evaluates the sources of its cell. For 57 sources spread 12 blocks apart (plus a beacon beam in view) that cut
terrain draws from 0.441 to 0.123 ms at 1080p. Sources packed together still cost about the same, since every lit cell holds most of them.

Shader: a second module (`terrain.wgsl` + `dynlight.wgsl`) with `vs_cube_dyn`, `vs_model_dyn`, `fs_main_dyn` and
`fs_translucent_dyn`, bound with a third bind group. The vertex variants share their decode with the base entry
points and also output the sample point, the interpolated sky level and the AO × face-shade factor. The sample point
is the corner half a block out along the face normal (the shade face for model quads), which is where vanilla
samples the light of a face. The fragment looks up its grid cell, evaluates `level − distance(p, segment) · 15 / 7.75` for each
source in the cell's masks, takes the maximum, and lights the fragment with `max(vertex colour, lightmap(sky, dynamic) · ao · shade)`. Since
the lightmap is monotonic in block light, that equals looking up `max(block, dynamic)`, LambDynamicLights' own
rule. It runs per fragment because greedy quads span many blocks and per-vertex light would miss a torch in the
middle of a floor. The result is continuous rather than per block, close to the mod's look with smooth lighting.

Like the mod, the light has no occlusion and also reaches faces that point away from it.

### Zero cost without a provider

The variant module, pipelines, buffer and bind group are created only when `RendererConfig::dynamic_lights` is
set, which mod mode does. A frame uses the variants only when at least one source survives the culling. Without
mods there is no JVM, no light block reader and no variant pipeline, so the terrain path and its cost are as before.

### Setting

`mcrDynamicLights` (`Options::mcr_dynamic_lights`, default true) is mcr's own key in `options.txt`; vanilla
ignores unknown keys. Video Settings lists "Mod Dynamic Lights" only while Java reported `LIGHTS_PROVIDER`
(`Settings::dynamic_lights_provider`, not saved). Off, the shell passes no sources to the renderer. The mod's own
mode and toggles stay in its own screen and config file.

## Risks

- Internals can change between mod versions. Every lookup is checked once at start; a miss logs and disables the
  channel without affecting the game.
- Many sources close together: a fragment evaluates every source that reaches its cell, at most 64.
- One to two frames of latency for moving sources (Java's previous finished frame). At walking speed that is
  centimetres.

## Measurements

Ryzen 7 5800X, RX 6800 XT, LambDynamicLights with Fabric API (Fabric) or with AppleSkin (so the set boots
NeoForge), 1920x1080 offscreen, flat world at midnight, `--bench 3000` after the scene settled (`MCR_MOD_PROFILE=frame`), GPU medians from mcr's timestamps:

| Scene | Terrain draws | World passes | Java frame |
|---|---|---|---|
| no source | 0.040 ms | 0.085 ms | 0.315 ms |
| held torch (1 source) | 0.063 ms | 0.106 ms | 0.310 ms |
| 64 glowstone items 12 blocks apart (56 drawn) | 0.112 ms | 0.156 ms | 0.323 ms |
| 100 glowstone items within 20 blocks (64 drawn) | 0.508 ms | 0.554 ms | 0.454 ms |
| the 100 items, Mod Dynamic Lights off | 0.050 ms | 0.107 ms | 0.428 ms |
| NeoForge, no source | 0.040 ms | 0.085 ms | 0.306 ms |
| NeoForge, held torch | 0.063 ms | 0.107 ms | 0.323 ms |
| no mods (vanilla path), this change | 0.044 ms | 0.083 ms | |
| no mods, the terrain shader before the change | 0.045 ms | 0.091 ms | |

The two no-mods rows are the same scene with the base terrain shader of this change and of `main`.
Publishing the sources does not show in Java's frame time; the extra Java time with 100 items is their rendering.
