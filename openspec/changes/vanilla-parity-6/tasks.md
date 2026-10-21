## 0. Change

- [x] 0.1 Proposal, design, specs and tasks; `openspec validate vanilla-parity-6 --strict`

## 1. Fabulous transparency completion

Scope: mcr-render transparency, mcr-app.

- [ ] 1.1 P1 (V) Translucent entity layers and item entities through the OIT chain
- [ ] 1.2 P2 (V) Weather, clouds and world border through the OIT chain
- [ ] 1.3 P3 (M) Composite writes depth
- [ ] 1.4 P4 (M) Dynamic lights in the OIT terrain pass
- [ ] 1.5 Offscreen tests; GPU time with the option on and off; fmt, clippy and workspace tests

## 2. Skin elytra texture and live checks

Scope: mcr-auth, mcr-entity-model humanoids and maps.

- [ ] 2.1 P5 (V) Fetch and use the profile ELYTRA texture
- [ ] 2.2 P6 (M) Live check: cape on elytra with an online-mode account
- [ ] 2.3 P7 (M) Live check: map label order with 37 named markers
- [ ] 2.4 fmt, clippy and workspace tests

## 3. Terrain late-phase cost and beacon cadence

Scope: mcr-render cull and terrain, mcr-app block_entities.

- [ ] 3.1 P8 (M) Solid-before-cutout in the late cull phase, pixel-identical
- [ ] 3.2 P9 (M) Beacon beam scan one step per tick
- [ ] 3.3 GPU medians before and after; fmt, clippy and workspace tests

## 4. World Options and debug screens

Scope: mcr-app world options, debug_keys and packs, mcr-ui menus.

- [ ] 4.1 P10 (V) World Options lacks Restrictions, guest commands, force game mode rows and tooltips
- [ ] 4.2 P11 (M) LAN can be opened from World Options but not closed
- [ ] 4.3 P12 (M) Default game mode row starts from the host's current mode, not the world default
- [ ] 4.4 P13 (M) F3+F6 is a toggle list, not vanilla's debug entry/profile screen; F3+L writes text, not a profile zip; F3+1 draws frame charts, not the profiler pie
- [ ] 4.5 P14 (M) Pack drop copy failures go to stderr, not a toast
- [ ] 4.6 P15 (M) Live checks for spear hit dip, delayed level events 3012-3021 and stop_sound in mod mode
- [ ] 4.7 fmt, clippy and workspace tests; live check against a vanilla 26.3 server

## 5. Close out

- [ ] 5.1 Re-audit each batch against the decompiled client after it merges; record leftovers
- [ ] 5.2 Validate and archive the change
