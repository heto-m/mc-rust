## Context

The sweep ran 28 Fabric mods and 10 libraries for 26.3 from Modrinth through `mcr --mods` offscreen against one
vanilla 26.3 server on port 25567, each alone and then all together, with an autopilot script (inventory hovers,
F3, zoom, third person, chat, looking at blocks) and screenshots. Jars stayed in a worktree-private cache and
were never committed. Results are in `docs/mod-compat.md`.

## Findings that led to fixes

- **Lithium excluded silently.** It requires `mixinextras`, which Fabric Loader 0.19.5 bundles under that id; the
  resolver did not count it as provided.
- **Configs written into the wrong place.** NEA, 3D Skin Layers, Lithium and EntityCulling open `config/…`
  through `java.io`, which resolves relative paths against the process directory and ignores `user.dir`. They
  wrote into whatever directory `mcr` was started from, or failed.
- **Zoom mods had no effect.** Zoomify changes Java's camera FOV, but mcr draws the terrain with its own.
- **EMF and ETF never loaded.** Their `fabric.mod.json` has a raw newline inside a string. Fabric's reader
  accepts it; serde_json does not.
- **All mods together lost Java rendering for the session.** REI and JEI reload their plugins on join and block
  Java's render thread for over 2 s. The hang watchdog detached the bridge for good.

## Decisions

- **MixinExtras.** `HostEnv` provides `mixinextras` at the version bundled with the pinned loader (0.5.5).
- **Working directory.** `Bridge::start` makes every path absolute, passes `-Duser.dir=<game dir>` and moves the
  process into the game dir before creating the VM. The JVM is per process anyway, and both views of the
  directory then agree (log4j's `logs/latest.log` broke when only `user.dir` moved). `mcr` absolutizes its own
  path options first.
- **FOV.** A `CameraMixin` stores Java's computed FOV over vanilla's for the same partial tick in
  `FSTATE_FOV_SCALE` (offset 36). Rust clamps it to 0.02–4, multiplies its camera FOV by it while Java draws the
  world, and scales mouse sensitivity down with it when zooming in. `BRIDGE_VERSION` is 8 on both sides.
- **Lenient manifests.** When strict parsing fails, the scanner retries with ASCII control characters replaced by
  spaces and reports the strict error if that fails too.
- **Recoverable stalls.** A watchdog detach after a missed deadline is recoverable; Java keeps getting frames one
  at a time. After 30 frames in a row that each finish within 250 ms of being posted, the lockstep returns to
  running, emits `RenderResumed` and the user gets a toast. Three recoveries per session; the fourth stall, Java
  stopping or Java's net bridge detaching are final.
- **Startup log.** `mcr` prints one `[mods]` line per mod left out or loaded with a caveat, with the reason.
- **Policy.** Particle Rain is `partial`: mcr keeps drawing its own weather, so the mod's particles add to the
  rain rather than replacing it. The update checks of Mod Menu and IPN still go to the proxy and are refused;
  allow-listing Modrinth is left to a later decision.

## Test hooks

The test mod gained `-Dmcr.testmod.stallMillis` (one bounded render-thread stall, with `hangAfterTicks`), a
relative `config/mcr_testmod.txt` write at init, a mod particle type and a zoom mixin driven by its K key. The
autopilot gained `hover x y` to show tooltips in Java screens.

## Not fixed

- Freecam: the camera is mcr's. Controlling: rebinding vanilla movement keys in Java does not rebind mcr's.
- Continuity and other terrain-model mods: terrain is meshed by mcr.
- Sodium, ImmediatelyFast, Xaero's Minimap and JourneyMap stay blocked (OpenGL).
