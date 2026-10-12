## Context

`mod-polish-1` cancels Java's resource-pack push and pop handlers while bridged. `mod-entity-render-hooks` drops
every `minecraft:*` particle Java creates and leaves Java's fog on its own `renderDistance`. `mod-compat-sweep-1`
found that Controlling's rebinding of vanilla keys has no effect on mcr.

## Decisions

- **One channel for settings.** Rust-to-Java settings ride the control ring the frame link already writes.
  `MSG_KEYS` and `MSG_PACKS` are queued on `FrameLink` and pushed under the writer lock ahead of the next
  `MSG_INPUT`/`MSG_FRAME`, so Java applies them before that frame's input and ticks. A full ring keeps them for the
  next frame. Nothing is sent while Java boots.
- **Key mappings.** mcr's `options.txt` stays the persisted source for vanilla mappings. `MSG_KEYS` carries
  `name\tkey` lines for every vanilla mapping mcr knows. Java sets those `KeyMapping`s, calls `resetMapping` and
  remembers what it applied. After `KeyMapping.resetMapping` (every vanilla and Controlling rebind) and
  `Options.save`, Java diffs the synced mappings against what it last applied or reported and sends the changes as
  `REPORT_KEYS`. Before the first `MSG_KEYS` Java reports nothing, so stale lines in Java's own `options.txt`
  never overwrite mcr's. mcr applies a reported key only for a known mapping and key name, then saves its file.
  The `MSG_KEYS` that follows matches what Java has, so nothing echoes. Mod mappings are never in the table.
- **Server packs.** After mcr's resource reload succeeds, the shell sends the active pack files as `MSG_PACKS`
  (absolute paths, server push order). Java diffs them against what it loaded. Popped ids go to
  `DownloadedPackSource.popPack`. New ones go to `allowServerPacks` and `pushLocalPack(id, path)`, with a
  name-based UUID of the path. Java's pack manager then reloads from the local file, so there is no second
  download and no prompt (the push handler stays cancelled). Any `ServerboundResourcePackPacket` that Java's
  feedback sends is dropped by the outbound filter as before; only mcr answers. `cleanupAfterDisconnect` forgets
  the loaded set.
- **Render distance.** `FrameBegin` gains `FRAME_RENDER_DISTANCE` (mcr's world render distance, the one its fog
  uses) and `FRAME_ENTITY_SCALE`. While Java renders in lockstep, `Options.getEffectiveRenderDistance` returns the
  synced distance (2–64). That drives Java's fog, `Entity.setViewScale`, culling distance and section radius.
  `entityDistanceScaling` is set to mcr's value (0.5–5).
- **Particles.** `McrParticles` keeps a depth counter of mirrored scopes:
  - the packet drain in `McrNet.pump`
  - `ClientLevel.tickNonPassenger`, `Level.tickBlockEntities`, `ClientLevel.tick` and `ClientLevel.animateTick`
  - `GameRenderer.tick` and `ParticleEngine.tick`

  A custom payload's `handle` opens a mod scope inside the drain, because mod payload handlers are mod code.
  `minecraft:*` particles are dropped only inside a mirrored scope and outside a mod scope. Depths reset every frame
  in case a handler threw. Fabric's world tick events fire at `tickEntities` HEAD and TAIL, outside the per-entity
  scope, so they keep their particles.
- **Menu backdrop.** With a panel, `ConfirmScreen` fills the whole screen at `0xF0101010` instead of the
  translucent dim gradient. The panel itself passed width and height to the corner-based `fill_rect`, so it
  ended above the buttons; it now spans the title and the buttons.
- **Lock keys.** On Linux, the brightness of `/sys/class/leds/*::capslock` and `*::numlock` seeds the toggles when
  the mod host starts. Other platforms start off, as before.
- **Quiet refusals.** A log4j filter goes on every logger config, installed at `preLaunch` and again at client
  init. It denies events whose message or throwable chain (8 deep) carries the proxy's 403 refusal. In their place
  it logs one line per logger name (at most 64) saying the host is not allow-listed. The allow-list is unchanged.

## Findings

- authlib's `discovery.minecraftservices.com` lookup is refused by the proxy too (it is not among the Mojang
  hosts); it is now one quiet line like the mods' update checks.

## Not done

- Lock-key seeding on macOS and Windows: no cheap API without new dependencies.
- Particles that a mod mixes into vanilla entity, level or packet code paths are treated as vanilla and dropped.
