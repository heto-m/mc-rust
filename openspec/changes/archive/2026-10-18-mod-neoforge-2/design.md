# Design

## Warm-up under FML

FML 12 loads the game, NeoForge and the mods through one `TransformingClassLoader` (a parallel-capable
`ModuleClassLoader` with per-name locks). JDK 25's AOT cache serves classes from custom loaders when the bytes match,
so the Fabric design carries over. Two inputs change:

- **Key.** FML takes the patched game jar from `-DlibraryDirectory`, not the class path, so a re-patched jar would
  not change the Fabric key. Under NeoForge the key also hashes the patched jar (size and modification time) and the
  extra string names the loader and its version (`bridge 19 neoforge 26.3.0.90`). The Fabric key is unchanged, so
  Fabric caches survive the change.
- **Preloader.** `McrPreload` used Knot through `FabricClasses`. `GameClasses` now chooses the loader at run time:
  when the bridge itself was loaded by FML's `TransformingClassLoader` it uses that loader, probes already-loaded
  classes through its private `getLoadedClass` (reflection, read once), and reads the class processors from
  `classTransformer.processors.sortedProcessors`. The processor packages (Mixin through `FMLMixinClassProcessor`,
  access transformers, enum extender) join Mixin and MixinExtras in the exclusions, for the same lock-order reason:
  processors load their own classes while transforming. If the processors cannot be read, the preloader stops and
  reports `preload_stopped` rather than guess. Mixin's own config and plugin exclusions apply unchanged.

### Measurements (Ryzen 7 5800X, 14 NeoForge mods, offscreen 1920x1080)

Before the change NeoForge reached the title in 10.0–10.7 s. Title screen, three runs per mode, modes alternated:

| Mode | Runs (s) | Median (s) |
|---|---|---|
| neither | 10.45, 10.53, 10.18 | 10.45 |
| AOT cache only | 9.60, 9.33, 9.19 | 9.33 |
| preload only | 8.70, 8.41, 8.47 | 8.47 |
| both | 7.78, 7.81, 7.88 | 7.81 |

The recording session reached the title in 11.69 s and quit in 3.37 s while `DestroyJavaVM` wrote the recording.
The cache is 146 MB. A warm session took 16,310 of its 32,766 classes from it.

Joining a vanilla server (neither, then both): title 10.83/10.44/10.71 against 8.05/8.07/7.91 s; in the world at
12.14/11.71/11.99 against 9.08/9.07/8.83 s. Without warm-up, every run detached Java once just after joining
(JEI's first-world work went over 2 s without a frame). Some seconds of the first second after ready averaged
over 100 ms per Java frame. With warm-up no run detached, and Java's frames averaged 1–34 ms. The frame rate 5 s
in was unchanged (about 2,200–2,400 fps). Preloading loaded about 9,950 classes in segment 0 in 2.3 s, skipped
131, and left no strikes.

## Fatal errors without dialogs

FML's `FatalErrorReporting` sets `java.awt.headless=false` and shows a Swing dialog, or an SDL message box when
AWT is headless, before exiting with 1. A NeoForge mod that throws in its constructor reaches this from
`Main.main`. It is a crash on the main thread, not the loading error screen. With a display variable set, that
dialog would open on the user's desktop. `McrBoot` therefore calls `GraphicsEnvironment.isHeadless()` before the game's
`main` (AWT caches the first answer, so the later property change has no effect). Under NeoForge
(`-Dmcr.boot.sdlOffscreen=true`) it also sets SDL's `SDL_VIDEO_DRIVER` hint to `offscreen`, which has no message
box backend. The hint is set only under NeoForge, where LWJGL sits on the class path that
`McrBoot` shares with the game.
The VM then exits 1 with a crash report, and mcr's existing VM-exit path takes over. The screenshot flow, which
waited for Java's title, now settles when Java is gone.

## Client commands

NeoForge patches `ClientPacketListener.sendCommand` to try `ClientCommandHandler.runCommand` first. The bridge's
HEAD injection (chat and commands go to Rust for signing) runs before that patch, so client commands never ran.
`Loaders` now carries a client-command predicate. The NeoForge entry binds NeoForge's handler by method handle,
because the NeoForge universal jar is not on the bridge's compile class path. The mixin asks it first and sends
to Rust only when it declines. Fabric keeps the default predicate, which declines everything.

## Multi-loader jars

Some mods ship one jar with `fabric.mod.json` and `neoforge.mods.toml` (LambDynamicLights 4.13.0). It used to
be read as Fabric only, which made a NeoForge set mixed. The scan now keeps the NeoForge reading next to the Fabric
one. Multi-loader jars do not vote on the loader. When the evaluated set comes out NeoForge, the manager swaps in
the NeoForge readings and evaluates again. A set of only multi-loader jars is Fabric. No ABI change.

## Test hooks

`-Dmcr.bridge.test=crash-in-tick|hang-in-tick` with `-Dmcr.bridge.test.ticks=N` (default 60) count client ticks
once the game finished loading with no overlay. Then they throw on the render thread or park it forever. They are
loader-neutral (`MinecraftMixin` tick hook). The NeoForge JVM tests run each case in a child process (one VM per
process), with `DISPLAY` and `WAYLAND_DISPLAY` removed.

## One pin

`crates/mcr-modbridge/src/pins.rs` holds `LOADER_VERSION`, `NEOFORGE_VERSION` and `FML_VERSION`. `xtask` includes
the same file with `#[path]`, so it holds constants only.

## Results outside code

- Config screens: AppleSkin (NeoForge `ConfigurationScreen`), Jade and BetterF3 (Cloth Config) change, save and
  reopen with the new value. JEI registers no `IConfigScreenFactory`, so the mod list's Config button is disabled
  as on stock NeoForge. Its wrench needs MezzConfig GUI, and with it a change lands in `jei-client.ini`. No bridge
  fix was needed.
- Singleplayer under NeoForge needed no change. Tested: Create World, the EULA prompt, play, pause (the server logs
  the freeze), Save and Quit, the world in the list with its icon, reopening it at the same spot, and Open to LAN.
- The sweep scoreboard is in `docs/mod-bridge.md`.
