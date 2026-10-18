# Tasks

## 1. Singleplayer crate

- [x] 1.1 `lan`: forwarder (bounded connections and buffers, stop) and announcer (sanitized MOTD, interval, stop); loopback tests
- [x] 1.8 `lan` login gate: bounded handshake and login start decode with a timeout; refuse the host's name (any case), its UUID and names already online; tests for each refusal, garbage and slow openings
- [x] 1.2 `mcr_data::game_rules` table and validation; test against `commands.json` when present
- [x] 1.3 `flat`: preset string and JSON to validated `generator-settings`; tests with hostile input
- [x] 1.4 `packs`: data pack validation and copy into `<world>/datapacks`; symlink, size and count tests
- [x] 1.5 Creation props: experiments, generator settings, `max-players=8`; tests
- [x] 1.6 `recreate`: read 26.x saved data and pre-26 `level.dat` places; preset mapping and refusal; tests
- [x] 1.7 Icon: centre crop, 64×64 downscale, PNG write through `safe`; tests

## 2. Renderer

- [x] 2.1 `request_capture` / `take_capture`: copy before UI, forced intermediate target on the surface path

## 3. UI

- [x] 3.1 Pause menu Open to LAN button; `ShareToLanScreen` with port check callback; tests
- [x] 3.2 Create New World: Game Rules, Data Packs and Customize screens; settings fields; tests
- [x] 3.3 World list Re-Create through `WorldStore`; warning screen label for local worlds; tests

## 4. App

- [x] 4.1 Session: LAN open/stop, console commands, no freeze while published
- [x] 4.2 Shell and Play: LAN action, chat lines, icon capture on pause and write on Save and Quit
- [x] 4.3 Creation: packs copy, experiments, flat, game rules at Ready; Re-Create store
- [x] 4.4 Unit tests for mappings

## 5. Mod bridge and Java (ABI 18)

- [x] 5.1 ABI 18: `WORLD_LAN`, `lan` request, create keys (rules, experiments, packs, packdir, flat); Rust parsing tests
- [x] 5.2 Java create hook: rules, experiments, packs, flat JSON, temp pack folder kept until the session settles
- [x] 5.3 World Options LAN section and `McrLanScreen`; `WORLD_LAN` chat line; pause rule when published
- [x] 5.4 Warning screen label, crash screen with Back to World List, `createFreshLevel` refusal
- [x] 5.5 JVM test `world_screens` extended (create keys, refused types, crash screen, fresh level)

## 6. Verification

- [x] 6.1 Live native: LAN open, second client through the forwarder on 127.0.0.1, announcement captured on loopback
- [x] 6.2 Live native: icon written after Save and Quit; Re-Create of a seeded world; game rule and data pack applied
- [x] 6.3 Live mod mode with `.cache/play-mods`: LAN from World Options, crash screen, icon
- [x] 6.4 docs/mod-bridge.md; fmt, clippy, tests, java-build; merge main and pick the ABI number; archive
