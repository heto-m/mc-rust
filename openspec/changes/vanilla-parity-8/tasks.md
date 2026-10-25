## 0. Change

- [x] 0.1 Proposal, design, specs and tasks; `openspec validate vanilla-parity-8 --strict`

## 1. Text

Scope: mcr-ui text, font and chat.

- [ ] 1.1 T1–T6; `cargo xtask parity font` at 100%
- [ ] 1.2 fmt, clippy and workspace tests

## 2. Title panorama and window size

- [ ] 2.1 S1 panorama cubemap with blur and scroll speed; screenshot against vanilla
- [ ] 2.2 S2 `--size` sets the initial window size
- [ ] 2.3 fmt, clippy and workspace tests

## 3. Carried

- [ ] 3.1 C1 stop_sound in mod mode (after mods resume)
- [ ] 3.2 C2 cape on elytra (owner)
- [ ] 3.3 C3 server-level F3 entries, if the managed server can expose them

## 4. Text components

Scope: mcr-protocol text, mcr-ui text.

- [ ] 4.1 X1–X8; `cargo xtask parity text` at 100%
- [ ] 4.2 fmt, clippy and workspace tests

## 5. Section meshing

Scope: mcr-mesh, mcr-render vertex formats.

- [ ] 5.1 M1–M8; `cargo xtask parity section_mesh` at 100%, `block_model` still 100%
- [ ] 5.2 Mesh memory, upload and GPU time before and after; keep vertex formats compact where precision allows
- [ ] 5.3 fmt, clippy and workspace tests

## 6. Close out

- [ ] 6.1 Validate and archive the change
