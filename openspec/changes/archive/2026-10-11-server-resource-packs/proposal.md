## Why

Servers push resource packs (`resource_pack_push` / `resource_pack_pop`). The client currently answers every push with a canned status and never loads the pack, so servers that require packs cannot be played and custom textures, sounds and language never show.

## What Changes

- New crate `mcr-respack`: pack manager state machine following vanilla's `ServerPackManager`, a bounded HTTP(S) downloader (ureq, rustls) with SHA-1 verification, a hash-keyed cache under `downloads/` with eviction, and the vanilla status sequence.
- `mcr-client` hands pack pushes/pops to the app through a channel link and forwards the app's statuses to the server, in both configuration and play (the server waits for a final status during configuration).
- `mcr-assets`: size-bounded, traversal-safe `ZipSource` for untrusted archives.
- `mcr-app`: per-server "Server Resource Packs: Prompt/Enabled/Disabled" (persisted in `servers.dat`), vanilla confirm screen, required-and-declined disconnect, and a hot reload of models, textures, mesh, sounds, language, fonts, item models and GUI atlases.

## Out of scope

- Local resource-pack selection screen, `pushLocalPack`, proxies, download progress toasts (progress is logged), and pack-provided shaders/post effects.

## Impact

- Affected specs: new `server-resource-packs`; touches `client-connection` behaviour (packs no longer auto-answered when a link is present) and `server-list` (new per-entry setting).
