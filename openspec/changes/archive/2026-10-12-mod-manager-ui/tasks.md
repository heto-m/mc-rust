## 1. Scanning and policy
- [x] 1.1 Jar scanner (`fabric.mod.json`, nested jars, NeoForge detection, icons) with fixture jars built in tests
- [x] 1.2 `mod-policy.json` and loader, plus a test that every policy id is unique and every reason is non-empty
- [x] 1.3 Version predicate parser and dependency resolver, with unit tests for each predicate form, `provides`, nested ids and `breaks`

## 2. State and startup
- [x] 2.1 `mcr-settings`: `mods.enabled`, `mods.disabled_all`, `jvm.heap_mib`
- [x] 2.2 Startup decision and `fabric.addMods` list from the resolved set; no JVM when the set is empty
- [x] 2.3 `last-session.json` and the crash/hang fallback prompt

## 3. UI
- [x] 3.1 Mods screen in `mcr-ui` (list, badges, toggles, folder button, heap slider, restart banner)
- [x] 3.2 Entry from mcr's main menu (done; `Shell::open_mods` is the hook). The "mcr" button inside Java screens is deferred to `mod-input-screens`

## 4. Verification
- [x] 4.1 Integration test from the proposal's acceptance test (five jars, restart with Mod Menu disabled, all disabled → no `mcr-java` thread)
- [x] 4.2 UI snapshot test of the Mods screen (offscreen render, our own UI only)
- [x] 4.3 fmt, clippy, tests, archive
