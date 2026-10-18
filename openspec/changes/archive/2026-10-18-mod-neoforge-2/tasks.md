## 1. Warm-up

- [x] 1.1 Measure NeoForge without warm-up (title, join, first seconds after joining)
- [x] 1.2 AOT cache under NeoForge: key covers the patched game jar and the NeoForge version; check cache hits
- [x] 1.3 Loader-neutral preloader (`GameClasses`): FML's loader, already-loaded probe, class processor packages excluded
- [x] 1.4 Measure after (four modes alternating), check strikes stay clear
- [x] 1.5 JVM test `neoforge_warmup` (record, build, use the cache, preload)

## 2. Config screens

- [x] 2.1 JEI, Jade, AppleSkin, BetterF3: open from NeoForge's mod list, change, save, reopen; fix what breaks

## 3. Singleplayer

- [x] 3.1 Create a world through Java's Create World screen under NeoForge, play, pause (tick freeze)
- [x] 3.2 Save and Quit, reopen the world
- [x] 3.3 Open to LAN

## 4. Crash and exit

- [x] 4.1 Bridge test hooks for a crash in game and a hang, both loaders
- [x] 4.2 JVM tests under NeoForge: mod failure at load, crash in game, hang detaches within 2 s, `System.exit`, clean quit
- [x] 4.3 No native dialog on a fatal load error: AWT pinned headless, SDL offscreen under NeoForge

## 5. One pin

- [x] 5.1 Shared pin file for the NeoForge and Fabric loader versions, used by `mcr-modbridge` and `xtask`

## 6. Compat sweep

- [x] 6.1 Fifteen more client-side NeoForge mods for 26.3 from Modrinth into `.cache/mods-neoforge/`
- [x] 6.2 Each loads, joins, main feature works; fix bridge gaps found
- [x] 6.4 Multi-loader jars take the set's loader (LambDynamicLights)
- [x] 6.5 NeoForge client commands run before the bridge sends commands to Rust (spark)
- [x] 6.3 Scoreboard in `docs/mod-bridge.md`

## 7. Wrap-up

- [x] 7.1 Fabric regressions: `fabric_boot` (as is), `jvm_preload`, `mod_title`
- [x] 7.2 Docs, fmt, clippy, tests
