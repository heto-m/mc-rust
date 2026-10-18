# Proposal

## Why

The owner wants real CurseForge modpacks to play well in mod mode. Two 26.3 Fabric packs (Echoes of the Sift,
19 mods; Medieval Immersed, 134 mods) were tried. Content mods need the server to run them too, the server then
syncs modded registries, and Rust, which decodes every packet and owns terrain, physics and light, only knows
vanilla's registries: the first run joined but showed no terrain (every chunk with a modded block was dropped).
A Fabric server also refused mcr outright, because Rust answered its configuration ping before Java's channel
registration reached it.

## What Changes

- **`cargo xtask modpack-fetch <zip>`** downloads a CurseForge pack's files without an API key (cfwidget for
  file names, the CDN for files, cached under `.cache/packs/`), places each jar for the client and the server
  (one flat `mods/` folder, placements in `pack.json`), and installs the overrides into a game dir. The
  deny-list stays in one place, `mod-policy.json`, which gains a `server` status (Lithium, C2ME, Krypton).
- **`cargo xtask fabric-server`** runs a Fabric dedicated server with a pack's server jars on loopback.
- **Modded singleplayer.** When the active mods ship content, mcr's managed server is a Fabric server with the
  two-sided and server-only jars, in the same capped scope with the same pid-file proof
  and the client JVM's egress fence.
- **Modded registries in Rust (ABI 21).** Java exports its registries after Fabric's registry sync (and once at
  the title screen): names of every static registry, and for every block, block state, item, entity type and
  fluid the data the vanilla extractor writes. Rust rebuilds its tables from them (mods add properties to vanilla
  blocks, which renumbers vanilla's states), widens block state ids to 32 bits, makes the rebuilt tables active, and only then acknowledges the end of configuration, so the first chunk is
  decoded with the server's block state count. Mod jars join Rust's resource stack, so modded blocks bake with
  mcr's own model baker and mesh like vanilla ones.
- **Undecodable packets in mod mode** are dropped and counted instead of disconnecting, since Java decodes and
  applies the same frame.
- **Configuration pings in mod mode** are answered by Java, in order with its custom payloads.
- Measurements of both packs (frame time, Java share, RSS, title and join time) and a script for the owner to
  measure the same pack in the official launcher.

## Out of scope

Forge/NeoForge packs (the fetcher refuses them), CurseForge packs for other game versions, KubeJS scripting
beyond copying its files, Java-side model export for blocks whose models are code-generated (listed as a
follow-up when a pack needs it), modded fluids' rendering, and resource pack parity between Java and Rust.

## Capabilities

### New Capabilities
- `modpacks`: fetching CurseForge packs, the client/server split and the Fabric dedicated server for testing.
- `mod-registries`: modded registries synced from a modded server reach Rust's tables, renderer and physics.

### Modified Capabilities
- `mod-net-bridge`: configuration pings answered by Java; undecodable packets dropped in mod mode.
- `mod-singleplayer`: a Fabric server runs worlds when the active mods ship content.
