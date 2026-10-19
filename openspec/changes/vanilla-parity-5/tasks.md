## 0. Change

- [x] 0.1 Proposal, design, specs and tasks; `openspec validate vanilla-parity-5 --strict`

## 1. Placement prediction and adventure predicates

Scope: mcr-client use_item and adventure.

- [ ] 1.1 L1 (V) Block placement prediction covers property-less blocks only
- [ ] 1.2 L2 (M) Adventure predicates approximate block entity data
- [ ] 1.3 Unit tests: stair facing and half from the hit, slab onto slab, log axis, obstruction by an entity with the placement state; NBT and component predicates
- [ ] 1.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 2. Command suggestions and chat acknowledgements

Scope: mcr-client commands and secure_chat, mcr-ui chat, mcr-app view.

- [ ] 2.1 L3 (M) Suggestion providers other than ask_server
- [ ] 2.2 L4 (M) Chat acknowledgements ignore the chat delay
- [ ] 2.3 Unit tests: sound, entity and post-effect suggestions; a delayed message is acknowledged when shown
- [ ] 2.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 3. Riding and projectiles

Scope: mcr-physics body, mcr-client entity and riding, mcr-app entity_loops and ambience.

- [ ] 3.1 L5 (M) Trident loyalty ignores dealt damage and ground time
- [ ] 3.2 L6 (V) Attached firework rockets have no hand offset
- [ ] 3.3 L7 (M) Nautilus dash sounds and dash flag
- [ ] 3.4 L8 (V) Happy ghast and nautilus riding loops
- [ ] 3.5 Unit tests: trident returns after five ground ticks, rocket at the main and off hand, dash flag gates the bar, riding loop starts and stops with the vehicle
- [ ] 3.6 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 4. Entity rendering and particles

Scope: mcr-entity-model humanoids, misc and maps, mcr-app entities, mcr-particles.

- [ ] 4.1 L9 (V) Elder guardian curse effect not drawn
- [ ] 4.2 L10 (V) Elytra ignore the cape texture
- [ ] 4.3 L11 (V) Boats show water inside the hull
- [ ] 4.4 L12 (V) Smoke particles at the camera draw as a dark square
- [ ] 4.5 L13 (M) Map labels drawn after all icons
- [ ] 4.6 Offscreen tests: curse model in front of the camera, elytra with a cape, boat on water without water inside, smoke at the eye, label order on a map
- [ ] 4.7 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 5. First-person spear feedback and spectator menu

Scope: mcr-app hand, mcr-client spectator, mcr-app hud.

- [ ] 5.1 L14 (M) Spear hit feedback dip missing
- [ ] 5.2 L15 (M) Spectator team page order and icons
- [ ] 5.3 Unit tests: feedback curve after a kinetic hit; team and player page order
- [ ] 5.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 6. Screens and debug keys

Scope: mcr-ui menus and lan_screen, mcr-app packs, debug_keys, play and hud, mcr-singleplayer.

- [ ] 6.1 L16 (V) Pause menu lacks World Options
- [ ] 6.2 L17 (M) Pack screen without drag and drop or icons
- [ ] 6.3 L18 (M) Missing debug chords
- [ ] 6.4 L19 (M) Debug chords ignored while a screen is open
- [ ] 6.5 L20 (M) No autosave indicator in local worlds
- [ ] 6.6 Unit tests: pause menu layout local and remote, World Options rows, pack icon fallback, chords with a non-consuming screen, F3+C hold timing
- [ ] 6.7 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 7. Level event and mod-mode sounds

Scope: mcr-app level_events and audio, mcr-modbridge sound.

- [ ] 7.1 L21 (M) Distance-delayed level events play at once
- [ ] 7.2 L22 (M) stop_sound not Java-owned in mod mode
- [ ] 7.3 Unit tests: delay grows with distance for 3012 to 3021; stop_sound owned by Java in mod mode
- [ ] 7.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 8. World rendering

Scope: mcr-render, mcr-app overlays and block_entities, mcr-ui renderer, mcr-block-entity.

- [ ] 8.1 L23 (V) improvedTransparency (Fabulous) not implemented
- [ ] 8.2 L24 (M) Camera overlay textures sampled nearest
- [ ] 8.3 L25 (M) Texture filtering NONE differs at mip seams
- [ ] 8.4 L26 (M) Beacon levels refreshed every 20 ticks
- [ ] 8.5 L27 (M) Model-less block entities crack along their outline
- [ ] 8.6 L28 (M) Leaves behind terrain are shaded
- [ ] 8.7 Offscreen tests: water behind glass with improved transparency, linear pumpkin blur, cracked chest; GPU time before and after L23 and L28 (1920x1080, render distance 16, 600 frames, median, interleaved) with pixel-identical frames for L28
- [ ] 8.8 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 9. Close out

- [ ] 9.1 Re-audit each batch against the decompiled client after it merges; record leftovers in a follow-up change
- [ ] 9.2 Validate and archive the change
