# Tasks

## 1. Packs

- [x] 1.1 `mcr-modmeta` crate: scan, policy, resolve and pins out of `mcr-modbridge`, shared with xtask
- [x] 1.2 `cargo xtask modpack-fetch`: manifest, cfwidget with cache and pacing, CurseForge file record fallback, CDN download with size check, overrides; tests
- [x] 1.3 Client and server placement from environment and `mod-policy.json` (new `server` status), flat `mods/` layout, `pack.json` report; tests
- [x] 1.4 `cargo xtask fabric-server` (dev only, port 25565 refused) and `fabric-fetch` writing `.cache/fabric/server.json`
- [x] 1.5 FerriteCore measured with and without on both packs; kept

## 2. Registries

- [x] 2.1 `McrRegistries` format 2 export at the title screen and at the end of configuration (ABI 21)
- [x] 2.2 `mcr_data::rebuild` and `export_of` with bounds; vanilla round trip, modded block, added property, refusal and hostile-size tests
- [x] 2.3 `PerVersion<T>` caches for physics, light table and spawner names
- [x] 2.4 `BlockState` and paletted storage as `u32`; section updates bounded by the active count; benches compared
- [x] 2.5 Configuration pings answered by Java in order; `FinishConfiguration` held for the report (10 s cap)
- [x] 2.6 Mod jars (and nested asset jars) in Rust's resource stack; rebake on table change; server biome colours
- [x] 2.7 Undecodable play packets skipped and counted in mod mode
- [x] 2.8 Unreadable item model definitions skip only that item

## 3. Singleplayer

- [x] 3.1 `mcr_singleplayer::fabric`: server manifest parsing inside the cache, `mcr-mods.txt`; tests
- [x] 3.2 `Spec::fabric` with the `list` ready probe; `MCR_SP_PORT`; tests
- [x] 3.3 Fabric server behind the client's egress fence (`egress_properties`); command-line tests, vanilla unchanged

## 4. Runs and measurements

- [x] 4.1 Echoes of the Sift against the Fabric server and in singleplayer
- [x] 4.2 Medieval Immersed against the Fabric server and in singleplayer
- [x] 4.3 Frame time, Java and Rust share, RSS, title and join time in design.md
- [x] 4.4 `tools/official-launcher/measure_pack.py` for the owner's official-launcher comparison
- [x] 4.5 docs/mod-bridge.md: modpacks, registries, Fabric singleplayer and egress
