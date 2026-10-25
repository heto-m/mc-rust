## 0. Change

- [x] 0.1 Proposal, design, specs and tasks; `openspec validate vanilla-parity-7 --strict`

## 1. F3 debug entries

Scope: mcr-ui debug_entries, mcr-app debug.

- [x] 1.1 Q1 (V) Add the missing debug entries the client has data for
- [x] 1.2 Q2 (M) Write the F3 text in vanilla's entry order
- [x] 1.3 fmt, clippy and workspace tests

## 2. World Options scroll and cloud depth

Scope: mcr-app world options, mcr-render clouds.

- [x] 2.1 Q3 (M) Scroll the World Options rows
- [x] 2.2 Q4 (M) Cloud depth target with improved transparency; GPU time before and after
- [x] 2.3 fmt, clippy and workspace tests

## 3. Live checks

- [x] 3.1 Q5 (M) stop_sound in mod mode: moved to the next parity change (needs the Fabric JVM; mods are paused)
- [x] 3.2 Q6 (M) LAN, Command Access and Force Game Mode on a managed singleplayer world with a guest
- [x] 3.3 Q7 (M) Pack drop copy failure toast
- [x] 3.4 Q8 (M) Cape on elytra with an online-mode account: moved to the next parity change (owner only)

## 4. Close out

- [x] 4.1 Re-audit each batch against the decompiled client after it merges; record leftovers
- [x] 4.2 Validate and archive the change
