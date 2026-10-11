## 1. Audit
- [x] 1.1 Compare every handler in `ClientPacketListener`, `ClientCommonPacketListenerImpl` and the configuration listener with the mcr path; table in `design.md`

## 2. Client state (`mcr-client`)
- [x] 2.1 Title, subtitle and action-bar timing in `Titles` (tick, fades, negative times, restart, clear keeps the action bar)
- [x] 2.2 Boss bars: insertion order, 100 ms progress lerp
- [x] 2.3 Scoreboard: ignore unknown objectives, exclusive teams, number formats, team colour sidebar slots, team-formatted names, sidebar rows; tab order and tab names
- [x] 2.4 `animate` vs `swing_animation` events; explosion sound and debris data; game events 0, 5, 6, 9, 10
- [x] 2.5 Rotation acknowledgement, immediate respawn, mount hint and boat yaw
- [x] 2.6 Trusted server links, custom report details, low disk space toast, `reset_chat`
- [x] 2.7 Transfer as `ClientError::Transferred` with cookies, from play and configuration; login cookie answers
- [x] 2.8 World border in ticks, `warning_strength`, hostile-value guards
- [x] 2.9 Tick freeze (`runs_normally`, frozen entity simulation and pickups) and client tick length
- [x] 2.10 `cooldown_percent` with partial ticks

## 3. App and renderer
- [x] 3.1 HUD: titles from state, ordered boss bars with cutoff, sidebar and tab list rewrite, cooldown overlay
- [x] 3.2 Container slot cooldown overlay through `GameView::cooldown`
- [x] 3.3 Crit and enchanted-hit tracking emitters, off-hand swings, explosion sound and debris, totem sound and held item
- [x] 3.4 Vignette with border warning tint; world border walls in `mcr-render`
- [x] 3.5 Freeze gating of particles, weather and block-entity ticks; tick-rate driven tick length
- [x] 3.6 Shell follows transfers; below-name scores use number formats

## 4. Verification
- [x] 4.1 Unit tests (`audit_tests`, overlays, border quads), mock-server transfer test, GPU border screenshot test
- [x] 4.2 Live session against vanilla 26.3: `/worldborder`, `/title`, `/scoreboard`, `/team`, `/bossbar`, tab list, `/tick freeze`, `/tick rate` with offscreen screenshots
- [x] 4.3 Gate

## 5. Follow-ups (not in this change)
- [ ] 5.1 Leash rope rendering from `set_entity_link` holders (~0.5 day)
- [ ] 5.2 `set_camera`: spectate through another entity, its view and input lock (~1 day)
- [ ] 5.3 `post_effects`: post-processing chains from resource packs (~2-3 days, needs a post pipeline)
- [ ] 5.4 `custom_chat_completions` in chat suggestions (~2 hours)
- [ ] 5.5 Code of conduct screen with accept/decline and per-server memory (~0.5 day)
- [ ] 5.6 `add_transient_block` rendering for landed falling blocks (~0.5 day)
- [ ] 5.7 `projectile_power`: client motion for fireballs and wind charges between position updates (~0.5 day)
- [ ] 5.8 Boss bar `darken_screen` sky darkening and `create_world_fog` fog (~0.5 day)
- [ ] 5.9 Elder guardian ghost overlay for game event 10 (~0.5 day)
- [ ] 5.10 Demo intro popup screen for game event 5 param 0 (~2 hours)
- [ ] 5.11 Tab list hearts render type and player heads in online mode (~0.5 day)
- [ ] 5.12 Locator bar from `waypoint` (~1 day)
- [ ] 5.13 Head yaw 3-tick lerp for `rotate_head` (~1 hour)
- [ ] 5.14 Server list icon update from `server_data` (~1 hour)
- [ ] 5.15 Debug renderers, remote TPS chart and ping chart (`debug/*`, `debug_sample`, `pong_response`) behind F3 subscriptions (~2 days)
- [ ] 5.16 Experimental minecart lerp steps (`move_minecart_along_track`) when the feature flag is on (~0.5 day)
- [ ] 5.17 Insecure-chat toast on login and real key names in server-triggered messages (~2 hours)
