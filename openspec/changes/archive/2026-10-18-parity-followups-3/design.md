# Design

## Built-in registry entries

Vanilla's client answers `select_known_packs` with the packs it ships (`minecraft:core` for its version). The
server then sends every entry of a synced registry by name, and leaves out the data of entries that come
unchanged from a pack both sides know. The client reads that data from its own copy of the pack. mcr already
sent the same answer, but kept such entries without data, and a few consumers patched the hole on their own
(dimension shapes from a table, damage effects from a table, audio and visual attributes and jukebox songs
from the jar). Chat types, biome precipitation and temperature, dialogs and biome colours had no fallback.

`mcr_client::Registries` gets an optional `BuiltinData` provider: given a registry and an entry id it returns
the entry compound or nothing. `apply_registry_data` asks it for every entry the server sent without data, under
the same per-entry and total NBT byte budgets as server data, plus a cap on lookups per session, so a hostile
server naming thousands of entries cannot make the client parse the jar's files without end. The app builds the
provider from the client jar: `data/<entry namespace>/<registry dir>/<entry path>.json`, where the registry dir
is the registry path for `minecraft:` registries and `<namespace>/<path>` otherwise (vanilla's
`Registries.elementsDirPath`). Only the vanilla jar is read, never resource packs. Ids are checked before they
become a path (lowercase `[a-z0-9_.-/]`, no `..`).

JSON becomes NBT the way vanilla's codecs would see it: booleans are bytes, integers ints (longs when out of
range), other numbers doubles, strings strings, objects compounds. Arrays become typed lists; mixed lists wrap
each element in a compound under the empty key, as vanilla's heterogeneous lists do. The converter lives in
`mcr-nbt` (`json` module) with a depth limit, and replaces `mcr-audio`'s compounds-only copy.

JSON carries no tag types: a biome's temperature is a float on the wire and a double from JSON. Vanilla's codecs
read numbers through `NumericTag` and accept any numeric type, so `Value::as_i8` … `as_f64` (and the compound
getters on top of them) now convert between numeric tags the way Java's casts do (narrowing integers wrap,
floating to integer saturates). Nothing in mcr relied on a getter refusing another numeric type.

With every entry filled, consumers read only the registry: the per-consumer jar fallbacks in ambience and
animation go. Biome colours are built per biome name from the same provider (`BiomeClimate::from_nbt` on the
jar's biome file) instead of the data generator folder next to the jar, which only exists in a dev checkout;
biomes the jar does not have keep plains colours as before.

## Chat type decorations

`format_chat` takes the decoration's `translation_key`, `parameters` and `style` from the chat type entry, and the
style of inline chat types too. With the registry filled, `/me` uses `chat.type.emote`, `/say`
`chat.type.announcement`, `/msg` `commands.message.display.incoming`/`outgoing` (grey, italic), and `/teammsg`
`chat.type.team.text`/`sent`. The `chat.type.text` fallback stays for entries with no usable data.

## Secure chat trust on the drawn text

Vanilla's `ChatTrustLevel.evaluate` checks `decorated.getString().contains(signedContent)`, where `getString`
resolves translations with the client's language. mcr's check concatenated the plain text of all translation
arguments without the template, so a template that drops the argument, a `%2$s` that skips it, a language value
without `%s` (where only the fallback had one) or a chat type whose translation leaves out `content` all passed
as Secure while the player read something else.

There is now one source of truth: the text the UI draws. `mcr_client::TextRenderer` wraps a closure from
component to text; the app builds it from `mcr_ui::flatten` (the same flattening the chat HUD draws, with its
budgets, positional rules and legacy `§` stripping) over the GUI's current language, which `Gui` publishes in an
`Arc<RwLock<Arc<Lang>>>` whenever it loads one. `ClientConfig.text_renderer` carries it into `SecureChat`, and
the old argument-joining helper is gone. Checking the drawn text is stricter than vanilla's `getString` in one
way: legacy codes are removed first, so "§I agree" no longer passes for "I agree". Without a renderer
(headless, tests that do not set one) a decorated line that contains a translation, or a `§`, is Modified:
the shown text is unknown, so the check fails closed.

The unsigned-content font rule uses the packet's unsigned content even when Only Show Secure Chat replaces it
with the signed text, as vanilla does. "Own message on the local server is Secure" applies only when the
connection is not a transfer, goes to `127.0.0.1` and uses the port of the running local server
(`Session::local_port`, set once the server is ready); previously any localhost address counted while a session
existed, including one still shutting down.

## Edit Game Rules in mod mode

26.3's `InWorldGameRulesScreen` works over the network on any server: it sends `client_command`
(`REQUEST_GAMERULE_VALUES`), fills itself from `game_rule_values`, and on Done sends `set_game_rule` with the
changed rules. The dedicated server answers both only for players with gamemaster permission. World Options
enables the button for a player with that permission (or, with an integrated server, for the host when commands
are allowed). In mod mode `client_command` was already forwarded and `game_rule_values` reaches Java with the
clientbound copy, but `set_game_rule` was dropped by the outbound filter, so Done changed nothing.

`set_game_rule` joins the forward list. In mcr's local worlds the owner is an operator at level 4 exactly when the
world allows cheats, so the button is active with cheats and greyed without them, as in vanilla singleplayer;
on remote servers it follows the operator level. Routing through the server console with `gamerule` (as the
difficulty change does) was considered and dropped: the console has no permission check, so it would let a
player without cheats change rules, which vanilla does not allow, and the values Java shows already come from
the server's own answer. The forward list is Rust-side only, so the ABI stays at 19.

## Back to the launcher

`ScreenAction::Launcher` comes from the native title screen's new Launcher button (it shares the Mods row) and
from an entry in the "mcr" menu while no world is open (Java's title screen opens that menu). The shell sets
`to_launcher`, the app leaves the shell scene and:

- **No JVM in this process:** drops the shell (renderer, audio, assets) and creates the launcher scene on the same
  window, as at startup. Play then runs the normal startup path, which may boot Java for the first time.
- **A JVM was started in this process** (successfully or not): the JVM cannot be created twice in one process.
  The app leaves the event loop, `ModHost::finish` shuts Java down as on quit (session record, AOT recording),
  and mcr replaces itself (`exec` on Unix, spawn and exit elsewhere) with `--launcher --data-dir <dir>`; an
  offscreen run keeps `--screenshot` and `--size` so it stays offscreen. If Java did not stop cleanly, the exec
  still happens, which also ends the parked VM. Trade-off: the window closes and reopens, Java's boot cost is
  paid again on the next Play, and any other flags of the first run are dropped.

The offscreen runner loops the same way: after a shell that asked for the launcher, the launcher runs again, so
the round trip is testable without a window.

## Risks

- Lenient numeric getters could hide a type mistake in mcr's own NBT writers; they match what vanilla accepts.
- Filled biome entries hold the full data pack JSON (spawns, features), larger than the network codec's subset;
  the jar's synced registries total about 0.4 MB of JSON, well inside the 16 MiB budget.
