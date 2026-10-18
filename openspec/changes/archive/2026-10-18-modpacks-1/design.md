# Design

## Fetching packs

`cargo xtask modpack-fetch <zip>` needs no CurseForge API key. cfwidget (`api.cfwidget.com/<project>`) gives each
project's file list with names and sizes; the CDN URL is `edge.forgecdn.net/files/<id / 1000>/<id % 1000>/<name>`.
Requests to cfwidget are sequential, 600 ms apart, retried while cfwidget says the project is queued, and cached
under `.cache/packs/cfwidget/`. cfwidget's lists go stale (Medieval Immersed's The Deeper End file was missing), so
a file it does not list is looked up once on the CurseForge website's file record
(`www.curseforge.com/api/v1/mods/<project>/files/<file>`, cached as `file-<id>.json`). Downloads land in
`.cache/packs/files/<file id>/` and are hard-linked into `.cache/packs/<slug>/mods/`.

**One flat `mods/` folder.** The first layout put server-only jars in `mods/server/`. Fabric's server needs the
two-sided jars as well, and mcr's client already reads each jar's `environment` and the policy, so the jars of both
sides now sit together and `xtask fabric-server`, mcr's singleplayer and the client each pick their own set
(`Policy::runs_on_server`, `Report::server_jars`). `pack.json` records each file's client and server placement.

### Deny-list

`crates/mcr-modmeta/data/mod-policy.json` stays the only list. The pack fetcher, mcr's manager and the server set
all read it.

| Status | Mods | Client | Server |
|---|---|---|---|
| `blocked` | Sodium, Iris, ImmediatelyFast, Distant Horizons, Xaero's maps, JourneyMap, FancyMenu, Sound Physics Remastered, Sodium Extra, Reese's Sodium Options, Better Block Entities | dropped | dropped when two-sided |
| `unsupported` | Simple Voice Chat, Litematica, Dynamic FPS | dropped | dropped when two-sided |
| `partial` | Continuity, More Culling, BadOptimizations, Bobby | kept (no visible effect) | per environment |
| `server` (new) | Lithium, C2ME, Krypton | dropped | kept |

A two-sided mod left out of the client is left off the server too, because a server-side content mod the client
lacks fails registry sync. Shader packs are skipped (they need Iris). Medieval Immersed: 113 jars, 102 for the
client, 89 for the server; dropped from the client are the eleven blocked or unsupported mods above that it ships,
Lithium, and SlashLoot (server-only).

### FerriteCore

FerriteCore shrinks Java's block state property maps. In mod mode Rust holds the canonical tables, but Java's
client still builds its own, so it can matter only for Java's heap. Measured with the same heap, server and
1800 ticks (live heap after a forced full GC, via `jcmd GC.run` and `GC.heap_info`):

| Pack | With FerriteCore | Without |
|---|---|---|
| Echoes of the Sift | 568 MB live, frame mean 0.90 ms, RSS 3158 MB | 564 MB live, 0.88 ms, 3183 MB |
| Medieval Immersed | 994 MB live, frame mean 1.68 ms (p99 5.20), RSS 4412 MB | 1062 MB live, 1.67 ms (p99 4.86), 4398 MB |

It saves about 70 MB of live heap on the large pack and nothing measurable on the small one; frame time and title
time are the same. **Keep it** (not deny-listed): it is harmless, packs ship it, and removing it would only make
Java's heap larger. Peak RSS is set by the heap cap, not by FerriteCore.

## Modded registries

Rust decodes every packet and owns terrain, physics and light. A Fabric server's registry sync changes Java's
numbering, and mods add properties to vanilla blocks: Echoes of the Sift's `IchorStateDefinitionMixin` adds
`ichorlogged` to every waterloggable block, so Java has 70472 block states against vanilla's 35723, and every id
after the first waterloggable block moves. Appending modded blocks after vanilla's tables cannot express that.

**Full rebuild.** `McrRegistries` (Java) exports format 2: every block from id 0 with its properties and, per
state, collision, outline and occlusion shapes, light emission, light block, flags, map colour, sturdy faces,
occlusion faces and fluid state; shapes and fluid states deduplicated; items (stack size, damage, block, tool
rules, equipment slot, use and cooldown times, food), entity types (size, eye height, category) and fluids from 0;
and every static registry's names. `mcr_data::rebuild` turns that into a new `Tables`, requiring vanilla's registries
to be a prefix of Java's, checking every index and bound (2^20 states, 2^16 shapes, 256 boxes per shape, 32
properties, 1024 values), and `Version::rebuild` installs it as the active version. `export_of` is the inverse, so a vanilla round trip is a unit test.

**When.** Java reports once at the title screen and again from `ClientConfigurationPacketListenerImpl` at the end
of configuration (after Fabric's registry sync). Rust holds `FinishConfiguration` until the report arrives (at
most 10 s, then it continues with the current tables and says so), switches tables, then acknowledges, so the
first chunk is decoded with the server's state count. Rust's configuration pings wait for Java's pongs while Java
answers custom queries; without that, Rust's pong overtook Fabric's channel registration and the server refused
the client.

**Per-version caches.** Tables derived lazily from a `Version` (physics data, light table, spawner entity names)
are `PerVersion<T>`: one `OnceLock` per installed version slot, so a rebuilt version gets its own cache.

### 32-bit block state ids

`BlockState` went from `u16` to `u32` (`WorldConfig::MAX_BLOCK_STATES` = 2^20); biomes stay 16-bit. The direct
palette uses `ceil(log2(count))` bits as vanilla does. Packed storage is generic over a `Slot` trait for `u16` and
`u32`. Section updates with a state past the active count are dropped. Benchmarks against `main`: meshing
unchanged, section snapshot +6%, paletted container within ±8%. Palette entries take 4 bytes instead of 2; the
bit-packed data is unchanged.

## Rendering approach

**Decision: Rust's own baker with the mods' resources.** Mod jars (and their nested jars that carry assets) are
pushed into Rust's resource stack as assets-only packs, so blockstate JSONs, models and textures resolve exactly as
vanilla's do, and modded blocks mesh with the rest of the terrain at no per-frame cost. The model cache is rebaked
when the active tables change (about 1.0 to 1.3 s for 65k to 70k states). Biome colours come from the server's
registry entries, since modded biomes are not in the client jar.

What stays unbaked is logged by name: Echoes 16 states (`the_sift:ichor`, a modded fluid: Rust's fluid renderer
knows only water and lava), Medieval 31 (Easy Anvils' and Visual Workbench's replaced vanilla models, Kaleidoscope
Tavern's string lights and pressing tub, which use code-registered or custom-loader models). Java draws block
entities and entities in mod mode, so the tavern's block entity parts still show.

**Fallback (future):** export Java-baked quads for states Rust cannot bake (`BlockStateModel.collectParts` per
state, sent once over the bridge after the resource reload) and mesh them as static quads. Not built yet; no tested
pack needs it badly enough. Modded fluids need their own path (Fabric's `FluidRenderHandler` sprites and tint).

## Modded singleplayer

When a server-run mod ships content (`ModInfo.content`: blockstates in `assets/` or data other than tags), the
managed server is Fabric's `KnotServer` over the same server jar: `-Dfabric.gameJarPath=<server.jar>`,
`-Dfabric.addMods=@mcr-mods.txt` (absolute jar paths, one per line) and a class path from `.cache/fabric/server.json`
(written by `xtask fabric-fetch`, every path checked to stay inside the cache). Fabric logs `Done` before its
started hooks finish and drops connections made in that window, so readiness is a probe: mcr sends `list` and
waits for the answer on the console. `MCR_SP_PORT` overrides the free-port pick (tests, and keeping off the owner's
server).

### Egress

The managed Fabric server runs mods' code, so it gets the client JVM's fence, built by the same function
(`mcr_modbridge::egress_properties`): a localhost-only `jdk.net.hosts.file` (written into the server dir) and
`http(s).proxyHost=127.0.0.1` / `http(s).proxyPort` of the bridge's running loopback proxy, which applies the
policy's `http_hosts` allow-list. The client sets no `nonProxyHosts` (Java's default, loopback only) and so does the
server. Without a running proxy the Fabric server does not start. The vanilla launch is unchanged.

Limits, the same on both sides: the fence covers DNS (Java resolves only localhost) and Java's HTTP stack
(`HttpURLConnection`, `HttpClient` honour the proxy properties). It does not stop raw sockets to literal IP
addresses (`new Socket("1.2.3.4", 80)`), nor native code. The client additionally refuses `Connection.connect`
outside `ConnectScreen`, which has no server counterpart (the server only listens). `cargo xtask fabric-server`
is a development tool for the owner's own packs and runs without the fence; its help says so.

## Measurements

Ryzen 7 5800X, RX 6800 XT, Linux, offscreen at 1280x720, release build, `MCR_MOD_PROFILE=frame`, the Fabric test
server on loopback (view distance 10, 473 chunks) or mcr's singleplayer (render distance 32, about 3500 chunks).
Frame time is mcr's whole frame; Java and Rust-calls are per frame from the profile; RSS is the client process's
peak (the JVM lives inside it). The Fabric server ran in its own scope (Medieval: 1792 MiB heap, 2.2 to 2.3 GB
peak). Title and join times are with a warm JVM AOT cache; a run that records the cache (first run after the mod
set changes) adds 4 to 11 s to the title.

| | Echoes of the Sift | Medieval Immersed |
|---|---|---|
| Mods (client / server) | 12 / 12 of 19 jars | 102 / 89 of 113 jars |
| Block states, blocks, items, entity types | 70472, 1382, 1796, 171 | 64836, 1892, 2706, 230 |
| Title | 9.7 to 9.8 s | 26.1 to 26.8 s |
| Joined (dedicated) | 12.4 s after launch | 28.6 to 29.4 s |
| Rebake on join | 1.2 to 1.3 s | 1.0 to 1.1 s |
| Frame (dedicated) | mean 0.90, p50 0.80, p99 2.5 to 2.9 ms | mean 1.68, p50 1.44, p99 4.9 to 5.2 ms |
| Profile fps (incl. wait) | about 530 | about 380 |
| Java per frame | 0.39 ms | 0.92 to 0.97 ms |
| Rust calls per frame | 0.18 ms | 0.27 ms |
| Client heap / live after GC | 2048 MiB / 568 MB | 2304 MiB / 994 MB |
| Peak RSS (dedicated) | 3.16 to 3.20 GB | 4.36 to 4.41 GB |
| Singleplayer frame | mean 1.97, p50 1.43, p99 10.7 ms | mean 2.90, p50 2.04, p99 12.1 ms |
| Singleplayer RSS (client) | 3.26 GB | 4.14 GB (2048 MiB heap) |
| Singleplayer join | not recorded | 49.1 s after launch (server start 17 s) |

Java's frame (GUI, entities, block entities through the bridge) is the larger share in both packs: 0.39 of 0.90 ms
and 0.95 of 1.68 ms. Medieval at 2560 MiB heap was OOM-killed once at a 4.5 GB cap during the resource reload; 2304
MiB fits in 4.4 GB with about 1 GB live, so big packs want a heap default by mod count (owner decision).

**Official launcher.** `tools/official-launcher/measure_pack.py` builds the same pack as an instance for the
official launcher (`prepare <slug> --set mcr|pack`), adds a Fabric installation with the same heap, GC and window
size, and `watch` reports title and join time, peak RSS and MangoHud's per-frame times in the shape of mcr's
summary, against the same `xtask fabric-server`. `--set pack` includes Sodium and the other mods mcr blocks, for
the pack as its players run it.
