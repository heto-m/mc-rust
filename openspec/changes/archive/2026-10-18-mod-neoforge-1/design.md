# Design

## NeoForge 26.3 as found (26.3.0.90, FML 12.0.12)

- **Installer.** `neoforge-<v>-installer.jar` holds `version.json` (launcher profile, `inheritsFrom: 26.3`),
  `install_profile.json` and `data/client.lzma` (binary patches). The client install has one processor:
  installertools `PROCESS_MINECRAFT_JAR --input client.jar --output <patched> --apply-patches client.lzma`.
  The output goes to `libraries/net/neoforged/minecraft-client-patched/<v>/minecraft-client-patched-<v>.jar`.
  `net.neoforged:neoforge:<v>:universal` holds NeoForge's own classes, `neoforge.mods.toml`, access
  transformers and `neoforge.mixins.json`.
- **Launch.** Main class `net.neoforged.fml.startup.Client`. Game arguments add `--fml.neoForgeVersion <v>
  --fml.mcVersion 26.3 --fml.neoFormVersion 1`, and JVM arguments add `-DlibraryDirectory=<libraries>`,
  `--add-opens java.base/java.lang.invoke=ALL-UNNAMED` and
  `--add-exports jdk.naming.dns/com.sun.jndi.dns=java.naming`. The class path holds FML and its libraries
  plus the vanilla libraries, but not the client jar.
- **No ModLauncher, no SecureJarHandler.** FML 12 starts the game itself. `FMLLoader.create` discovers mods,
  then builds one `ModuleLayer` over the boot layer for all game content (Minecraft, NeoForge, mods, jar-in-jar
  libraries), each as an automatic module. The layer's `TransformingClassLoader` falls back to the class path
  loader, and automatic modules read every unnamed module, so `dev.mcr.boot` on the class path is shared by the
  game layer the way `fabric.systemLibraries` shares it under Knot.
- **Discovery.** `GameLocator` takes the patched jar and the universal jar from `libraryDirectory`.
  `ModsFolderLocator` scans `<gameDir>/mods`. `InDevFolderLocator` adds `fml.modFolders`
  (`name%%path` entries; entries with the same name are merged into one mod file).
- **Mixin.** FML ships `net.fabricmc:sponge-mixin:0.17.4+mixin.0.8.7`, the same build Fabric Loader 0.19.5
  uses, behind its own `FMLMixinService`. Mods list configs in `[[mixins]] config = "…"`. MixinExtras is
  bundled by NeoForge.
- **Instrumentation.** `Client.main` passes `DevAgent.getInstrumentation()`, which may be null. Then only FML's
  `ClassLoadingGuardian` is skipped. `FMLLoader.create(StartupArgs)` would self-attach by spawning
  `ProcessHandle.current().info().command()`, which is the `mcr` binary, but `Client` never takes that path.
- **Early window.** `ImmediateWindowHandler` opens its own SDL/OpenGL window unless `earlyWindowControl` is
  false in `config/fml.toml`.

## Decisions

### One jar, one mixin config, two entry classes
`mcr-bridge.jar` gets `META-INF/neoforge.mods.toml` (`modLoader="javafml"`, mod `mcr_bridge`, `[[mixins]]
mcr_bridge.mixins.json`) next to `fabric.mod.json`. Each loader ignores the other's manifest. The Fabric entry
(`McrBridge`, `ClientModInitializer` + `PreLaunchEntrypoint`) and the NeoForge entry (`McrBridgeNeoForge`,
`@Mod(value = "mcr_bridge", dist = Dist.CLIENT)`, no-argument constructor) both fill `Loaders`: the mod
list (id, version, top-level id, root paths) and the loader name. They then run the shared init.
`McrProfiler` reads mods through `Loaders`. `McrPreload`'s Fabric calls move into the Fabric layer, which only
runs when the preload property is set, and it is never set under NeoForge. FML scans classes without loading
them, and Knot never loads the NeoForge entry, so neither loader touches the other's API.

The NeoForge sources (`java/mcr-bridge/src-neoforge`, `resources-neoforge`) compile against FML's loader jar.
`deps.txt` marks `neoforge-launch optional`. Without `.cache/neoforge/launch.json`, java-build skips them and
says so, so a checkout without NeoForge still builds. A NeoForge boot checks that the bridge jar holds
`META-INF/neoforge.mods.toml` and otherwise names the two commands to run.

### Loader choice in Rust
`mcr_modbridge::Loader` is `Fabric` or `NeoForge`. `Loader::for_mods` looks at the active entries: only Fabric
means Fabric, only NeoForge means NeoForge, and both is `Err(MixedLoaders)` naming one mod of each kind. The
manager's `evaluate` runs it after the dependency pre-check; on a mixed set every active mod becomes
`Excluded(<message>)` and `Report::mixed` keeps the error, so the Mods screen shows why each mod is not loaded,
`report.jars()` is empty and no JVM starts. mcr prints the message once instead of one line per mod.
`BridgeConfig.loader` picks the manifest (`fabric/launch.json` or `neoforge/launch.json`) and the plan.
Everything after `JavaVm::start` (rings, frames, net, sound, input) is unchanged.

### NeoForge launch plan
The plan uses the same `McrBoot launch <main> <args>` and the same `-D` set as Fabric minus the
`fabric.*` keys, plus:
- `-DlibraryDirectory=<.cache/neoforge/libraries>` and the profile's JVM arguments with `${library_directory}`
  substituted;
- `-Dfml.modFolders=mcr0%%<bridge>:mcr1%%<mod>…`, one name per jar so FML does not merge them;
- `--illegal-native-access=allow`. `--enable-native-access` names boot-layer modules only. The game layer is
  built by FML, which keeps its `Controller`, so the bridge's restricted FFM calls (`reinterpret`, upcall
  stubs) would warn per module under JDK 25's default. `ALL-UNNAMED` already allows every Fabric mod, so the
  policy is the same;
- the `--fml.*` game arguments before vanilla's.

The game dir gets `config/fml.toml` with `earlyWindowControl = false` and `versionCheck = false` (other keys
kept) before every NeoForge start.

### Warm-up
`Bridge::start` builds the AOT and preload plans only for Fabric, so a NeoForge session passes no AOT or
class-load log option, and mcr prints that both are off under NeoForge. The AOT key and the preloader's
exclusions are written against Knot (`FabricLauncher.isClassLoaded`, Knot's hidden classes), and FML's module
layer defines classes through a loader the cache validation has not been measured against.

### Patched classes
NeoForge's client jar is recompiled from patched sources, and a name-only mixin selector takes the first method
of that name. NeoForge added `compileSync`/`compileAsync(RenderSectionRegion, List<AdditionalSectionRenderer>)`
next to vanilla's one-argument methods (now deprecated wrappers), and its `LevelRenderer` calls the new ones.
The bridge only cancelled the old ones, so sections meshed for real and `LevelLoadingScreen` never saw the
player's section compiled. `RenderSectionMixin` now names vanilla's methods by descriptor and adds a handler for
NeoForge's overloads with `require = 0` (absent under Fabric). A scan of every mixin target for overload counts
that differ between the vanilla and the patched jar found no other case.

`-Dio.netty.transport.noNative=true` (NeoForge only): `ConnectScreen` still builds netty's event loop group, and
netty's kqueue probe threw inside NeoForge's debug-file log appender. Rust owns the sockets, so Java never needs
native transports.

### Mods screen and pre-check
`neoforge.mods.toml` is read with a small bounded reader for the keys we need: `[[mods]]` `modId`, `version`,
`displayName`, `description`; `[[dependencies.<id>]]` `modId`, `type` (or legacy `mandatory`), `versionRange`
and `side`. It is not a full TOML parser, and unknown syntax only drops fields. `${file.jarVersion}` versions
fall back to the jar manifest's `Implementation-Version`. Maven ranges (`[1.0,2.0)`) go through a small
translation onto the existing `Range`; a bare version is Maven's soft requirement and matches anything.
Jar-in-jar mods listed in `META-INF/jarjar/metadata.json` count as provided, like Fabric's nested jars. A jar
without `neoforge.mods.toml` whose manifest says `FMLModType: GAMELIBRARY` or `LIBRARY` (MezzConfig's
standalone jar) is a NeoForge library and is passed to FML. Provided ids: `minecraft` (26.3), `neoforge`
(pinned), `fml` (12.0.12), `java` (25). The mod policy applies to NeoForge jars too: blocked ids such as
`sodium` and `iris` mean the same mods on both loaders.

## Tests

`fabric_boot` asserted no TCP socket at the title screen, which has failed since the services discovery host
was allow-listed: Java's discovery request leaves a proxied upstream socket open. Both boot tests now run with
an empty proxy allow-list, which keeps the check on what it guards (Java opening no socket of its own).

## Risks

- NeoForge binpatches change vanilla methods the bridge's mixins target (`defaultRequire: 1` makes a missed
  injection fatal). Each is fixed at a point both loaders share. Fabric stays green (`fabric_boot`, `mod_title`).
- NeoForge's client hooks may reach for SDL, OpenGL or networking that the bridge does not intercept.
- NeoForge's network layer adds handlers to the netty pipeline and a configuration-phase negotiation; against a
  vanilla server it falls back to vanilla mode, which the net bridge has to carry.
