## Context

Vanilla keeps one `ServerPackManager` per connection: packs are pushed, accepted (by the per-server setting or a confirm screen), downloaded, then all active packs are loaded in one reload. Final statuses are reported after the reload. During configuration the server blocks until each pack reaches a final status, so answers must flow while `Client::connect` is still waiting.

## Decisions

- **Link, not callback.** `ClientConfig` carries an optional `PackLink` (a pair of channels). The client emits `PackRequest::Push/Pop` and drains `PackReply` (status or disconnect) in its receive loops, in configuration and play alike. Without a link the client declines, as before.
- **Manager is pure.** `PackManager` consumes events (push, pop, prompt answer, download result, reload result) and returns actions (send status, start download, reload with these paths). It owns no threads, so the vanilla matrix is unit-tested.
- **Downloads.** One worker thread per batch, sequential like vanilla. HTTP and HTTPS only (redirects stay within http/https), 250 MiB cap on both `Content-Length` and bytes read, connect/read timeouts and an overall deadline. Cache file name is the SHA-1 when a valid hash is given (`downloads/<sha1>`), otherwise the pack id; entries beyond 20 are evicted oldest first. Verified before the file is renamed into place.
- **Hostile archives.** `ZipSource::open_limited` rejects archives over an entry count, total declared size or per-entry size, skips entries with traversal/absolute/backslash names, and caps real reads at the entry limit regardless of declared sizes. Loaders already log and fall back for bad JSON/PNG; PNG dimensions are bounded.
- **Hot reload.** A fresh `Assets` (jar + asset index + active packs in push order) is built, then swapped in on the main thread: texture arrays, tint palette, animations, entity/particle/effect assets, the mesh pool (all loaded sections remeshed in place), GUI (`Ui`, `ItemIcons`, `ScreenAssets`, UI atlas pages) and audio (pack sounds and `sounds.json`). Failure keeps the previous assets and reports `FAILED_RELOAD`.
- **Per-server setting** stored as `acceptTextures` in `servers.dat` (already parsed by `mcr-settings`); the prompt answer persists Enabled/Disabled like vanilla.

## Risks

- Building and swapping assets run on the main thread (about one second with the vanilla jar), so a reload hitches the frame. Lang and sounds.json merge across packs; other files use the topmost pack.
