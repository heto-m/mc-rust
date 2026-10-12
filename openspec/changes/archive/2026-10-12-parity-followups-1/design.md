## Context

Rows of the packet-audit table that were `partial` or `missing`, plus the render-polish-1 follow-ups.

## Outcome per audit row

| Row | Outcome |
|---|---|
| `code_of_conduct` | Fixed: `ConductLink` like the pack link; unaccepted text opens `ConductScreen`; accept or decline answers on the link; the Java `String.hashCode` of the text is stored when remembered, and a matching hash accepts silently. Tests: mock server (wait, silent, decline), screen actions. |
| `boss_event` darken/fog | Fixed: `GameRenderer` ramp (+0.05 / -0.0125 per tick) feeds `Environment.boss_darkening`; fog flag pulls fog to 10..96 blocks. Tests on both helpers. |
| `rotate_head` | Fixed: head turns over 3 ticks (`lerpHeadTo`). Test. |
| `server_data` | Fixed: a valid 64x64 PNG icon is written into the server's `servers.dat` entry. Test. |
| `set_camera` | Fixed: `camera_entity` state, movement keys zeroed, eye and view of the entity, entity hidden, local player shown, sneak leaves. Test on state. |
| `set_entity_link` | Fixed: 24 camera-facing segments in the lead colours between the leashed mob and its holder. Test renders pixels. |
| `waypoint` | Fixed: locator bar geometry in `mcr-client::locator` (yaw, pitch arrows, hash colour, style sprite by distance), drawn in place of the experience bar for 100 ticks after an XP change. Update semantics follow vanilla (data only, same variant). Tests. |
| `custom_chat_completions` | Fixed: bounded list, plain-chat completion of the last word. Test. |
| `add_transient_block` | Fixed: drawn as a falling-block view for 20 ticks. Test. |
| `game_event` 10 | Fixed: the ghost fades across the screen for 30 ticks. The demo intro popup stays deferred (no demo mode). |
| tab-list heads | Fixed: `mcr-auth::textures` accepts only `textures.minecraft.net` URLs over https, no redirects, 256 KiB; `SkinStore` bounds requests, evicts departed players, survives worker panics; heads shown only when the connection is online, as vanilla's `isEncrypted`. |
| debug charts | Fixed locally: frame, tick and ping charts in the F3 overlay. `debug_sample`/`pong_response` stay ignored-by-design. |
| `post_effects` | Deferred: needs the world rendered into an intermediate target between the 3D passes and the UI and WGSL ports of the creeper, spider and invert chains. That touches the renderer's present path, which differs for the surface, offscreen and mod-split modes. Should be its own change. |
| `projectile_power` | Deferred: hurting projectiles interpolate between server updates; the client-side acceleration only matters for sub-three-tick gaps. |
| `move_minecart_along_track` | Deferred: feature-flagged experimental minecarts. |
| `game_event` demo popup | Deferred: no demo mode. |

## Render follow-ups

- **Trident glint**: `ThrownTrident.ID_FOIL` (data 12) adds an additive `enchanted_glint_item` layer on the same model.
- **Cushion**: `Cushion` data 8 (dye colour) selects `entity/cushion/<colour>_cushion`; pose is `Ry(180 - facing) * Rx(180)`, lowered 0.25.
- **Ominous item spawner**: item cluster scaled over the first 50 ticks, spinning 40 degrees per tick, fully lit.
- **Display billboards**: orientation follows `DisplayRenderer.calculateOrientation` (fixed, vertical, horizontal, center) using the camera angles now in `PlanCx`; degenerate quaternions fall back to identity.
- **Ender dragon lean**: 64-sample flight history kept for dragons only; yaw from sample 7, pitch `(y5 - y10) * 10`.
- **End crystal base**: checked. Vanilla scales by 2, translates -0.5 (scaled), model unflipped; here `extra_offset` -1 with a net `(2,2,2)` scale gives the base at -1..-0.5, matching. No change.
- **Breeze wind charge render type**: deferred, the scrolled translucent pipeline is visually close.

## Decisions

- **Conduct link**: a channel pair, same shape as packs, because the configuration handshake runs on the connect thread and must wait for the UI without blocking keep-alives.
- **Skin store**: failures remembered by 64-bit hash in a capped set; at most 4 downloads in flight; results always reported so the in-flight count cannot leak.
- **Locator projection**: vanilla's NDC arithmetic done on the camera basis, so no projection matrix is needed in the client crate.

## Risks

- Leash ropes are camera-facing ribbons, not vanilla's flat strip; thickness reads the same at normal distance.
- Tab heads cannot be seen on offline servers by design.
