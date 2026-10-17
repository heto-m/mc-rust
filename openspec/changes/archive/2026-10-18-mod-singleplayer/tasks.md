## 1. ABI and Rust bridge

- [x] 1.1 ABI 17: `MSG_WORLD`, `WORLD_*` kinds, `REPORT_WORLD`, `JAVA_PAUSING`; Java mirror and constant test
- [x] 1.2 `WorldRequest` parsing from `REPORT_WORLD` (bounded, unit tests)
- [x] 1.3 Ordered world message queue in `Pending` (coalescing status, bounded, unit tests); `ModFrames::world`
- [x] 1.4 `JavaInput::pausing`; `BridgeConfig::saves_dir` launch flags (test)

## 2. Java

- [x] 2.1 Saves redirect (`LevelStorageSource` constructor) and busy guard on world access
- [x] 2.2 Play and create interception, option mapping, unsupported type toast, `doWorldLoad` guard
- [x] 2.3 `McrWorlds` state, `McrWorldScreen` (progress, saving), EULA and failure screens, `MSG_WORLD` handling
- [x] 2.4 `NET_OPEN` parent for local joins; pause flag, Save and Quit label, `disconnectFromWorld` saving wait
- [x] 2.5 Remove the singleplayer "not supported" block (Realms stays blocked)

## 3. App

- [x] 3.1 Mod host: saves dir, world requests queue
- [x] 3.2 Shell Java-flow sessions: requests, EULA, updates to Java, no native screens, pause from `JAVA_PAUSING`
- [x] 3.3 `--singleplayer` in mod mode after Java settled; no native disconnected screen over Java's GUI
- [x] 3.4 Unit tests (request mapping, pause decision)

## 4. Verification

- [x] 4.1 JVM test `world_screens`: list from mcr saves, play and create requests, EULA, failure, no Java writes
- [x] 4.2 Update `input_title` (singleplayer opens the world list)
- [x] 4.3 Live: play-mods and Fabric API + Mod Menu: create, play, pause (console freeze), save and quit, reopen
- [x] 4.4 docs/mod-bridge.md; fmt, clippy, tests, java-build; archive
