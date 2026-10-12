# Design

## Camera handoff

`FrameBegin` keeps mcr's *player* camera in its camera fields, because Java's `alignWithEntity` override still
uses them, and gains a render camera plus `FRAME_CAMERA_HANDOFF` for the camera mcr actually renders.

- `alignWithEntity` takes Rust's camera only while the camera entity is the player or the entity the server set
  with `SetCamera` (spectating, which mcr follows itself). Any other camera entity is a mod's, and vanilla's
  alignment runs for it.
- At `GameRenderer.extractCamera` HEAD, after every `Camera.update` mixin and before the level reads the camera,
  Java compares the final camera with the player camera: a foreign camera entity, a position more than 1e-4
  blocks off, a rotation more than 1e-3 degrees off, or a different detached state. A difference is reported in
  the frame state: `CAMERA_MOD`, `CAMERA_DETACHED`, and `CAMERA_INPUT` when `player.input` is no longer a
  `KeyboardInput` (Freecam swaps in a bare `ClientInput`). Position is in blocks, and yaw and pitch use mcr's
  radians.
- Then Java places its camera, re-preparing the cull frustum, at the render camera when the handoff flag is set,
  or else back at the player camera. Java's entities and mcr's terrain therefore always share one camera, and a
  mod camera reaches the screen one frame late. Rendering Java's camera immediately would put entities and terrain
  one frame apart.
- mcr ignores a report with non-finite values. It keeps the position within (render distance + 2) chunks
  horizontally of the player's eye, and within the dimension height plus 64 blocks. The rate of change is not
  clamped, because Freecam toggles and tripods teleport the camera on purpose.
- With `CAMERA_INPUT`, mcr's movement input is empty and mouse motion does not turn the player. Java's
  `GameplayKeys` mirrors the movement mappings from Rust's key bitset instead of releasing them, and
  `turnPlayer` runs, so the mod's camera entity flies and turns.
- Known limit: a mod that changes the camera inside `alignWithEntity` while the player is the camera entity, at
  an injection point that runs before ours and is then overwritten, is not seen. Mixins at `update` TAIL, at
  `setup` or on the camera entity are seen.

## Weather replacement

The bridge used to cancel `WeatherEffectRenderer.extractRenderState` at its head, which hid any mod that
cancelled it too. Now the bridge marks the level's `popPush("weather")` profiler step, and cancels the extraction
right after its first statement, the `intensity` store. "Step reached but the store never ran" means a mod
cancelled the extraction at its head (Particle Rain), redirected the call, or overwrote the method. Java then sets
`FSTATE_WEATHER` and mcr calls `Renderer::set_weather_hidden`, which drops the rain and snow columns but keeps the
rain level's sky and fog darkening. Vanilla's column loop never runs, so this costs one field store per frame.
`prepare`, `render` and `renderOit` stay cancelled. We chose this check over a mod-policy flag because it needs
no list of mods, and it also notices Particle Rain's own setting that keeps vanilla's weather.

## Top-down GUI

In the GUI slot, Java's main target resolves to mcr's scene whenever their sizes match. Pipelines that read the
`Projection` uniform get a vertex variant compiled without naga's Y adjustment (`top_down`), so they draw
top-down. Scissors and clear rectangles mirror to `H - y - h`. Fullscreen passes (blur, post) read and write in
the same row order and need no flip. GUI depth is private, so the GUI's depth clears leave the scene alone. Java
only reads the main target for copies and screenshots. Those reads first flip the current scene into Java's real
`MainTarget` color once (`refresh_main`). Main-target screenshots are deferred until after the GUI slot, so they
see the finished frame. Golden test: the Java screenshot matches the scene at SSIM 1.0000, and the scene matches
the OpenGL golden at SSIM 0.9996.

Cost at 1920x1080 on the title screen, 1200 unpaced frames, three runs each:

| | mcr frame incl. GPU p50 | p99 | GUI compose CPU p50 |
|---|---|---|---|
| Before (two flip copies) | 981 / 759 / 991 µs | 2014 / 1437 / 2120 µs | 249 / 207 / 248 µs |
| After (top-down) | 673 / 645 / 665 µs | 1399 / 1255 / 1410 µs | 88 / 87 / 90 µs |

## Section visibility

`VisibilityGraph::update` records a bit for every section it enters, in its grid box at
`x + z * dx + y * dx * dz`. `ExternalCtx::sections` exposes the bits for the frame's camera, but only while the
graph runs (it is skipped when the camera is inside an opaque block). The render thread copies the bits into a
shared block (`SHARED_SECTIONS`, header plus at most 2^23 bits, 1 MiB) when the search's generation changes. It
does this only between Java frames, while nothing is outstanding, and sets `FRAME_SECTIONS` for frames where the
copy is current. Sets that do not fit are not shared, and Java then keeps its own culling.

Java treats anything outside the box as reached. While `extractVisibleBlockEntities` runs, a bridge section mesh
returns no block entities for an unreached section. `isEntityVisible` additionally rejects an entity when every
section touched by its bounds is unreached. The bounds are inflated by half a block and raised one block. The
entity is always kept if its bounds span more than 4 sections on an axis, if it is leashed, if it carries
passengers, or if it is the player, the camera entity or the player's vehicle. Entities whose renderer opts out
of culling are kept too.

The graph marks a section as soon as it enters it, even when the section's own faces are solid. So a pocket is
unreached only when solid sections surround its sections, and the measurement builds such a shell.

The test is 500 NoAI zombies in an air pocket sealed by a solid shell one section thick, in front of the camera
(`entity_scene::java_frame_time_with_sealed_entities`, 1920x1080, about 35 s per run). It compares Java's own
culling (`-Dmcr.sections=false`) with mcr's sections:

| Run | Java's culling: entities, Java frame p50 / p99 | mcr's sections: entities, Java frame p50 / p99 |
|---|---|---|
| 1 | 500 drawn, 6662 / 25993 µs (2506 frames) | 0 drawn, 1364 / 6567 µs (5845 frames) |
| 2 | 500 drawn, 5025 / 8575 µs (3820 frames) | 0 drawn, 496 / 3062 µs (10125 frames) |

The other agent's builds loaded the machine during both runs, so the absolute numbers vary. The ratio does not.
The live entity probes still pass, including the zombie occluded by the wall.

## Tests

- Rust unit tests: frame-state camera sanitising and bounds, the shared section layout and refusal, reached bits
  matching the search, hidden weather keeping the rainy sky (GPU), and top-down rectangles and pipeline variants.
- The test mod (`java/mcr-testmod`): its camera key `J` cycles a three-block `Camera.update` lift, a free camera
  entity with input capture and `Entity.turn` redirected, and off. `-Dmcr.testmod.replaceWeather=true` cancels
  the weather extraction like Particle Rain does.
- Live tests (`mcr-app/tests/entity_scene.rs`): `mod_cameras_hand_off_to_mcr` and
  `replaced_weather_hides_mcrs_rain`, plus the existing entity probes against the vanilla server.
- Real jars, offscreen against the vanilla server: Freecam 1.5.0-alpha.2 hands its camera over (it flew 20 blocks
  while the player stayed put) and gives it back. Particle Rain 4.0.2 sets the weather flag, and only its
  particles show.
