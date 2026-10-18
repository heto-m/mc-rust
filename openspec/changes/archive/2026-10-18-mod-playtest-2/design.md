## Context

`mcr-render` stages section geometry in a CPU upload ring (`Uploader`). Each `set_section` allocates new arena
ranges, frees the old ones, and queues one atomic batch: the quads, the tint table and the 64-byte section
meta. `flush_uploads` writes whole batches at the start of a frame until 64 MiB are written; the rest waits
for later frames, in order. Opaque draws come from the GPU cull pass, which reads the section meta from the
GPU, so they stay consistent with the data actually written. Translucent draws are sorted on the CPU from
`SectionSlot::translucent` and written straight to `trans_draws` each frame.

## Decisions

### Translucent draws commit on flush

`set_section` used to store the new translucent range, tint and slot in the `SectionSlot` immediately. When the
section's batch was left for a later frame, `build_translucent` already drew the new range: arena memory that
was never written (zeros, so degenerate quads and the water vanishes), or memory freed by another section that
still holds that section's quads, with a tint table index whose table was not uploaded (zero, so black water,
or another biome's colour). It lasts until the batch is written, so a long render-loop stall during heavy streaming
could make distant water blink.

Batches now carry an optional commit: the section key, a serial and the translucent draw to use once the batch
is written. `Uploader::flush` reports the commits of the batches it wrote, and the renderer applies them, in
order, before it builds the translucent list. `SectionSlot` keeps the allocation it owns (`translucent`, for
freeing) apart from what is drawn (`drawn`). A new slot draws nothing until its first commit; a replaced slot
keeps drawing the old range. Commits whose serial does not match the slot (removed and re-added since) are
ignored.

Keeping the old range drawable after it was freed is safe because uploads are written in queue order: a range
freed by batch *n* can only be reused by a batch queued after *n*, so it is overwritten no earlier than the
flush that also applies commit *n*, and all of a frame's writes land before its submit.

Measured offscreen at render distance 32 while flying over an ocean by teleport (162 hops of 12 blocks) and
while holding still as chunks streamed in, with and without the play mods, the largest frame flushed under
4 MiB and no frame left uploads for the next one: the 3 ms per-frame `set_section` budget keeps uploads far
below 64 MiB. The race is real but needs a long stall of the render loop, so it is unlikely to be what the
owner saw.

### Animated cutout mips share one coverage scale

Consecutive screenshots of a still camera over the ocean (80 frames, one tick apart) showed distant kelp
forests, seen through the water, thinning out to scattered texels for one or two frames about every
13 ticks and coming back, in plain mcr and in mod mode alike. `TextureArrayBuilder` searched the cutout alpha
scale separately for every animation frame. Frame 0 of `kelp_plant` ends with no texel above the cutout at
mips 3 and 4, and frame 18 at mip 3, while its other frames stay solid, so kelp hundreds of blocks away
vanishes on those frames. Vanilla runs `MipmapGenerator` over the whole animation strip (`SpriteContents`
passes the original image), which gives every frame of a level the same coverage target and alpha scale;
emulating that keeps every `kelp_plant` frame covered at mips 3 and 4. The builder now measures the base
coverage over the strip and rescales each mip level over the strip as well (frames are consecutive layers,
so the layers of one sprite form the vertical strip in memory). With the change the same capture shows no
16-pixel block changing by more than 30 between consecutive frames, against 4 blocks in 11 of 79 frames
before.

Other causes checked:

- **Depth precision.** Terrain uses reverse-Z with an infinite far plane into `Depth32Float`; distant water and
  the sea floor are metres apart, far above its resolution. Not a cause.
- **Sorting.** Sections sort back to front by centre distance; quads inside a section keep mesh order, and a
  water section's top faces never overlap each other from above. The cap of 16384 translucent sections is far
  above what render distance 32 reaches. Not a cause.
- **Merged water tops.** Greedy 3x3 merging of uniform cells produces the same vertex grid on both sides of a
  section border and inside it (0, 3, …, 15, 16), so a flat ocean has no T-junctions. Shores can, but a crack
  there is a single sub-pixel at an edge, not a blink.
- **Mips.** `fs_translucent` samples with explicit gradients taken before any discard, the sampler repeats per
  array layer, and the result is static per frame apart from the animation blend.
- **Culling.** The CPU (`beyond_render_fog`) and GPU (`beyond_fog`) tests agree, and the translucent occlusion
  cull reads the current frame's late visibility. A slot whose meta is not written yet is invisible to it,
  which hid brand-new sections but not replaced ones.

### Difficulty through mcr's server

Vanilla's World Options enable the difficulty button and lock for a player with gamemaster permission or the
integrated server's owner, and send `ServerboundChangeDifficultyPacket`/`ServerboundLockDifficultyPacket`. A
dedicated server accepts them only from gamemasters, and Rust dropped both as not on the forward list.

- A mixin on `DifficultyButtons.updateDifficultyButtonsState` treats the player as the owner while
  `McrWorlds.local()`. Vanilla's hardcore and locked branches still apply.
- `ClientCommonPacketListenerImpl.send` reroutes both packets to `REPORT_WORLD` in a local world:
  `difficulty\t<id>` and `lockdifficulty`. Elsewhere they go through the connection, and both are now on the
  forward list, so a remote operator's changes work.
- Rust refuses requests without a running local session, in a hardcore world, or once locked. A change goes to
  the console as `difficulty <name>`; when the player has gamemaster permission Rust sends the vanilla packet
  instead, because the console's admin broadcast would print a chat line to operators.
- The dedicated server has no lock command and applies `server.properties`' difficulty at start ignoring the
  lock, so a lock cannot reach the running server without op. Rust keeps a lock flag in the session, sends
  `WORLD_LOCKED` to Java, and after the server has exited sets `difficulty_settings.locked` in `level.dat`
  (keeping `level.dat_old`), also when mcr quits with the world open (the session stops and waits for the
  server before writing). From the next start the server itself reports the lock.
- Java keeps `McrWorlds.difficultyLocked` until the session ends and ORs it into
  `ClientLevelData.setDifficultyLocked`, so later server difficulty packets (respawn, dimension change) cannot
  unlock the screen. On `WORLD_LOCKED` it sets the level data and refreshes a `HasDifficultyReaction` screen.

## Risks

- A LAN guest's client does not see the in-session lock; the server does not know it until the next start.
- If the server process is killed so hard that it never exits, the lock is not written.
