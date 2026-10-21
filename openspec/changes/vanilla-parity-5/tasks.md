## 0. Change

- [x] 0.1 Proposal, design, specs and tasks; `openspec validate vanilla-parity-5 --strict`

## 1. Placement prediction and adventure predicates

Scope: mcr-client use_item and adventure.

- [x] 1.1 L1 (V) Block placement prediction covers property-less blocks only
- [x] 1.2 L2 (M) Adventure predicates approximate block entity data
- [x] 1.3 Unit tests: stair facing and half from the hit, slab onto slab, log axis, obstruction by an entity with the placement state; NBT and component predicates
- [x] 1.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 2. Command suggestions and chat acknowledgements

Scope: mcr-client commands and secure_chat, mcr-ui chat, mcr-app view.

- [x] 2.1 L3 (M) Suggestion providers other than ask_server
- [x] 2.2 L4 (M) Chat acknowledgements ignore the chat delay
- [x] 2.3 Unit tests: sound, entity and post-effect suggestions; a delayed message is acknowledged when shown
- [x] 2.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 3. Riding and projectiles

Scope: mcr-physics body, mcr-client entity and riding, mcr-app entity_loops and ambience.

- [x] 3.1 L5 (M) Trident loyalty ignores dealt damage and ground time
- [x] 3.2 L6 (V) Attached firework rockets have no hand offset
- [x] 3.3 L7 (M) Nautilus dash sounds and dash flag
- [x] 3.4 L8 (V) Happy ghast and nautilus riding loops
- [x] 3.5 Unit tests: trident returns after five ground ticks, rocket at the main and off hand, dash flag gates the bar, riding loop starts and stops with the vehicle
- [x] 3.6 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 4. Entity rendering and particles

Scope: mcr-entity-model humanoids, misc and maps, mcr-app entities, mcr-particles.

- [x] 4.1 L9 (V) Elder guardian curse effect not drawn
- [x] 4.2 L10 (V) Elytra ignore the cape texture
- [x] 4.3 L11 (V) Boats show water inside the hull
- [x] 4.4 L12 (V) Smoke particles at the camera draw as a dark square
- [x] 4.5 L13 (M) Map labels drawn after all icons
- [x] 4.6 Offscreen tests: curse model in front of the camera, elytra with a cape, boat on water without water inside, smoke at the eye, label order on a map
- [x] 4.7 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 5. First-person spear feedback and spectator menu

Scope: mcr-app hand, mcr-client spectator, mcr-app hud.

- [x] 5.1 L14 (M) Spear hit feedback dip missing
- [x] 5.2 L15 (M) Spectator team page order and icons
- [x] 5.3 Unit tests: feedback curve after a kinetic hit; team and player page order
- [x] 5.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 6. Screens and debug keys

Scope: mcr-ui menus and lan_screen, mcr-app packs, debug_keys, play and hud, mcr-singleplayer.

- [x] 6.1 L16 (V) Pause menu lacks World Options
- [x] 6.2 L17 (M) Pack screen without drag and drop or icons
- [x] 6.3 L18 (M) Missing debug chords
- [x] 6.4 L19 (M) Debug chords ignored while a screen is open
- [x] 6.5 L20 (M) No autosave indicator in local worlds
- [x] 6.6 Unit tests: pause menu layout local and remote, World Options rows, pack icon fallback, chords with a non-consuming screen, F3+C hold timing
- [x] 6.7 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 7. Level event and mod-mode sounds

Scope: mcr-app level_events and audio, mcr-modbridge sound.

- [x] 7.1 L21 (M) Distance-delayed level events play at once
- [x] 7.2 L22 (M) stop_sound not Java-owned in mod mode
- [x] 7.3 Unit tests: delay grows with distance for 3012 to 3021; stop_sound owned by Java in mod mode
- [x] 7.4 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 8. World rendering

Scope: mcr-render, mcr-app overlays and block_entities, mcr-ui renderer, mcr-block-entity.

- [x] 8.1 L23 (V) improvedTransparency (Fabulous) not implemented
- [x] 8.2 L24 (M) Camera overlay textures sampled nearest
- [x] 8.3 L25 (M) Texture filtering NONE differs at mip seams (measured and declined, see design)
- [x] 8.4 L26 (M) Beacon levels refreshed every 20 ticks
- [x] 8.5 L27 (M) Model-less block entities crack along their outline
- [x] 8.6 L28 (M) Leaves behind terrain are shaded
- [x] 8.7 Offscreen tests: water behind glass with improved transparency, linear pumpkin blur, cracked chest; GPU time before and after L23 and L28 (1920x1080, render distance 16, 600 frames, median, interleaved) with pixel-identical frames for L28
- [x] 8.8 fmt, clippy and workspace tests; live check against a vanilla 26.3 server
- [x] 8.9 Leftover found in batch 8: particles ignored fog; they now fade like terrain (offscreen test)

## 9. Close out

- [ ] 9.1 Re-audit each batch against the decompiled client after it merges; record leftovers in a follow-up change
- [ ] 9.2 Validate and archive the change
