## Context

Vanilla 26.3 flow:
- **Requesting.** `GameRenderer.update` builds the requested list: `end_of_frame`, then `LocalPlayer.getActivePostEffects()` (set by `ClientboundPostEffectsPacket`, which the `/posteffect` command drives), then the spectated-entity chain while it is active.
- **Loading.** Chains that fail `ShaderManager.isPostEffectValid(id, MAIN_TARGETS)` are skipped silently. This covers `entity_outline`, which needs the outline target, and `end_of_frame`, which only resource packs define.
- **Running.** `GameRenderer.render` runs the chains after `renderLevel` (which draws the 3D HUD and hand) and `blitEntityOutline`, and before the depth clear and `GuiRenderer.render`. Each chain processes `MainTarget` in place.
- **Spectated chain.** `checkEntityPostEffect` picks `creeper` (Creeper), `spider` (Spider, which includes the cave spider) or `invert` (Enderman). It runs:
  - from `setCameraEntity`;
  - with `null` when leaving first person, and with the camera entity when returning to it.

  `keyToggleSpectatorShaderEffects` flips its active flag.

mcr's present modes (`mcr-render` `renderer.rs`):
- **Offscreen**: a sampleable `Rgba8Unorm` texture.
- **Window**: the swapchain view, which cannot be sampled.
- **Scene** (mod mode): an `Rgba8Unorm` scene texture, blitted to the swapchain. `ExternalSubmitter` hooks run between submits.

## Decisions

**Own WGSL, no assets.** Each chain is a static list of passes in Rust: program, input target and filter, auxiliary input, output, and parameters. One WGSL module holds a fullscreen-triangle vertex shader, a rotate-scale vertex shader and one fragment entry per program: box blur, colour convolve, bits/mosaic, invert, blit and spider clip. The constants (blur radii, colour matrix, mosaic size, spider eye layout, tint) describe vanilla's look. The shaders are written from that behaviour, not copied.

**Bottom-up coordinates.** The vertex shader emits vanilla's screen-quad coordinates, which have y up. Every texture tap flips y, because mcr's targets are top-down. Asymmetric passes then match vanilla pixel for pixel: the half-weight tail of the box blur, the mosaic grid anchor and the spider eye placement. The mosaic tap adds half a texel so nearest sampling picks the same texel as GL at cell edges.

**Pass graph.**
- Targets are `Main` plus four pooled slots: `swap`, `temp`, `large_blur`, `small_blur`. Slots are created at screen size in the target format and recreated on resize.
- A chain is valid when:
  - every pass reads only targets already written in the chain (or `Main`);
  - no pass reads its own output;
  - the last pass writes `Main`.

  A unit test checks this for every chain.
- In-place processing of `Main` is safe, because each pass writes a different target than it reads.

**Caching.** Bind groups and per-pass uniforms (256-byte aligned slots in one buffer with dynamic offsets) are rebuilt only when the chain list, the main texture, the size or the menu blur radius changes. Pipelines are built on first use. No allocation per frame while the key is unchanged.

**Present modes.**
- Offscreen and scene targets are processed in place.
- A window without a scene texture switches the frame's colour target to a lazily created intermediate image in the surface view format, but only while chains are requested. It then blits that image with the existing `PresentBlit`. Frames without chains keep rendering straight into the swapchain.

**Ordering.** The world pass and `Overlay::record` (entities, particles, hand), then `after_world`, then the chains, then `before_ui`, then `Overlay::record_ui`, then the present blit. Vanilla's order is the same: level, hand, outline, post, GUI.

**Mod mode.**
- `FrameLink::after_world` keeps waiting for Java and draining the world slots, `WORLD_AFTER` and `POST_WORLD` (Java's hand). It no longer composes the GUI. Instead it records the frame index, and the new `before_ui` hook does the GUI flip, drain and flip back.
- Java's `GameRenderer.applyPostEffects` skips chains whose id is one of mcr's four. This is a redirect on `PostChain.process`. Mod and resource-pack chains still run in Java, under `POST_WORLD`.
- mcr is the single owner of the vanilla chains, so both modes apply them identically and nothing applies them twice.

**Glowing outlines (native).** mcr had none: Java draws them in mod mode, and the native entity path drew nothing.
- `EntityView.outline` carries `getTeamColor()` when vanilla's `shouldEntityAppearGlowing` holds:
  - the glowing shared flag (bit 6) is set;
  - or the viewer is a spectator holding `key.spectatorOutlines` and the entity is a player.

  The team comes from the scoreboard entry: the player name, or the UUID for other entities. It defaults to white.
- The local player's shared flags now keep the server-owned bits (fire, invisible, glowing) instead of rebuilding the byte from physics.
- Every non-additive part of a glowing entity carries the colour in its instance. That covers attachments and free quads, but not shadows or the leash rope.
- `EntityRenderer::draw_outlines` replays the frame's runs with a silhouette pipeline (`vs_outline`/`fs_outline`). Instances without a colour are clipped away. Texels with alpha 0 are discarded, as vanilla's `rendertype_outline` does. The output is a flat team colour.
- `mcr_render::GlowOutline` owns the silhouette target (`Rgba8Unorm` plus a private reverse-Z depth, so silhouettes occlude only each other). It runs vanilla's `entity_outline` chain on it (sobel, two 2-radius blurs whose alpha stays a sum, blit). Then it blends the result onto the world with vanilla's outline blit blend (`SrcAlpha, OneMinusSrcAlpha` for colour, keeping alpha).
- It runs at the end of `Overlay::record`, after the hand, matching `blitEntityOutline` after `renderLevel`, and before the screen chains.

**Hostile input.**
- The packet's list is truncated to 16 ids. Ids are matched against the four known names and anything else is dropped.
- At most 8 chains run per frame: 16 server ids plus the spectated chain could otherwise mean 17 chains of 10 passes.
- The packet in the configuration phase is ignored. Vanilla throws there, because no player exists yet.

## Risks

- A window frame with chains costs one extra fullscreen copy, plus the chain passes. Spider is 10 passes, of which four are 15- and 7-tap blurs.
- Resource packs that override vanilla chains are not followed outside mod mode.

## Verification

Live, against a vanilla 26.3 server with offscreen screenshots:
- `/posteffect add` for blur, creeper, invert and spider: each chain is applied and the UI stays unprocessed. `entity_outline` is ignored.
- `/spectate` a creeper, spider and enderman: the matching chain is applied. F4 turns it off, third person clears it, and first person brings it back.
- Glowing entities are outlined in team colours.

The parity-followups-1 check found and fixed these:
- While spectating, the local player was drawn over the camera. mcr now follows vanilla: the local player is drawn only when it is the camera and the camera is detached.
- Debug charts stacked on the left. They now sit like vanilla's: frame time bottom-left, the others bottom-right.
- `vs_outline` called another entry point, which WGSL forbids.
- Leash knots sat at the fence's block corner. Vanilla's `AddEntity` for a knot carries the block position, which the client snaps onto the post.

Follow-ups:
- Text displays are not drawn.
- The code-of-conduct "do not notify" option is a button, not vanilla's checkbox widget.
- The window present mode and the mod-mode Java skip of the mcr-owned chains were not run live (no windows, mod-mode memory). They are covered by the renderer wiring test only.
