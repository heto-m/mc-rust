## MODIFIED Requirements

### Requirement: Fabric launch inside mcr
`mcr-modbridge` SHALL start the stock `client.jar` through Fabric Loader's Knot client entrypoint inside the embedded JVM, with mods supplied only through `fabric.addMods`, the game jar only through `fabric.gameJarPath` (not on the system class path), the boot jar as a Fabric system library, the shared launcher-layout asset store as `--assetsDir`, and `--offlineDeveloperMode`.

#### Scenario: Fabric API and Mod Menu boot
- **WHEN** the bridge boots with Fabric API 0.162.0+26.3 and Mod Menu 21.0.0
- **THEN** both mods are reported loaded, their client initializers run, and Java reports `TitleScreen` as the current screen
