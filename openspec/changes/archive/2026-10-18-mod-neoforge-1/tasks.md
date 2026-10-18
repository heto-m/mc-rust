## 1. Survey and fetch

- [x] 1.1 Survey NeoForge 26.3 layout (installer, FML 12 startup, discovery, module layer, mixin service)
- [x] 1.2 `cargo xtask neoforge-fetch`: installer, libraries with checksums, patched client jar, natives, `launch.json`; unit tests for profile parsing and library merge

## 2. Rust side

- [x] 2.1 `Loader` enum chosen from the enabled set, mixed-set refusal; unit tests
- [x] 2.2 NeoForge launch manifest and plan (`libraryDirectory`, `fml.modFolders`, `--fml.*`, native access); unit test
- [x] 2.3 `config/fml.toml` seeding (early window and version check off); unit test
- [x] 2.4 AOT cache and preloading off for NeoForge
- [x] 2.5 Scan `neoforge.mods.toml`, resolver with NeoForge-provided ids; unit tests
- [x] 2.6 App: loader choice, refusal message, status line

## 3. Java side

- [x] 3.1 `Loaders` seam; Fabric entry layer moved behind it; McrProfiler and McrPreload loader-neutral
- [x] 3.2 `McrBridgeNeoForge` (`@Mod`), `META-INF/neoforge.mods.toml`, optional `neoforge-launch` build dependency
- [x] 3.3 Boot under NeoForge headless; fix mixins NeoForge's patches break, keeping Fabric green

## 4. Verify

- [x] 4.1 JVM test `neoforge_boot` (`MCR_JVM_TESTS=1`)
- [x] 4.2 Title screen composed onto mcr's frame (`--mods <neoforge dir> --screenshot`)
- [x] 4.3 Join a vanilla 26.3 server on port 25574 through the net bridge
- [x] 4.4 Popular client NeoForge mods from Modrinth into `.cache/mods-neoforge/`; record which work
- [x] 4.5 Singleplayer world screens under NeoForge do not crash
- [x] 4.6 Fabric regressions: `fabric_boot`, `mod_title`
- [x] 4.7 Docs (`docs/mod-bridge.md`), fmt, clippy, tests
