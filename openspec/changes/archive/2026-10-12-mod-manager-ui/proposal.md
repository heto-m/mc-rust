# Proposal

## Why

Users need a mods folder, a view of what is loaded and why something is not, and enable/disable toggles. Mods
that cannot work under the bridge (renderer replacements, direct OpenGL/OpenAL, camera or window owners) must be
blocked or flagged with a reason instead of crashing the client.

## What Changes

- Mods folder `<mcr data>/mods/`, scanned at startup:
  - Fabric jars are read from `fabric.mod.json`: id, name, version, description, icon, `depends`, `breaks`,
    `environment`, and nested jar ids under `META-INF/jars`.
  - NeoForge/Forge jars are recognised and listed as not supported yet.
  - Server-only mods are listed and not loaded.
- Policy data `crates/mcr-modbridge/data/mod-policy.json` (our own data). Each mod id has a status and a
  one-line reason: `blocked` (cannot be enabled, for example `sodium`, `iris`, `xaerominimap`), `unsupported`
  (enable with a warning, for example `voicechat`, `freecam`), or `partial` (loads, some features inert, for
  example `continuity`).
- Resolution before boot (dependencies, version ranges, `breaks`). Unresolvable mods are not loaded, with the
  reason shown. Fabric Loader's own errors are shown too.
- Enabled state in `mcr-settings` (by mod id). The enabled set goes to `mod-fabric-boot` via `fabric.addMods`.
  With nothing enabled, no JVM starts.
- A native mcr screen, "Mods":
  - Shows each mod's icon, name, version, status and reason, with toggles, "Open mods folder", a JVM heap slider
    (1–4 GiB) and a restart-required banner (a JVM cannot restart in-process).
  - Reachable from mcr's main menu and the "mcr" button in Java screens.
- Crash fallback: after a Java crash or hang, the next start offers "Start without mods" and shows the crash
  report path.

Out of scope:
- Downloading mods.
- Profiles.
- NeoForge loading.
- The OpenGL-backend mode for blocked mods.
- Automatic policy updates.

Acceptance test, with Fabric API, Mod Menu, Sodium, a NeoForge jar and a test mod with a missing dependency in the
folder:
- The Mods screen lists all five, with Sodium blocked (reason shown), the NeoForge jar marked unsupported and the
  test mod marked "missing dependency `foo`".
- Only Fabric API and Mod Menu, plus the bridge, are passed to the loader (Java reports them).
- After disabling Mod Menu and restarting, Java reports only Fabric API.
- With every mod disabled, no `mcr-java` thread exists.

## Capabilities

### New Capabilities
- `mod-manager`: mods folder scanning, policy, dependency pre-check, enable state, Mods screen, crash fallback.

### Modified Capabilities

## Impact

`mcr-modbridge`, `mcr-settings`, `mcr-ui`, `mcr-app`.
