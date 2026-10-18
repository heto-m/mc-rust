# Proposal

## Why

Chat from `/me`, `/msg`, `/say` and `/teammsg` shows as `<X> msg`: mcr answers `select_known_packs` with the
vanilla core pack, so the server leaves out the data of entries the client should know, and mcr had no copy of
it. Every synced registry is affected (chat types, biome climate, dialogs, damage types). The secure chat check
could be fooled by a hostile server's decoration. Edit Game Rules does nothing in mod mode, the launcher cannot
be reached again once the game started, and `net_bridge` was not rerun after the forward list changed.

## What Changes

- **Built-in registry entries.** Entries sent without data are filled from the user's client jar
  (`data/<namespace>/<registry>/<entry>.json`, converted to NBT), for every registry; per-consumer fallbacks go.
  Biome colours read the jar's biome files through the same provider instead of a dev-only data generator folder.
- **Chat type decorations** take key, parameters and style from the entry: `* X msg`, grey italic whispers.
- **Lenient NBT numbers**, as vanilla's `NumericTag`, since JSON numbers carry no tag type.
- **Secure chat trust on the drawn text.** The check joined all translation arguments, so a template without the
  content's slot, a skipping `%2$s`, a language value over a `%s` fallback or a chat type without `content` kept a
  rewritten line Secure. It now runs on the line as mcr-ui draws it with the current language, failing closed
  without a renderer. The unsigned font rule also applies with Only Show Secure Chat, and the local-server rule
  needs a direct join of the running local server's port.
- **Edit Game Rules in mod mode.** `set_game_rule` joins the forward list; the server keeps checking gamemaster
  permission, so the screen works with cheats and stays greyed without, as in vanilla.
- **Back to the launcher** from the native title screen (same process) and from the "mcr" menu on Java's title
  (Java stops cleanly, then mcr runs itself again with `--launcher` and the same `--data-dir`).
- **`net_bridge` rerun**, fixing failures.

## Out of scope

Jars for other versions, a native in-world game rules screen, switching profiles without restarting Java,
colours of biomes the jar does not have.

## Impact

`mcr-client`, `mcr-nbt`, `mcr-app`, `mcr-mesh`, `mcr-audio`, `mcr-ui`, `mcr-modbridge` (forward list),
`docs/mod-bridge.md`. No ABI change.
