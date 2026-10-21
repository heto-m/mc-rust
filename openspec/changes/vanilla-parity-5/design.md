## Context

The `vanilla-parity-4` close-out re-read each batch against the decompiled 26.3 client. It also collected
the "left over" notes from the batch reports and the Batch 21 notes. This change records what is still
different. Vanilla references are `Class#method` identifiers only. mcr references are paths at commit
`24e0472`. Severity follows `vanilla-parity-4`:

- **G**: gameplay-breaking. A feature does not work, the player is stuck, or the server rubber-bands or
  rejects what the client did.
- **V**: visible. A player notices that it looks, sounds or behaves differently from vanilla.
- **M**: minor. A small or rare difference.

## Decisions

- One task per leftover, grouped into batches that share code, so two agents never edit the same
  function.
- `projectile_power` was verified: `handler.rs` stores the power on the entity and the hurting-projectile
  body accelerates by it, with vanilla's 0.1 default and wind charges at 0. It is not a leftover.
- L28 is a performance item, not a visual one. Its fix must not change a single pixel.

## Re-audit result

All 175 parity-4 gaps have code that follows the cited vanilla method. Spot checks against the decompiled
client confirmed, among others, the collision rules for boats, shulkers and happy ghasts, every
`LevelEventHandler` id (1000 to 3021), a provider for every particle type, all 23 `key.debug.*` mappings
(with the actions listed in L18 still missing), void and boss fog darkening, lodestone compasses,
sleeping poses, tooltip styles, the creeper swell flash and the pause menu icon row. The leftovers below
are what the batch reports and the re-audit found.

## Gap catalogue

### Placement and adventure

#### L1 Block placement prediction covers property-less blocks only (V)
- Vanilla: `BlockItem#place` predicts the state from `Block#getStateForPlacement`, merges slabs through
  `canBeReplaced`, and `BlockItem#canPlace` tests that state's collision against every entity.
- mcr: `try_place` (`crates/mcr-client/src/use_item.rs:471-540`) predicts only blocks without properties,
  tests the default state for obstruction and never merges slabs.
- Impact: stairs, slabs, logs, doors and most building blocks appear one round trip late, and a slab
  placed onto a slab is not predicted.

#### L2 Adventure predicates approximate block entity data (M)
- Vanilla: `BlockPredicate#matches` checks `NbtPredicate` against the block entity's data and the
  `DataComponentMatchers` against its components.
- mcr: `block_predicate_matches` (`crates/mcr-client/src/adventure.rs:80-126`) passes any block that has a
  block entity when the predicate has NBT, and ignores component checks.
- Impact: an adventure player's `can_break` and `can_place_on` items can be predicted as allowed where the
  server refuses, which flickers the block.
- Fix note: `AdventureModePredicate#test` calls `BlockPredicate#matches(BlockInWorld)`, which checks the
  state and NBT only; the component matchers apply to `matches(ServerLevel, BlockPos)`. mcr matches the
  NBT against the client's block entity data plus `id`, `x`, `y` and `z`, and ignores components too.

### Commands and chat

#### L3 Suggestion providers other than ask_server (M)
- Vanilla: `SuggestionProviders` also registers `available_sounds`, `summonable_entities` and
  `post_effects`, which the client fills locally.
- mcr: only `minecraft:ask_server` is handled (`crates/mcr-client/src/commands.rs:13,172`).
- Impact: `/playsound`, `/summon` and post-effect arguments get no local suggestions.

#### L4 Chat acknowledgements ignore the chat delay (M)
- Vanilla: `ChatListener` queues delayed messages and calls `ClientPacketListener#markMessageAsProcessed`
  when a message leaves the queue.
- mcr: `SecureChat::mark_processed` (`crates/mcr-client/src/secure_chat.rs:589`) runs on arrival while the
  display waits in `tick_queue` (`crates/mcr-ui/src/screens/chat.rs`).
- Impact: with a chat delay, last-seen acknowledgements include messages the player has not seen yet.

### Riding and projectiles

#### L5 Trident loyalty ignores dealt damage and ground time (M)
- Vanilla: `ThrownTrident#tick` sets `dealtDamage` after `inGroundTime > 4` or a hit, and returns once
  `dealtDamage || isNoPhysics()`.
- mcr: `tick_arrow` (`crates/mcr-physics/src/body.rs:333-356`) homes in whenever loyalty and an owner
  anchor exist, without either condition.
- Impact: a loyal trident's client-side path can start returning on a different tick than the server's.

#### L6 Attached firework rockets have no hand offset (V)
- Vanilla: `FireworkRocketEntity#tick` places an attached rocket at the holder's position plus
  `LivingEntity#getHandHoldingItemAngle(FIREWORK_ROCKET)`.
- mcr: the anchor is the holder's position (`crates/mcr-client/src/entity.rs:770-817`).
- Impact: an elytra boost rocket and its trail sit in the player's body instead of their hand.

#### L7 Nautilus dash sounds and dash flag (M)
- Vanilla: the nautilus plays its dash and dash-ready sounds, and the synced dash flag gates the jump bar.
- mcr: `jumpableVehicle` handling (`crates/mcr-client/src/riding.rs:328-360`) plays neither and does not
  read the server's dash flag.
- Fix note: both sounds come from the server. `handleStartJump` and the dash-ready countdown run there
  and `makeSound` broadcasts to every player, the rider included; a client-side `makeSound` plays
  nothing. The live check hears `entity.nautilus.dash` and `dash_ready_land` from the server, so only the
  dash flag needed a client change.

#### L8 Happy ghast and nautilus riding loops (V)
- Vanilla: `LocalPlayer#startRiding` plays `RidingEntitySoundInstance` with `entity.happy_ghast.riding` and
  `entity.nautilus.riding`.
- mcr: only minecart riding loops exist (`crates/mcr-app/src/ambience.rs:471-532`,
  `crates/mcr-app/src/entity_loops.rs`).
- Impact: riding a happy ghast or a nautilus is silent apart from mob sounds.

### Entity rendering and particles

#### L9 Elder guardian curse effect not drawn (V)
- Vanilla: `ElderGuardianParticle` renders the elder guardian model in front of the camera.
- mcr: the curse is tracked (`crates/mcr-app/src/entities.rs:555,994`) but nothing appears on screen.
- Fix note: the ghost's offset had the wrong sign and sat behind the camera. Vanilla translates after its
  `(s, -s, -s)` scale, so the model lands 1.49 blocks in front.

#### L10 Elytra ignore the cape texture (V)
- Vanilla: `WingsLayer#getPlayerElytraTexture` uses the skin's elytra texture, else the cape when the cape
  part is shown.
- mcr: the wings always use the item's equipment texture (`crates/mcr-entity-model/src/humanoids.rs:932`).
- Fix note: the wings wear the cape while the cape part shows. mcr fetches no `ELYTRA` profile texture, so
  the skin's own elytra texture is still skipped.

#### L11 Boats show water inside the hull (V)
- Vanilla: `BoatRenderer` submits the water patch model with the water-mask pipeline, which hides water
  inside the hull.
- mcr: no water mask (`crates/mcr-entity-model/src/misc.rs:689`).
- Impact: boats on water look sunk.
- Fix note: the `boat#water_patch` part draws with a depth-only pipeline in the opaque pass, skipped for
  rafts and while the boat's eye is under water (which also stops the bubble wobble, as vanilla does).

#### L12 Smoke particles at the camera draw as a dark square (V)
- Vanilla: smoke and campfire smoke near the camera stay soft translucent sprites.
- mcr: a smoke particle reaching the camera draws as a dark square (`crates/mcr-particles`).
- Fix note: campfire smoke started at alpha 1, so one opaque texel filled the view. It now starts at 0.9
  (cosy) or 0.95 (signal), and translucent particles discard below vanilla's 0.1 alpha. Plain smoke stays
  dark like vanilla's (`random * 0.3` grey). Particles still ignore fog.

#### L13 Map labels drawn after all icons (M)
- Vanilla: `MapRenderer#render` draws each decoration and then its name, so later icons cover earlier
  names.
- mcr: `crates/mcr-entity-model/src/maps.rs` bakes all names over the icons.
- Fix note: vanilla draws every name at one depth, level with the 36th icon (`-0.02 - 0.003 * 35`
  against `-0.125`), so names cover the first 36 icons and later icons cover names. mcr keeps the baked
  label layer and puts it at that depth.

### First-person spear feedback and spectator menu

#### L14 Spear hit feedback dip missing (M)
- Vanilla: `SpearAnimations` lowers and pushes the first-person spear by `hitFeedbackAmount` after a
  kinetic hit.
- mcr: `crates/mcr-app/src/hand.rs` has no hit feedback.

#### L15 Spectator team page order and icons (M)
- Vanilla: `TeleportToTeamMenuCategory` and `TeleportToPlayerMenuCategory` order entries and pick icons
  their own way.
- mcr: `crates/mcr-client/src/spectator.rs` sorts teams and orders icons differently.

### Screens and debug keys

#### L16 Pause menu lacks World Options (V)
- Vanilla: `PauseScreen#createPauseMenu` shows Options and World Options side by side.
  `WorldOptionsScreen` holds difficulty, game rules, default and personal game mode, allow commands and
  the multiplayer section (LAN).
- mcr: Options and Open to LAN (`crates/mcr-ui/src/gui/menus.rs:507`, `crates/mcr-ui/src/gui/lan_screen.rs`),
  and no World Options screen.

#### L17 Pack screen without drag and drop or icons (M)
- Vanilla: `PackSelectionScreen` accepts dropped pack files and shows each pack's `pack.png`.
- mcr: `crates/mcr-app/src/packs.rs` lists packs without icons and has no drop target.

#### L18 Missing debug chords (M)
- Vanilla: `KeyboardHandler` handles F3+C held 10 s (crash), F3+F6 (debug options), F3+S (dump
  dynamic textures), F3+L (profiling), F3+1 (profiling chart) and F3+4 (lightmap texture).
- mcr: `DebugAction` (`crates/mcr-app/src/debug_keys.rs`, `crates/mcr-app/src/play.rs:2717-2830`) has none
  of these.

#### L19 Debug chords ignored while a screen is open (M)
- Vanilla: `KeyboardHandler#keyPress` falls through to the debug keys when the screen does not consume the
  key.
- mcr: chords fire only without a screen.

#### L20 No autosave indicator in local worlds (M)
- Vanilla: `Hud` fades the saving indicator in while the local server saves, when
  `showAutosaveIndicator` is on.
- mcr: the option is stored (`crates/mcr-app/src/options_pages.rs:624`) but nothing draws it. The managed
  server's save has to be read from its console.

### Level event and mod-mode sounds

#### L21 Distance-delayed level events play at once (M)
- Vanilla: `LevelEventHandler` plays the trial spawner, vault and cobweb sounds of events 3012 to 3021 with
  `distanceDelay`, so far ones arrive later.
- mcr: `crates/mcr-app/src/level_events.rs` plays them immediately.

#### L22 stop_sound not Java-owned in mod mode (M)
- Vanilla: one sound manager owns every sound.
- mcr: in mod mode Java plays the sounds it owns, but `stop_sound` is missing from that list
  (`event_owner`, `crates/mcr-modbridge/src/sound/owner.rs:13`), so it cannot stop Java's sounds.

### World rendering

#### L23 improvedTransparency (Fabulous) not implemented (V)
- Vanilla: `improvedTransparency` renders translucent terrain, particles, weather, clouds and item entities
  into separate targets and composites them with the `transparency` post chain.
- mcr: stored only (Batch 21 notes in the archived `vanilla-parity-4`). The F3 toggle flips the option.
- Impact: water behind glass and particles behind water sort wrong compared with Fabulous.

#### L24 Camera overlay textures sampled nearest (M)
- Vanilla: `Hud#extractTextureOverlay` (pumpkin blur, powder snow) uses a linear sampler.
- mcr: overlays (`crates/mcr-app/src/overlays.rs`) go through the GUI renderer's nearest sampler
  (`crates/mcr-ui/src/renderer.rs:193-194`).
- Impact: the pumpkin blur shows hard texels when scaled.

#### L25 Texture filtering NONE differs at mip seams (M)
- Vanilla: linear filtering with the `sampleNearest` shader trick.
- mcr: hardware nearest sampling. The two differ only at mip seams.

#### L26 Beacon levels refreshed every 20 ticks (M)
- Vanilla: `BeaconBlockEntity#tick` updates the base every 80 ticks.
- mcr: `BEACON_INTERVAL = 20` (`crates/mcr-app/src/block_entities.rs`).
- Impact: a beam appears or disappears up to 3 s earlier than vanilla after editing the pyramid.

#### L27 Model-less block entities crack along their outline (M)
- Vanilla: block entity renderers apply the crumbling overlay to their own model parts.
- mcr: chests, signs and other block entities without a baked model fall back to their outline boxes
  (`crates/mcr-render/src/effects/destroy.rs`).

#### L28 Leaves behind terrain are shaded (M, performance)
- Vanilla: draws the SOLID layer before CUTOUT, so leaves behind terrain fail the early depth test.
- mcr: solid and cutout quads share one discarding pipeline, and sections draw in cull order
  (`crates/mcr-render/src/shaders/terrain.wgsl`, `cull.wgsl`). Under a dark forest canopy the vanilla leaf
  faces cost 6% of GPU time with culling on and 15% with it off (archived `vanilla-parity-4` close-out
  notes).
- Fix direction: draw solid quads first, or split cutout quads into their own draws, then measure. The
  picture must not change.

## Severity totals

| Group | G | V | M | Total |
|---|---|---|---|---|
| Placement and adventure | 0 | 1 | 1 | 2 |
| Commands and chat | 0 | 0 | 2 | 2 |
| Riding and projectiles | 0 | 2 | 2 | 4 |
| Entity rendering and particles | 0 | 4 | 1 | 5 |
| Spear feedback and spectator menu | 0 | 0 | 2 | 2 |
| Screens and debug keys | 0 | 1 | 4 | 5 |
| Level event and mod-mode sounds | 0 | 0 | 2 | 2 |
| World rendering | 0 | 1 | 5 | 6 |
| **Total** | **0** | **9** | **19** | **28** |

## Live checks

Against a vanilla 26.3 server on port 25585 (offline, flat world):

- Batches 1 to 3: `live_place` (12 placements, every predicted state equals the server's), `live_suggest`
  (local `summon` and `playsound` suggestions) and `live_riding` (server dash and `dash_ready_land` sounds,
  loyal trident leaves the ground after 5 ticks) pass.
- Batch 4: an offscreen client run shows the curse ghost fading over the view after
  `/particle elder_guardian`, oak and chest boats on water with dry hulls, the elytra on a player in
  third person, and translucent grey campfire smoke with the camera inside a signal fire column. The cape
  and the 36-icon map order are covered offscreen only: offline players have no cape, and named map
  markers need 37 banners.
