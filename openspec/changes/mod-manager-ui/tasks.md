## 1. Scanning and policy
- [ ] 1.1 Jar scanner (`fabric.mod.json`, nested jars, NeoForge detection, icons) with fixture jars built in tests
- [ ] 1.2 `mod-policy.json` and loader, plus a test that every policy id is unique and every reason is non-empty
- [ ] 1.3 Version predicate parser and dependency resolver, with unit tests for each predicate form, `provides`, nested ids and `breaks`

## 2. State and startup
- [ ] 2.1 `mcr-settings`: `mods.enabled`, `mods.disabled_all`, `jvm.heap_mib`
- [ ] 2.2 Startup decision and `fabric.addMods` list from the resolved set; no JVM when the set is empty
- [ ] 2.3 `last-session.json` and the crash/hang fallback prompt

## 3. UI
- [ ] 3.1 Mods screen in `mcr-ui` (list, badges, toggles, folder button, heap slider, restart banner)
- [ ] 3.2 Entry from mcr's main menu and from the Java-screen "mcr" overlay

## 4. Verification
- [ ] 4.1 Integration test from the proposal's acceptance test (five jars, restart with Mod Menu disabled, all disabled → no `mcr-java` thread)
- [ ] 4.2 UI snapshot test of the Mods screen (offscreen render, our own UI only)
- [ ] 4.3 fmt, clippy, tests, archive
