# Proposal

## Why

The app joins a server straight from `--connect` with a one-line chat editor and no menus. The UI crate already has the title, server list, container, chat, pause and death screens; the app must host them so the client is usable end to end.

## What Changes

- App flow: title screen, server list backed by `servers.dat` with status pings (MOTD, players, version, latency, favicon), connecting screen, play, disconnected screen back to the list. `--connect` stays a direct shortcut.
- In-game screens: inventory (E), server-opened containers, pause (Esc), death, chat (T and `/`), driven by client container state with click prediction and server resync.
- A client-side Brigadier command tree parsed from the `commands` packet, with local suggestions and `command_suggestion` round trips for server-provided suggestion types.
- Options screens (Video, Sound, Controls, Chat) that write `options.txt` and apply live, including render distance to the server.
- Offscreen autopilot gains UI steps so screens can be captured.

Out of scope: advancements, statistics, merchant trade data, resource pack screens, the 3D player model in the inventory (placeholder only).

## Capabilities

### New Capabilities
- `app-screens`: how the app hosts menus and in-game screens and routes their actions.
- `command-suggestions`: the client-side command tree and suggestion pipeline.

### Modified Capabilities
- `server-list`: status pings expose the favicon and MOTD to the server list screen.

## Impact

`crates/mcr-app` (shell, play, options screens, status ping), `crates/mcr-client` (command tree module, suggestion events, view distance update, container button actions), `crates/mcr-ui` (favicon on `OnlineStatus`, slot geometry accessor).
