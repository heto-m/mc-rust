# Design

## Choice: Java's screens, mcr's server (option b)

Two ways were possible: (a) show mcr's native world list and create screens over Java's GUI, or (b) keep
Java's `SelectWorldScreen`/`CreateWorldScreen` and intercept Play and Create. (b) is used:

- Java's `LevelStorageSource` reads any vanilla saves folder; only its base directory differs (`<game
  dir>/java/saves` by default). Redirecting it to mcr's `<game dir>/saves` gives both clients the same list, and
  mcr's `mcr-server/` folder inside a world is ignored by Java's scan.
- Every creation option mcr's native screen offers exists on Java's screen (`WorldCreationUiState`), so the
  mapping is lossless for what mcr supports. What mcr cannot do is refused or reported, not silently changed.
- Mods that add widgets to the world list or creation screen keep working, and the look is vanilla's.

## Saves redirect

`BridgeConfig::saves_dir` passes `-Dmcr.bridge.saves=<abs>` and `-Dmcr.bridge.backups=<abs>`. A
`LevelStorageSource` constructor mixin swaps the base and backup directories when the base is Java's own
`<game dir>/saves`, so only the client's storage moves.

## Interception points

| Hook | Effect |
|---|---|
| `WorldOpenFlows.openWorld(String, Runnable)` HEAD | cancel; `REPORT_WORLD play\t<folder>`; Java shows `McrWorldScreen` |
| `CreateWorldScreen.createWorldAndCleanup` HEAD | read `getUiState()`; unsupported type or debug mode: toast and stay; else `REPORT_WORLD create` and cancel (temp pack dir removed) |
| `Minecraft.doWorldLoad` HEAD | never reached by vanilla paths now; any other caller (a mod) is cancelled, the access closed, and the title shown |
| `LevelStorageSource.createAccess`/`validateAndCreateAccess` HEAD | throw `IOException("the world is open in mcr")` while mcr's world is busy |

`openWorld` is the one path vanilla uses to play (world list, keyboard selection, quick play), and
`createWorldAndCleanup` runs after vanilla's experimental-settings confirmation, right before the callback that
would create the folder. Java's Edit, Delete, Re-Create, backups and Optimize keep their vanilla behaviour when no
local world is open; Re-Create ends in the create hook.

## Messages (ABI 17)

Java to Rust, `REPORT_WORLD` text, first line the verb:

- `play\t<folder>`
- `create` then `key=value` lines: `name`, `mode` (`survival|hardcore|creative`), `difficulty` (0–3), `cheats`,
  `preset` (`normal|flat|large_biomes|amplified`), `seed`, `structures`, `bonus` (booleans `0|1`)
- `eula\t1` / `eula\t0`
- `cancel`

Rust parses it bounded (64 KiB report, 256-byte fields, folder a single path component), drops unknown keys,
and the shell queues at most 16 requests.

Rust to Java, control message `MSG_WORLD`: `[u32 kind][utf8 text]`, kinds `WORLD_STATUS` (progress line, as
`lang:key|arg` or plain), `WORLD_EULA`, `WORLD_READY` (server up, Rust connecting), `WORLD_FAILED` (reason) and
`WORLD_STOPPED` (reason when not requested). They are queued in order (at most 32, consecutive status lines
coalesce) and pushed with the next frame like `MSG_KEYS`.

Input state flag `JAVA_PAUSING` = vanilla's `Gui.isPausing()`, published with the other Java flags.

## Java state

`McrWorlds` holds `IDLE`, `PREPARING`, `LOCAL` (server ready or in game) and `STOPPING`; anything but `IDLE` is
busy, and Play or Create while busy only shows a toast. Only `WORLD_FAILED` (nothing runs) and `WORLD_STOPPED`
return to `IDLE`; Rust answers a `cancel` without a session with `WORLD_STOPPED`. `McrWorldScreen` shows status lines and Cancel (`cancel` report, back to the world list); a status
message while idle opens it (the `--singleplayer` path). `WORLD_EULA` opens a vanilla `ConfirmScreen` with the
EULA link. `WORLD_FAILED` opens an `AlertScreen` back to the world list. On `WORLD_READY` the next `NET_OPEN`
starts `ConnectScreen` with `SelectWorldScreen(TitleScreen)` as its parent, so a failed join or any later
disconnect returns to the world list.

## Pause and quit

While `LOCAL` and in game:

- `PauseScreen` sees `isLocalServer()` as true, so the button reads "Save and Quit to Title".
  The real field stays false: vanilla code that sees it true also dereferences `getSingleplayerServer()`.
- `Minecraft.pauseGame` passes "can really pause", so F3+Esc opens the menu-less pause screen like vanilla.
- The `pause` store at the end of `runTick` is redirected to `Gui.isPausing()` (vanilla's rule without an
  integrated server); vanilla's own next lines then pause non-music sounds, so mods see `isPaused()` as in vanilla
  singleplayer.
- `disconnectFromWorld` takes the local path (saving screen); its title screen is replaced by `McrWorldScreen` in
  saving mode until `WORLD_STOPPED`, then the title. Java's disconnect closes Rust's connection; the shell's
  session end stops the server (`save-all flush`, `stop`, bounded wait, kill).
- Open to LAN lives in `WorldOptionsScreen` behind `getSingleplayerServer() != null`, which is never true.

Rust freezes the server while Java's GUI is composed, a local world runs and `JAVA_PAUSING` is set; otherwise
it uses the native pause screen rule.

## Shell flow in mod mode

A world started from Java (or `--singleplayer` in mod mode) is a *Java-flow* session: no native connecting,
EULA, failure or saving screens are pushed (they would become an overlay above Java's GUI), and every session
update goes to Java instead. The EULA answer arrives as a request; the pending launch waits for it. When a
session ends while Java's GUI is composed, the native disconnected screen is not pushed either; Java shows its
own. A failed join of a Java-flow session stops the server and reaches Java as the `WORLD_STOPPED` reason once it
exited (Java's connect screen shows the failure instead if it was already open).

## Not done

Game rules, data packs and experiments from Java's create screen; customised superflat layers and single-biome
worlds; Java's `icon.png` auto-screenshot (it needs an integrated server); LAN; the "Save and Quit" label on the
custom-dialog warning screen. A mod calling `WorldOpenFlows.createFreshLevel` while idle still leaves the empty
folder Java created before `doWorldLoad` is cancelled.
