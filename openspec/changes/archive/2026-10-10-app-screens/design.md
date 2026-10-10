## Context

`Play` owns the renderer, UI, HUD and the connected client. Menus need those resources before any client exists.

## Decisions

- A `Shell` owns the renderer and UI resources while no session exists, and hands them to `Play` on connect and takes them back on disconnect. A fresh `RenderSystem` per session avoids stale meshes.
- Connecting runs `Client::connect` on a worker thread; the connecting screen polls a channel.
- Status pings run on worker threads, one per server, with bounded reads and a timeout. Responses are hostile: JSON depth, favicon size and string lengths are capped, and click/hover events in MOTDs are dropped.
- Screens read game state through a `GameView` adapter over `ClientState`; actions are translated in `Play`.
- Container clicks go through `Client::click_container`, which predicts pickup, swap and throw and sends changed slots with the current state id; other modes rely on the server resync.
- The command tree lives in `mcr-client::commands`; local completion covers literals, booleans and enumerated parsers; everything marked ask-server goes to the server.
- Options are shared as `Rc<RefCell<Settings>>` with a version counter that `Play` polls to apply changes.

## Risks

Predicted clicks that differ from the server cause one resync, not a loop; verified live.
