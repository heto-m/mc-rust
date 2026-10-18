# Design

## Open to LAN

Vanilla's integrated server opens a second TCP listener on all interfaces. mcr's server is a dedicated server
bound to `127.0.0.1` on a random port, and its `server.properties` is only read at start, so the listener cannot
be added to the running server. A forwarder sits in front of it instead (`mcr_singleplayer::lan`):

- `Forwarder::start(bind, target, gate)` binds a `TcpListener` on `bind` (`0.0.0.0:<port>`, port 0 for "any free port"),
  sets it non-blocking and polls `accept` every 50 ms on one thread, so stopping needs no wake-up trick.
- Each accepted connection first goes through the login gate on its own thread (below). Only then does it open a
  blocking `TcpStream` to the loopback server, replay the bytes the gate read, and get two copy threads, each with a
  fixed 16 KiB buffer. At most 16 connections live at once; more are accepted and closed at once. `TCP_NODELAY` is
  set on both sides. When one direction ends, both sockets are shut down.
- `stop()` (and `Drop`) clears the run flag, shuts down every live socket (clones kept in a bounded list) and joins
  the accept thread. Copy threads end when their sockets are shut down.
- Every LAN player reaches the server from `127.0.0.1`. That is harmless with `online-mode=false` and no IP bans.

### Login gate (host impersonation)

The server runs with `online-mode=false`, so a player's UUID is derived from the name alone, and `ops.json` ops the
host's offline UUID when cheats are on. Without a check, anyone on the LAN could log in as the host's name and
become op, or kick the host with a duplicate login. The forwarder therefore reads the opening of each connection
before piping:

- The handshake (`intention`) and, for login and transfer intents, the login start (`hello`: name, UUID) are
  decoded with `mcr-protocol`'s codecs. Frame lengths are capped (781 bytes for the handshake, 68 for the login
  start), at most 1 KiB is read before the decision, and the whole opening must arrive within 5 s (a read
  timeout set to the time left), so a slow client only ties up its own gate thread and connection slot. Status
  pings pass unchanged. A leading `0xFE` (legacy ping), a wrong packet id, a bad intent, an over-long name or any
  other garbage closes the connection without reaching the server.
- A login whose name equals the host's name ignoring ASCII case, or whose UUID equals the host's offline UUID, is
  refused with a login disconnect (`multiplayer.disconnect.name_taken`) and closed. So is a name already online
  through the forwarder (tracked lowercase per live connection and released when it closes), so a guest cannot
  kick another guest. The host never comes through the forwarder, and its name is always taken.
- `ops.json` only ever holds the host (written at every launch), and Allow Cheats on the LAN screen ops or deops
  the host only, as in vanilla. A guest gets op only if the host types `/op` for them during the session.

Accepted risk: processes on the same machine can still connect to the loopback port directly and skip the gate.
Such a process already runs as the user and can read and write the world, `ops.json` and the client's files, so
the gate would not protect anything from it.

`Announcer::start(motd, port, target)` sends `[MOTD]<motd>[/MOTD][AD]<port>[/AD]` from an unbound UDP socket every
1.5 s until stopped (a condvar wait, so stop is immediate). The target is vanilla's `224.0.2.60:4445`. The MOTD
is `<player> - <world name>` like the integrated server's. Controls, `[` and `]` are removed and it is cut at 64
characters, because a world name like `x[/MOTD][AD]25565[/AD]` would otherwise advertise another port.

`MCR_LAN_BIND` (an IPv4 address) overrides the bind address for development and tests. When it is a loopback
address, announcements go to `127.0.0.1:4445` instead of the multicast group, so nothing leaves the machine.
Automated tests only use loopback.

The choices are applied with console commands after the forwarder is up: `defaultgamemode <mode>`, then
`gamemode <mode> <host>`, then `op <host>` or `deop <host>` when Allow Cheats differs from the world's own setting.
26.3's integrated server can also give cheats to guests ("guest command access"). That is out of scope:
only the host is opped. `ops.json` is rewritten from the world's own setting on the next launch, so LAN cheats
last only for the session, as in vanilla. `max-players` goes from 4 to vanilla's integrated 8.

Vanilla stops pausing once a world is published. So while LAN is open, the pause menu no longer freezes the
tick, natively or in mod mode. Save and Quit stops the announcer and the forwarder before the server.

### Native screens

The pause menu shows Options and Open to LAN (`menu.multiplayerOptions.button`) side by side in a local world that is not
published yet, like pre-26 vanilla. The LAN screen (`lanServer.title`) has:

- Game Mode: survival, creative, adventure or spectator, starting from the host's current mode.
- Allow Cheats, starting from the world's setting. Both buttons are locked in hardcore.
- Port Number (`lanServer.port`): empty means any free port; otherwise 1024 to 65535, with vanilla's invalid and
  unavailable messages. Availability is checked through a callback the app provides, so UI tests never bind a
  socket.
- Start LAN World and Cancel.

On success, chat shows `menu.multiplayerOptions.publish.started.lan` with the port. On failure it shows
`commands.publish.failed`.

### Mod mode

Java's World Options screen builds its multiplayer section only for an integrated server. A mixin on the end of
`generalOptions` adds a Multiplayer header and an Open to LAN button while `McrWorlds.local()`. Once published,
the button reads "LAN: port N" and is inactive. The button opens `McrLanScreen`, a small Java screen with the same
fields that sends the report `lan\tmode=<m>\tcheats=<0|1>\tport=<n|0>`. Rust answers with `WORLD_LAN` (text
`ok\t<port>` or `failed\t<reason>`), and Java adds vanilla's chat line. Java's pause rule also checks
`!McrWorlds.published()`.

## World icons

The native list already decodes `icon.png` like server favicons: at most 1 MiB, exactly 64×64, size checked
before pixels are allocated, at most 64 sprites cached. That stays.

Writing one needs a picture of the world without GUI. `Renderer::request_capture()` makes the next frame copy its
colour target to a readback buffer right after the world and post passes, before the UI overlay and Java's
`before_ui` hook. In the direct-to-surface path, a requested frame uses the intermediate window texture (the
post-chain path), because swapchain images cannot be copied. The buffer is mapped once, synchronously, and the
pixels (RGBA or BGRA) are cropped to the centre square and box-filtered to 64×64 on the CPU.

The shell asks for a capture when a local world without `icon.png` starts pausing (native pause screen or Java's
`JAVA_PAUSING`), the moment the player opens the menu. On Save and Quit the latest capture is PNG-encoded and
written with `safe::write_file` into the canonical world folder, only if `icon.png` is still missing (vanilla
writes it once).

## Re-Create

`mcr_singleplayer::recreate::read(world)` reads, all bounded like `level.dat`:

- `level.dat`: name, game type, hardcore, difficulty, `allowCommands`, `DataPacks.Enabled`.
- `data/minecraft/world_gen_settings.dat` (26.x saved data) or the old `Data.WorldGenSettings`: seed,
  `generate_features`, `bonus_chest`, and the overworld generator.
- `data/minecraft/game_rules.dat` or the old `Data.GameRules`: values that differ from the defaults.

The overworld generator maps to a preset:

| Generator | Preset |
|---|---|
| `minecraft:flat` | superflat, its settings as `generator-settings` JSON |
| noise `minecraft:amplified` | amplified |
| noise `minecraft:large_biomes` | large biomes |
| noise `minecraft:overworld` | normal |
| anything else (single biome, debug, custom) | refused: "This world type cannot be re-created" |

Cheats come from mcr's sidecar file first. Experiments are the enabled feature packs. Data packs are the world's
own `datapacks` entries, with that folder as the copy source. The native world list's bottom row gets a
Re-Create button (Edit, Delete, Re-Create, Back, as in vanilla) that pushes Create New World with these settings. Java's Re-Create already ends in the create hook.

## Creation options

`CreateWorldSettings` gains `game_rules` (changed rules as id/value), `experiments` (feature pack ids),
`data_packs` (names), `pack_dir` (their source folder) and `flat` (superflat settings: a vanilla preset string
`layers;biome` or the generator JSON).

- **Game rules.** `mcr_data::game_rules` holds 26.3's 59 rules: id, category, bool or int with bounds, default. A
  test checks it against the generated `commands.json` when it is present. Values are validated and unknown ids
  are dropped. On the first launch the shell sends `gamerule <id> <value>` at `Ready`, before joining. The server
  saves them in `game_rules.dat`. The native Game Rules screen lists them by category, with toggles and number
  boxes, using vanilla's lang keys or the id when a key is missing.
- **Experiments.** Known feature packs (`minecart_improvements`, `redstone_experiments`, `trade_rebalance`) become
  `initial-enabled-packs=vanilla,<ids>` on the first launch. Unknown ids are dropped.
- **Data packs.** Each name is one path component inside `pack_dir`. It is either a regular `.zip` file of at most
  64 MiB that starts with a local file header, or a real directory with `pack.mcmeta` (walk of at most 10,000
  entries, 16 deep, 256 MiB, regular files only, no symlinks). Packs are copied into `<world>/datapacks` with
  `ensure_dir` and `write_file`, at most 64 of them, before the first start. World packs load automatically on a
  new world. The native Data Packs screen lists `<game dir>/datapacks` (Open Pack Folder creates it) plus the
  experiments.
- **Superflat.** A preset string (`[n*]block,…;biome`, at most 64 layers) or the generator JSON is turned into
  compact JSON: at most 64 layers, heights 0 to 4064 and a total of at most 4064, namespaced lowercase ids, known
  blocks when block data is available, optional biome, features, lakes and up to 64 structure overrides. Unknown
  keys are dropped. It goes to `generator-settings` on the first launch. The native Customize button next to
  World Type (superflat only) opens a screen with the preset string and vanilla's Classic Flat default.
- **Refused types.** Java's single biome and debug worlds keep the toast. A Java test checks that no request and no
  folder result from them.

Java's create hook adds `rule.<id>=<value>` lines (changed rules only), `experiments=` (comma list), `packs=` (the
enabled `file/` packs), `packdir=` (the absolute temp pack folder) and `flat=` (the flat settings encoded with
`FlatLevelGeneratorSettings.CODEC` to JSON). Java keeps its temp pack folder until the session reports ready,
failed or stopped, then deletes it. Enabled packs Rust cannot load (mod packs such as `fabric`) still get the
toast. Rust bounds the request at 16 KiB and fields at 256 bytes, except `flat` (8 KiB) and `packdir` (4 KiB). At
most 128 rule lines and 64 packs are read.

## Mod mode gaps

- `DialogScreen.WarningScreen`'s constructor reads `isLocalServer()` for its button label. A redirect adds
  `McrWorlds.local()`, as on the pause screen. The native warning screen gets the same label in a local world.
- Crash. `WORLD_STOPPED` with a reason while Java is in the world, or shows a disconnected or saving screen, opens
  `DisconnectedScreen(worldList, "disconnect.lost", reason, "gui.toWorld")`. Later disconnect screens from the
  closing connection are replaced by it, for one second, through a check in `McrWorlds.pump`.
- `WorldOpenFlows.createFreshLevel` is cancelled at HEAD in mod mode with a toast, before `createWorldAccess` makes
  the folder, and Java returns to the screen the mod passed.

## ABI 18

`REPORT_WORLD` gains the `lan` verb. `MSG_WORLD` gains `WORLD_LAN = 6`. Create requests carry the new keys.
BRIDGE_VERSION becomes 18. If main has moved on when this merges, it takes the next free number.
