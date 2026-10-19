## 0. Change

- [x] 0.1 Proposal, design, specs and tasks; `openspec validate vanilla-parity-4 --strict`

## 1. Containers that do not work

Scope: mcr-app view and play, mcr-client inventory, mcr-ui container screens.

- [x] 1.1 C1 (G) Horse, donkey, llama and nautilus inventories never open
- [x] 1.2 C2 (G) Inventory key while riding opens the player inventory
- [x] 1.3 C3 (G) Villager trade list always empty
- [x] 1.4 C4 (G) Stonecutter recipe list always empty
- [x] 1.5 C5 (G) Loom shows no patterns
- [x] 1.6 C6 (G) Anvil rename never syncs on item placement
- [x] 1.7 C7 (G) Crafter screen corrupts the client's hotbar and inventory
- [x] 1.8 C10 (V) Enchanting table clue tooltip missing
- [x] 1.9 C11 (V) Ghost item stays on the cursor after closing a container
- [x] 1.10 C14 (V) Anvil cost not red when unaffordable
- [x] 1.11 C17 (M) Merchant double-click collects
- [x] 1.12 C20 (M) StateView leaves spectator, trims and score at defaults
- [x] 1.13 C21 (M) Crafter F-swap toggle
- [x] 1.14 Unit tests: mount open event builds the mount menu, crafter mirroring keeps the hotbar, anvil rename emits the packet, StateView returns merchant, stonecutter and loom data
- [x] 1.15 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 2. Sleeping and respawn state

Scope: mcr-client player and handler, mcr-physics pose, mcr-app camera and hand, mcr-ui in-bed screen.

- [x] 2.1 S1 (G) Sleeping is not modelled; a player in bed cannot leave it
- [x] 2.2 N5 (V) Immediate respawn breaks after the first respawn
- [x] 2.3 N6 (V) Respawn data-kept flags ignored
- [x] 2.4 Unit tests: sleeping blocks input and Leave Bed sends STOP_SLEEPING, respawn keeps show_death_screen and the flagged attributes and data
- [x] 2.5 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 3. Use and attack loop

Scope: mcr-app play gameplay loop, mcr-client use_item and actions.

- [x] 3.1 I1 (G) Spears (piercing and kinetic weapons) do not work
- [x] 3.2 I2 (G) Items used by the server but not tracked never send RELEASE_USE_ITEM
- [x] 3.3 I7 (V) Attacking and mining while using an item
- [x] 3.4 I8 (V) UseItem and UseItemOn repeat every 4 ticks while in use
- [x] 3.5 I9 (V) Survival mining timing fixed at dig start
- [x] 3.6 I14 (V) Attack strength ticker not reset on item change or abort
- [x] 3.7 I15 (V) No miss penalty
- [x] 3.8 I19 (M) Attacking and using while rowing
- [x] 3.9 I20 (M) Holding use on an entity does not repeat
- [x] 3.10 Unit tests: spear left-click sends STAB, release sent for server-flagged use, no attack while using, per-tick progress restarts on item change, miss lockout
- [x] 3.11 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 4. Targeting, adventure and placement

Scope: mcr-app target, mcr-client actions, use_item and digging.

- [x] 4.1 I3 (G) Entity interactions send location (0,0,0)
- [x] 4.2 I4 (G) Adventure mode can never mine, even with can_break
- [x] 4.3 I5 (V) Entity reach is 4.5 in survival; range attributes ignored
- [x] 4.4 I6 (V) Entity pick boxes use the default type size
- [x] 4.5 I10 (V) Block placement prediction incomplete
- [x] 4.6 I11 (V) Use-without-item blocks missing from the consume table
- [x] 4.7 I13 (V) Pick block covers blocks only, without Ctrl data
- [x] 4.8 I18 (M) Buckets and boats aimed at fluid
- [x] 4.9 I21 (M) No interaction-shape override in the raycast
- [x] 4.10 I22 (M) A reverted prediction does not snap the player back
- [x] 4.11 I23 (M) Hotbar scroll moves several slots and keeps the accumulator
- [x] 4.12 Unit tests: attribute reach, scaled and baby pick boxes, relative interact location, can_break and can_place_on, consume table, interaction shape face
- [x] 4.13 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 5. Physics collisions

Scope: mcr-physics collide, mover and world; mcr-client state.

- [x] 5.1 P1 (G) No collision with entities
- [x] 5.2 P2 (V) World border is not solid
- [x] 5.3 P3 (V) Entities never push the local player
- [x] 5.4 P4 (V) No push-out of suffocating blocks
- [x] 5.5 P9 (V) Scale attribute ignored for the player's box and eye height
- [x] 5.6 P19 (M) Attribute values not sanitised
- [x] 5.7 P22 (M) Floating bottom scaffolding cannot be stood on
- [x] 5.8 Unit tests: landing on a boat box, border stop, push velocity, push-out from a block, scaled box, hostile step height, unstable scaffolding; criterion bench for collide with entity boxes
- [x] 5.9 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 6. Invisible entities and entity render state

Scope: mcr-entity-model scene and equipment, mcr-app entities.

- [x] 6.1 E1 (G) Invisible entities are drawn with their full body
- [x] 6.2 E7 (V) Burning entities show no fire
- [x] 6.3 E8 (V) Third-person held items never show use state; crossbow pose frozen
- [x] 6.4 E10 (V) Local player does not sit when riding in third person
- [x] 6.5 E15 (M) Upside-down names and shaking
- [x] 6.6 E16 (M) Walk animation for passengers and babies
- [x] 6.7 E17 (M) Armour flashes red when hurt
- [x] 6.8 E18 (M) Shadows: no distance fade or light term; option ignored
- [x] 6.9 E21 (M) Flat dropped-item stacks scattered in 3D
- [x] 6.10 Offscreen tests: invisible player draws held items only, spectator translucency, bow pull on a remote player, shadow fade
- [x] 6.11 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 7. Effect vision and fog

Scope: mcr-render environment, mcr-app effects.

- [x] 7.1 R1 (G) Mob effects never change vision: night vision, conduit power, blindness, darkness
- [x] 7.2 R2 (V) No lava or powder snow fog
- [x] 7.3 R3 (V) Underwater fog colour, biome and ramp wrong
- [x] 7.4 R4 (V) Biome sky and fog colours ignored
- [x] 7.5 R5 (V) Rain fog missing
- [x] 7.6 R6 (V) Superflat draws a black lower sky
- [x] 7.7 R7 (V) End sky flashes missing
- [x] 7.8 R14 (M) Fog colour details
- [x] 7.9 R18 (M) Block light does not flicker
- [x] 7.10 R25 (M) Environment chosen by level name
- [x] 7.11 Unit tests: lightmap with night vision and darkness, fog distances per environment, water vision ramp, biome sky colour, flat horizon
- [x] 7.12 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 8. Riding and vehicles

Scope: mcr-client riding and handler, mcr-physics vehicle.

- [x] 8.1 P11 (V) Ridden pigs and striders step only 0.6 blocks
- [x] 8.2 P12 (V) Ridden horses and camels sink in deep water
- [x] 8.3 P13 (G) Saddled nautilus cannot be steered
- [x] 8.4 P14 (V) Turning a boat does not turn the camera
- [x] 8.5 P15 (M) Ridden mob air steering speed
- [x] 8.6 P23 (M) Boats skip block effects
- [x] 8.7 P24 (M) Camel dash during cooldown and camel sprint
- [x] 8.8 P25 (M) Vehicle fluid current and moves while the camera is detached
- [x] 8.9 N7 (V) Own vehicle rubber-bands on position sync
- [x] 8.10 N8 (M) Controlled vehicle ignores server velocity and bubble-column events
- [x] 8.11 Unit tests: nautilus steering sends MoveVehicle, pig step 1.0, boat yaw follows delta rotation, position sync ignored for the driven vehicle
- [x] 8.12 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 9. Spectator

Scope: mcr-app play and hud, mcr-client actions.

- [x] 9.1 S16 (G) Spectator hotbar menu missing
- [x] 9.2 I12 (V) Spectator interaction missing
- [x] 9.3 Unit tests: spectator menu pages and teleport packet, spectate action on left-click, scroll changes fly speed
- [x] 9.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 10. Level events and client sounds

Scope: mcr-app play, effects and audio, mcr-block-entity anim, mcr-audio.

- [x] 10.1 N1 (V) Almost every level event is dropped in native mode
- [x] 10.2 N2 (V) Pistons do not animate; pushed blocks vanish while moving
- [x] 10.3 N3 (M) Note blocks show no note particle
- [x] 10.4 S2 (V) Lightning makes no sound
- [x] 10.5 S3 (V) Stop-sound packet ignored
- [x] 10.6 S17 (M) Entity-bound sounds do not follow the entity
- [x] 10.7 S18 (M) Bubble column inside sounds missing
- [x] 10.8 S19 (M) Sounds keep playing while a local world is paused
- [x] 10.9 S20 (M) Sound packet seed ignored
- [x] 10.10 I16 (V) Own eating and drinking makes no particles or sounds
- [x] 10.11 I17 (V) Item sounds that only the user's client plays are missing
- [x] 10.12 H15 (V) Subtitles missing
- [x] 10.13 Unit tests: a table test mapping each level event id to its sound or particle, piston moving block progress, stop_sound by name and source, entity-bound sound follows and stops
- [x] 10.14 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 11. Item tooltips, bundles and screen extras

Scope: mcr-ui gui, items and tooltip, mcr-app view.

- [x] 11.1 C8 (V) Item tooltips show only the base item name
- [x] 11.2 C9 (V) Bundle UI missing
- [x] 11.3 C12 (V) Status effects list missing from inventory screens
- [x] 11.4 C13 (V) Screen previews missing: smithing armour stand, enchanting book, cartography map
- [x] 11.5 C15 (V) Smithing cycling slot icons and onboarding tooltips missing
- [x] 11.6 C16 (M) Quick-craft preview details
- [x] 11.7 C18 (M) Lectern buttons
- [x] 11.8 C19 (M) Screen keys hard-coded
- [x] 11.9 Unit tests: tooltip line order for enchanted, potion, dyed, renamed and attribute items with tooltip_display; bundle selection packet; advanced tooltips
- [x] 11.10 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 12. HUD

Scope: mcr-app hud and play, mcr-ui screens/hud.

- [x] 12.1 H1 (V) Status effect icons missing
- [x] 12.2 H2 (V) Attack indicator missing
- [x] 12.3 H3 (V) Crosshair not inverted, drawn in third person and spectator
- [x] 12.4 H5 (V) Overlays drawn above the HUD
- [x] 12.5 H6 (V) Selected item name popup missing
- [x] 12.6 H7 (V) Mount health and jump bar missing
- [x] 12.7 H8 (V) Hunger icons never show and food does not jitter at zero saturation
- [x] 12.8 H9 (V) Air bubbles wrong
- [x] 12.9 H10 (V) Off-hand slot ignores the main-hand option
- [x] 12.10 H11 (V) Tab list hearts objectives render nothing
- [x] 12.11 H13 (V) Locator bar background covers the XP level
- [x] 12.12 H14 (V) Jukebox "Now Playing" action bar missing
- [x] 12.13 H17 (M) Sidebar and title backdrops
- [x] 12.14 H20 (M) Title and action bar cut-off
- [x] 12.15 C22 (M) Hotbar pop animation
- [x] 12.16 Offscreen HUD tests: effect icons rows, attack indicator modes, air bubbles, left-handed off-hand slot, vehicle hearts and jump bar
- [x] 12.17 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 13. Camera effects and screen overlays

Scope: mcr-app overlays, play camera and FOV.

- [x] 13.1 H4 (V) Camera overlays missing (pumpkin, powder snow, portal, nausea, fire, in-block, underwater)
- [x] 13.2 E4 (V) View bobbing and hurt tilt apply only to the hand or only to the world
- [x] 13.3 E5 (V) FOV: bow zoom, clamp, death and fluid FOV missing
- [x] 13.4 E6 (V) Nausea and portal screen warp missing
- [x] 13.5 E20 (M) Third-person distance and eye height ignore attributes and scale
- [x] 13.6 S4 (V) Portal trigger sound missing
- [x] 13.7 Unit tests: FOV clamp, bow and fluid factors, overlay order and alpha, portal state ticks
- [x] 13.8 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 14. Movement effects and equipment

Scope: mcr-physics tick, effects and fly; mcr-client state.

- [x] 14.1 P5 (V) Auto-jump option does nothing
- [x] 14.2 P6 (V) Blindness does not block or stop sprinting
- [x] 14.3 P7 (V) Dolphin's Grace has no effect
- [x] 14.4 P8 (V) Leather boots are never detected, so powder snow is never walkable
- [x] 14.5 P10 (V) Gliding through vines cancels the elytra
- [x] 14.6 P16 (M) Cobweb ignores Weaving
- [x] 14.7 P17 (M) Nether lava current too weak
- [x] 14.8 P18 (M) friction_modifier and air_drag_modifier not wired
- [x] 14.9 P20 (M) Block speed factor applies while gliding
- [x] 14.10 P21 (M) Riptide does not bounce off entities
- [x] 14.11 Unit tests: auto-jump step, blind sprint refused, dolphin's grace drag, leather boots on powder snow, glide through vines
- [x] 14.12 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 15. First-person hand

Scope: mcr-app hand.

- [x] 15.1 E2 (V) Off-hand use not animated, wrong hand animated, off-hand scoping
- [x] 15.2 E3 (V) Off-hand charged crossbow hides the main hand
- [x] 15.3 E13 (V) First-person re-equip and lowering behaviours missing
- [x] 15.4 E14 (M) Swing duration and animation type fixed
- [x] 15.5 Unit tests: off-hand use animation, crossbow hand selection, item-used dip, swing duration with Haste
- [x] 15.6 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 16. Entity motion and projectiles

Scope: mcr-client entity and handler, mcr-client client.

- [x] 16.1 N9 (V) Many projectiles stutter between updates
- [x] 16.2 N10 (M) Living entity interpolation differs
- [x] 16.3 N12 (M) Entity velocity clamped to ±3.9
- [x] 16.4 N13 (M) Bundles not applied atomically
- [x] 16.5 N14 (M) player_look_at uses a fixed eye height
- [x] 16.6 N15 (M) Entity teleports ignore velocity-relative flags
- [x] 16.7 Unit tests: simulated projectile between syncs, stepped interpolation with queued steps, bundle applied in one tick, teleport delta flags
- [x] 16.8 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 17. Particles

Scope: mcr-particles, mcr-app effects.

- [x] 17.1 E12 (V) Many particle types use a generic fallback or are skipped
- [x] 17.2 N11 (M) Particle flags dropped or misused
- [x] 17.3 Unit tests: each particle type maps to a non-generic kind, falling leaves lifetime and gravity, override limiter beyond 32 blocks
- [x] 17.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 18. Entity layers, name tags, maps and compasses

Scope: mcr-entity-model humanoids, misc and maps, mcr-app nametags, skins and map, mcr-assets item_eval.

- [x] 18.1 E9 (V) Missing player layers: cape, shoulder parrots, stuck arrows and stingers, riptide swirl
- [x] 18.2 E11 (V) Minecart pitch, boat paddles and hit wobble missing
- [x] 18.3 H12 (V) Name tags
- [x] 18.4 C23 (V) Map decorations: wrong sprites, no names, no frame filter
- [x] 18.5 N4 (V) Compass and clock items frozen
- [x] 18.6 Unit tests: map decoration sprite names for every type, frame filter, name tag alpha; compass angle toward spawn
- [x] 18.7 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 19. Chat and commands

Scope: mcr-ui chat_screen, clicks and edit_box, mcr-client commands, mcr-app view.

- [x] 19.1 S5 (V) Chat click events handled differently
- [x] 19.2 S6 (V) No Brigadier feedback in the command box
- [x] 19.3 S7 (V) Suggestion popup ignores autoSuggestions; no player names in chat
- [x] 19.4 S8 (V) Chat colour and link options ignored
- [x] 19.5 S21 (M) Small chat differences
- [x] 19.6 H16 (M) Chat scale, spacing, delay and focused height ignored
- [x] 19.7 H18 (M) System message tag key
- [x] 19.8 H19 (M) Tab heads follow the account, not the server's online mode
- [x] 19.9 Unit tests: click event routing, argument colouring and usage, Tab player names, chat scale and spacing layout
- [x] 19.10 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 20. Options, keys and screens

Scope: mcr-app settings, play and shell, mcr-settings, mcr-client client info.

- [ ] 20.1 S9 (V) Options screens mostly missing
- [ ] 20.2 S10 (V) Key binds screen
- [ ] 20.3 S11 (V) Client information ignores options
- [ ] 20.4 S12 (V) Options nothing reads
- [ ] 20.5 S13 (V) No F3 key combos; F3 toggles on press
- [ ] 20.6 S14 (V) F2 screenshot and F11 fullscreen do nothing
- [ ] 20.7 S15 (V) Social Interactions, Quick Actions and pause screen additions missing
- [ ] 20.8 S22 (M) Credits controls
- [ ] 20.9 S23 (M) Smooth camera key
- [ ] 20.10 E19 (M) Glint speed and strength options ignored
- [ ] 20.11 Unit tests: client information from options, F3 combo handling and release toggle, key binds conflicts
- [ ] 20.12 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 21. Block rendering fidelity

Scope: mcr-mesh, mcr-render, mcr-assets, mcr-block-entity.

- [ ] 21.1 R8 (V) AO ignores per-block shade brightness
- [ ] 21.2 R9 (V) Translucent quads not sorted within a section
- [ ] 21.3 R10 (V) Pane and bar faces between neighbours not culled
- [ ] 21.4 R11 (V) Per-element light_emission dropped
- [ ] 21.5 R12 (V) Emissive block list wrong
- [ ] 21.6 R13 (V) Beacon beam without a pyramid
- [ ] 21.7 R15 (M) Rendering options parsed but never applied
- [ ] 21.8 R16 (M) New chunks do not fade in
- [ ] 21.9 R17 (M) Leaf, grate, mangrove root and water-overlay culling sets
- [ ] 21.10 R19 (M) Snow line and frozen-ocean rain simplified
- [ ] 21.11 R20 (M) Break cracks follow the outline box, not the model
- [ ] 21.12 R21 (M) Fluid side shading in the Nether
- [ ] 21.13 R22 (M) Biome tint height per column
- [ ] 21.14 R23 (M) Variant seeds for two-block blocks
- [ ] 21.15 R24 (M) End portal draws all six faces
- [ ] 21.16 Mesh tests: shade brightness per block, pane culling, emission per element, sculk phase, beacon without pyramid; criterion bench for the translucent sort
- [ ] 21.17 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 22. Close out

- [ ] 22.1 Re-audit each batch's gaps against the decompiled client after it merges; record leftovers in a follow-up change
- [ ] 22.2 Validate and archive the change
