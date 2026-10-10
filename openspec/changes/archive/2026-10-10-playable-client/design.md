## Context

`Game`/`RenderSystem` stream an Anvil save through a `WorldSource`. The network client already owns a dirty-tracking `ChunkStore` (`client.state.chunks`) with edge-aware block dirtying, so copying columns through a source would duplicate memory and block-update logic.

## Decisions

- `RenderSystem` meshes any `MeshWorld` (store, camera, render distance, pending predicate, unloaded columns, biome names). `Game` implements it for Anvil; `NetworkWorld` (owning the `Client`) implements it directly on `client.state.chunks`. Chunks in server range but not yet received count as pending so meshing waits for neighbours.
- Block changes seen as `ClientEvent::BlockChanged` mark their sections priority; priority sections bypass the streaming order, neighbour wait and the pool cap.
- `Session` owns input state, tick accumulator, interpolation and UI; the window loop and the offscreen autopilot runner both drive it through the same key, mouse, scroll and text entry points.
- Overlay: `Renderer::render_with_overlay` hands the frame's encoder, colour and depth views to an `Overlay` before submit (depth is stored instead of discarded). The app's overlay draws the block outline (CPU-built screen-space quads, depth tested) and the `mcr-ui` batch.
- Dig timing uses `Client::estimate_dig_ticks` (hardness-based, tool-agnostic, so never faster than the server accepts for correct tools).
- Audio failures degrade to silence. Block sounds use `BlockInfo::sound_type` as `block.<type>.<break|place|step|hit>`.
- Chat input is a bare line editor, replaceable by the chat screen.

## Risks

- Entity rendering and the player model are absent, so third-person shows no body.
- Tool-aware break speed is not modelled; survival digging with tools is slower than vanilla.
