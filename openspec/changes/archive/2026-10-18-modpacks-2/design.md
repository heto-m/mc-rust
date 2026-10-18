# Design

## Modded data components on the wire

26.3's clientbound `DataComponentPatch` is not length-delimited: each value is written by its type's stream codec,
and a modded type's codec is the mod's. Rust cannot skip an unknown value without knowing its codec, and asking
Java to decode the packet would put a JVM call on the network thread. Instead Java describes each modded codec
once, when it reports its registries: `McrCodecs` walks codecs built from vanilla's factories by reflection (the
anonymous class's enclosing method names the factory, its captured `val$` fields the parts) and writes a prefix
"wire shape" (`seq 2 vi opt str`, `list item`, `nbt`, …). Rust parses shapes with bounds (1024 tokens, depth 24,
fixed sizes up to 4096 bytes, 64 sequence parts) and skips values by shape with a fuel limit (65536 steps) and
2 MiB per value. The bytes are kept (`DataComponent::Modded`, `Particle::Modded`) so Rust can re-encode them;
modded entity data values are skipped and dropped, as nothing in Rust reads them.

A codec without a shape (a mod's own class, `StreamCodec.of`, `dispatch`) is reported as a registry problem by
class name; packets that use it are still dropped and counted. Medieval: 59 of 63 component types, 27 of 39
particle types have shapes; Echoes: all 11 and 10. Across every run after the change no play packet was skipped
(the baseline skipped a `level_particles` with a Better Combat particle).

## Java-baked models: the fallback

Rust's baker reads block state JSON and block models. Code-generated models (Fabric model loaders, models replaced
at bake time) have no JSON Rust can read. Options were: implement each loader in Rust (unbounded), render those
blocks in Java's world pass (a second terrain path), or let Java bake and Rust mesh. The last keeps one terrain
path: Rust asks Java (`MSG_MODELS`, newline-separated block names, 64 KiB) for blocks whose states it could not
bake; Java answers with every state's quads from `BlockStateModel.collectParts` (positions, sprite-local UVs,
direction, cull face, tint index, shade, emission, sprite name, translucency), the AO flag, particle sprite and
how each tint index is coloured (grass, foliage, dry foliage, water, a constant colour or none). Rust interns the
sprites, compiles the quads like its own and caches per state; a constant tint gets one of 27 palette kinds after
vanilla's (kinds 36 to 62). States with an invisible render shape (liquid blocks) export no quads. Requests and
the report are bounded (4096 blocks, 4096 states per block, 4096 quads per state, 2^20 quads, 65536 sprites).
Medieval: 79 states of 12 blocks come from Java, and none stay unbaked.

## Modded fluids

Java's `FluidModel` (26.3) names still, flowing and overlay sprites, the render layer and a tint source. Java
reports every fluid's model at the title; Rust maps each modded fluid to a family (`FluidKind::Other(n)`, at most
255) with its own sprites, layer and tint and meshes it with the fluid mesher. Medieval has 11 modded families,
Echoes 1 (ichor, 256×256 animated, 32 frames).

## Prebake

Joining a modded server rebuilds Rust's tables and rebaked the model cache (1.07 to 1.3 s). Java's title
registries already hold the modded blocks, so Rust bakes for them in the background at the title (once Java's
fluids arrived, at most three times per title report), and asks Java for the blocks it could not bake. At the join
the prebaked cache is re-indexed for the server's tables (`ModelCache::remap`: same blocks and state counts,
same render inputs per state, different order) and used if it fits; a join waits at most 10 s for a running
prebake. When Java answers a request after the join, the cache is rebaked in the background and swapped in.

## Java's resource packs

Medieval selects 16 packs in Java, Alacrity 32x among them; Rust's terrain showed vanilla textures and missed
sprites at Alacrity's paths. The render report carries Java's selected pack paths; Rust adds those inside the game
dir to its resource stack between mod jars and server packs, and rebakes on change.

Alacrity's models made the cache 233 MB of tables (2.5 million quads) and the client was OOM-killed at a 4.8 GB
cap. Identical compiled quad runs are now stored once (hashing the run, comparing on collision): vanilla goes from
450757 to 202976 quads, Medieval with Alacrity from 2.52 million to 456790 (52 MB), joined 690207 (75 MB).

Sprites come in more sizes than the renderer's four texture groups (Medieval with Alacrity: 34 sprites, Echoes: 3,
the ichor among them), and sprites past the fourth group rendered as missing. The four most used sizes are kept,
16×16 always, and other sprites are resampled to the nearest kept size (box filter where it divides).

## Loading screens

On a modded join, JEI starts in one Java frame under `LevelLoadingScreen` (about 2.3 s on Medieval), which
detached the bridge ("Mods stopped drawing"). Java now flags frames drawn under loading, connect, message and
progress screens and mcr's world screen (`FSTATE_WAITING`); such a frame does not end the loading period, and a
late one keeps the last composite as while Java loads.

## Automatic heap

Live heap after GC: Echoes (12 client mods) 568 MB, Medieval (102) about 1 GB, so about 510 MiB plus 4.8 MiB a mod.
The automatic heap is 2.3 times that (Medieval runs well at 2304 MiB with about 1 GB live), in 256 MiB steps, at
least 2048 MiB, at most 12288 MiB, three quarters of RAM, and RAM less 4.5 GiB for mcr's process (Medieval's
process is 2.3 GB above its heap) and the system. Extrapolated: 300 mods 4608 MiB, 400 mods 5632 MiB on a 16 GB
machine, which the old 4096 MiB ceiling could not hold; packs that need 8 to 10 GB are chosen explicitly up to
the ceiling. The settings and profile files now write `heap_explicit` with a chosen heap; a file written before
(always `2048`, without the flag) reads as automatic. The client JVM runs inside mcr's process, so there is no
scope to scale; the managed singleplayer server's scope already follows its own heap.

## Measurements

Ryzen 7 5800X, RX 6800 XT, offscreen 1280×720, release, `MCR_MOD_PROFILE=frame`, Fabric test server on loopback,
1200 ticks after the join at the same spot as the baseline, warm JVM AOT cache. Another agent's builds ran on the
same machine during the baseline and these runs (load average 3 to 15), so frame times vary by up to 30 %
between runs; ranges are of two runs.

| | Echoes before | Echoes after | Medieval before | Medieval after |
|---|---|---|---|---|
| Title | 9.7 s | 9.9 to 14.4 s | 31.8 s (26.1 to 26.8 in modpacks-1) | 26.4 to 28.6 s |
| Ready after launch | 12.9 s | 14.3 to 16.9 s | 40.4 s | 31.3 to 33.7 s |
| Model cache at join | baked, 1.20 s | prebaked, 0.15 to 0.25 s | baked, 1.07 s | prebaked, 0.33 to 0.34 s |
| Frame mean / p50 / p99 | 1.00 / 0.82 / 3.66 ms | 1.00 / 0.87 / 3.28 ms (quieter run) | 2.53 / 1.72 / 12.96 ms | 2.47 to 2.67 / 2.04 to 2.17 / 7.7 to 8.2 ms |
| Java / Rust calls per frame | 0.38 / 0.16 ms | 0.50 / 0.20 ms | 2.03 / 0.50 ms | 1.34 to 1.48 / 0.38 ms |
| Peak RSS | 3.16 to 3.20 GB | 3.21 to 3.33 GB | 4.36 to 4.41 GB | 4.56 to 4.65 GB |
| Late frames, skipped packets | 0, 0 | 0, 0 | 0, 1 | 0, 0 |
| States from Java, unbaked | 0, 16 | 16 (no quads), 0 | 0, 31 | 79, 0 |

Echoes' later title and ready are the JVM under load (JEI's recipe registration took 1.2 s instead of 0.6 s); the
Rust side of the join went from 1.2 s to 0.15 s. Medieval's RSS grows by the Alacrity textures (41 MB on the CPU
plus their GPU copies) and the model cache (75 MB) that Rust now uses.

Medieval singleplayer (render distance 32): ready 45.5 s after launch (49.1 s before), frame mean 3.18, p50 2.35,
p99 12.27 ms (2.90 / 2.04 / 12.1 before), client RSS 4.52 GB, no "Mods stopped drawing", 0 late frames, and the
second prebake (with Java's 79 states) finished before the join.

## Known

`fabric_boot` (opt-in JVM test) fails at "TCP sockets open at the title screen" on this change and on its base
commit alike: the stock client's allowed request to `discovery.minecraftservices.com` keeps a connection open
through the egress proxy. The assertion now prints the sockets.
