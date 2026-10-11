## Context

Mod mode is driven by `mcr-modbridge` (`mod-fabric-boot`): enabled jars go to Fabric Loader via `fabric.addMods`, and
the game directory's own `mods/` stays empty. The scan in `.cache/modscan/ANALYSIS.md` found that about 11% of mods
need OpenGL specifically, plus a set of mods that own the camera, window, audio device or terrain. The JVM cannot be
re-created in-process, so any change to the mod set takes effect on the next start.

## Decisions

**Scanning.**
- On startup and when the Mods screen opens, `mcr-modbridge::manager` walks `<data>/mods/*.jar` and reads
  `fabric.mod.json` with the `zip` crate. Only that entry and nested `META-INF/jars/*.jar` manifests are read, never
  class files.
- Fields: `id`, `version`, `name`, `description`, `icon` (decoded and cached), `environment`, `depends`,
  `recommends`, `breaks`, `provides`, nested jar ids.
- `META-INF/neoforge.mods.toml` or `mods.toml` marks NeoForge/Forge.
- A jar that fails to parse is listed as invalid with the error.
- Scanning is bounded: entry sizes are capped at 1 MiB per JSON, and nested jars are read one level deep.

**Policy.**
- `mod-policy.json` maps mod id → `{status, reason}`. It is compiled into the binary with `include_str!` and
  overridable by `<data>/mod-policy.json` for testing.
- The initial entries come from the scan's OpenGL/OpenAL/camera/terrain findings:

| id | status | reason |
|---|---|---|
| `sodium`, `iris`, `immediatelyfast` | blocked | replaces the OpenGL renderer; mcr renders terrain natively |
| `distanthorizons` | blocked | own OpenGL LOD renderer |
| `xaerominimap`, `xaeroworldmap`, `journeymap` | blocked | draws with OpenGL directly |
| `fancymenu` | blocked | OpenGL calls in menu rendering |
| `sound_physics_remastered` | blocked | OpenAL effects on the vanilla sound device |
| `voicechat` | unsupported | own OpenAL playback and capture |
| `freecam` | unsupported | camera is owned by mcr |
| `litematica` | unsupported | schematic rendering uses vanilla terrain meshing |
| `dynamic_fps` | unsupported | frame pacing and window are owned by mcr |
| `continuity`, `moreculling` | partial | terrain is rendered by mcr; no visible effect |
| `lambdynlights` | partial | terrain lighting is mcr's |
| `badoptimizations`, `c2me`, `bobby` | partial | optimises or caches paths mcr replaces |
| `sodium-extra`, `reeses-sodium-options` | blocked | require Sodium |

The policy is a list, not code, so new findings are data edits.

**Resolution.** A small resolver runs before boot:
- Version ranges use Fabric's predicate syntax subset (`*`, `>=`, `<`, `~`, `^`, `x` wildcards, space-AND, arrays as
  OR) against `minecraft = 26.3`, `fabricloader = <pinned>`, `java = 25` and the enabled mod set (with `provides` and
  nested ids).
- Missing or `breaks` conflicts disable the mod for this start, with the reason.
- The resolver is advisory: Fabric Loader remains the authority, and its failure arrives through the crash path
  (`on_crash`) and is shown on the Mods screen.

**State.** `mcr-settings` gains `mods.enabled: map<id, bool>` (default true for newly found mods unless blocked) and
`jvm.heap_mib` (default 2048, range 1024–4096). The mod-mode decision at startup is: at least one enabled,
resolvable mod, and the `mods.disabled_all` fallback flag is not set.

**Screen.**
- `mcr-ui` "Mods" screen: a scrollable list (icon, name, version, id, status badge with reason tooltip), an enable
  toggle (disabled for `blocked`), an "Open mods folder" button (platform opener), a heap slider and a
  restart-required banner once the enabled set differs from the running set.
- Entry points: mcr main menu (without mods) and the "mcr" overlay button in Java's title and pause screens.

**Crash fallback.**
- `mcr-modbridge` writes `<data>/java/last-session.json` with `{state: booting|running|crashed|hung, crash_report}`.
- On the next start, if the previous state is not clean, mcr shows a native prompt: "Start without mods", "Try
  again", "Open crash report". "Start without mods" sets `mods.disabled_all` until the user re-enables mods.

## Risks

- Policy drift as mods update: entries are by id, with an optional version range field for later.
- Mods that are libraries of blocked mods: dependency resolution marks dependents as disabled with a clear reason.
