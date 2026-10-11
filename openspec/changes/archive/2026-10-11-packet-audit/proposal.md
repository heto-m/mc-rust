# Proposal

## Why

Milestone 1 is full functionality against a vanilla server. Every clientbound packet now decodes, but
several handlers stopped short of what vanilla's `ClientPacketListener`, `ClientCommonPacketListenerImpl`
and configuration listener do with them: the world border was invisible and lerped twenty times too fast,
explosions were silent, critical hits had no sparks, off-hand swings never animated, titles ignored
vanilla's timing rules, the sidebar and tab list ignored teams and number formats, transfers were never
followed, `/tick freeze` froze nothing on screen. This change audits every handler against vanilla and
fixes the small gaps a player would notice.

## What Changes

- Audit table of every clientbound configuration and play packet (vanilla behaviour, mcr behaviour,
  status) in `design.md`.
- Titles, subtitle and action bar follow vanilla's tick timing; clearing titles keeps the action bar.
- Boss bars stack in arrival order, ease their progress over 100 ms and stop at a third of the screen.
- Scoreboard: unknown objectives are ignored, a player belongs to one team, number formats, team colour
  sidebar slots and team-formatted names; tab list uses vanilla's order, multi-line header/footer and list
  scores.
- Item cooldown overlay in the hotbar and container slots.
- World border lerps in ticks, renders its walls and tints the vignette near the edge (the vignette itself
  is new).
- `/tick freeze` stops entity simulation, particles, weather, block-entity ticking and the border; slow
  tick rates slow the client tick.
- `animate` drives crit and enchanted-hit sparks, `swing_animation` animates either hand.
- Explosions play their sound and spawn block debris particles; totems play `item.totem.use` and show the
  held death-protection item.
- Game events 0, 5, 6, 9 and 10; immediate respawn; dismount hint and boat yaw on mounting; rotation
  packets are acknowledged.
- Transfers close the connection and reconnect with cookies, also from configuration; login cookie
  requests answer from carried cookies.
- `reset_chat`, `custom_report_details`, trusted `server_links`, `low_disk_space_warning`.

Out of scope (follow-ups in `tasks.md`): leash rendering, spectator camera (`set_camera`), post effects,
custom chat completions, code-of-conduct screen, transient blocks, projectile power simulation, debug
renderers and samples, locator bar, boss-bar fog and sky darkening, elder guardian ghost, demo intro popup,
tab list hearts and heads, experimental minecart lerp steps.

## Capabilities

### New Capabilities
- `packet-parity`: client reactions to clientbound packets that the audit brought in line with vanilla.

### Modified Capabilities
None.

## Impact

`mcr-client` (handler, state, social, client, config, events, entity), `mcr-app` (hud, play, shell,
overlays, effects, entities, nametags, view), `mcr-render` (world border pass), `mcr-ui` (cooldown hook).
