# Design

## Context

Every clientbound configuration and play packet decodes byte-exact (`mcr-protocol` dispatch). This change
compares what vanilla 26.3 does with each one (`ClientPacketListener`, `ClientCommonPacketListenerImpl`,
`ClientConfigurationPacketListenerImpl`) against the path through mcr:
`mcr-protocol` dispatch → `mcr-client` (`client.rs` for configuration, `handler.rs` for play) →
`ClientEvent`s and `ClientState` → `mcr-app` (`play.rs`, `hud.rs`, `effects.rs`, `entities.rs`,
`overlays.rs`, `shell.rs`) and `mcr-render`.

Status after this change:

- **complete**: the player sees and the server receives what vanilla would.
- **partial**: the state is right but some client-side presentation is missing (follow-up listed).
- **missing**: nothing happens yet (follow-up listed).
- **ignored-by-design**: vanilla only uses it for debug tooling mcr does not have, or it is a no-op.

"Fixed here" marks rows this change moved.

## Audit: configuration

| Packet | Vanilla client | mcr | Status |
|---|---|---|---|
| `cookie_request` | Reply with the stored cookie | Reply from `server.cookies` (now seeded from a transfer) | complete |
| `custom_payload` | Store `minecraft:brand`, warn on others | Brand stored, others dropped | complete |
| `disconnect` | Disconnect with reason | Closes, `Disconnected` event | complete |
| `finish_configuration` | Build registries, ack, switch to play | Ack, phase play | complete |
| `keep_alive` / `ping` | Reply | Reply | complete |
| `reset_chat` | Forget the chat kept across reconfiguration | Was ignored; clears history, `ChatCleared` clears the HUD (fixed here) | complete |
| `registry_data` / `update_tags` | Collect registries and tags | Same | complete |
| `update_enabled_features` | Store feature flags | Stored (bounded) | complete |
| `select_known_packs` | Answer the packs it knows | Answers `minecraft:core 26.3` | complete |
| `resource_pack_push` / `_pop` | Prompt or apply per server setting; pop one or all | Pack manager in the app (prompt, download, apply, pop) | complete |
| `post_effects` | Set the player's post-processing chains | Ignored | missing |
| `store_cookie` | Store | Stored (64 cookies, 5 KiB each) | complete |
| `transfer` | Disconnect, reconnect to the target with cookies | Was an event the app ignored; now closes and the shell reconnects with cookies (fixed here) | complete |
| `custom_report_details` | Keep for crash reports | Was ignored; stored, 32 entries (fixed here) | complete |
| `server_links` | Keep only `http`/`https` URIs | Was unvalidated; filtered (fixed here) | complete |
| `clear_dialog` / `show_dialog` | Dialog screens | Dialog link to the app | complete |
| `code_of_conduct` | Show the code, accept or disconnect, remember per server | Auto-accepts without showing it | partial |

## Audit: play

| Packet | Vanilla client | mcr | Status |
|---|---|---|---|
| `bundle_delimiter` | Apply bundled packets together | Packets applied in order, delimiter dropped | complete |
| `add_entity` | Spawn, minecart/bee loops | Entity added, loops in `entity_loops` | complete |
| `animate` | 0 wake up, 1 crit sparks, 2 enchanted-hit sparks | Was mapped to a swing; now crit/enchanted-hit tracking emitters, wake-up via pose data (fixed here) | complete |
| `award_stats` | Update stats, refresh screen | Same | complete |
| `block_changed_ack` | Settle predictions | Same | complete |
| `block_destruction` | Crack overlay | Decals | complete |
| `block_entity_data` | Load block entity | Stored, renderers refresh | complete |
| `block_event` | Chest lids, bells, pistons | Block entity animations | complete |
| `block_update` / `section_blocks_update` | Server-verified set | Same, prediction-aware | complete |
| `boss_event` | Insertion-ordered bars, 100 ms progress lerp, stop at a third of the screen, darken/fog/music flags | Was UUID-ordered, no lerp, max 4; order, lerp and cutoff fixed here; music flag used; darken and fog flags not | partial |
| `change_difficulty` | Store | Stored | complete |
| `chunk_batch_start` / `_finished` | Batch size feedback | Same | complete |
| `chunks_biomes` | Replace biomes, re-mesh | Same | complete |
| `clear_titles` | Clear title/subtitle (not the action bar), optionally reset times | Also cleared the action bar; fixed here | complete |
| `command_suggestions` / `commands` | Suggestions, command tree | Same | complete |
| `container_*`, `set_cursor_item`, `set_player_inventory`, `open_screen`, `mount_screen_open`, `merchant_offers`, `place_ghost_recipe` | Menu state and screens | Same | complete |
| `cookie_request` / `store_cookie` | As configuration | Same; cookies now survive a transfer | complete |
| `cooldown` | Track cooldown, white overlay over the item | Tracked; overlay was never drawn; hotbar and container overlay with partial ticks (fixed here) | complete |
| `custom_chat_completions` | Add/remove/set extra chat completions | Ignored | missing |
| `custom_payload` | Brand | Brand | complete |
| `damage_event` / `hurt_animation` | Hurt flash, sound, camera tilt toward the source | Same | complete |
| `debug/block_value`, `debug/chunk_value`, `debug/entity_value`, `debug/event` | Debug renderers (subscription only) | Ignored | ignored-by-design |
| `debug_sample` | Remote TPS chart (subscription only) | Ignored; never subscribed | ignored-by-design |
| `delete_chat` | Remove a signed message | Removed from history | complete |
| `disconnect` | Disconnect | Same | complete |
| `disguised_chat` / `player_chat` / `system_chat` | Decorated chat, overlay to the action bar | Same (no signature checks in offline mode) | complete |
| `entity_event` | Per-entity effects; totem: particles, `item.totem.use`, item activation of the held death-protection item | Effects in app; totem sound and held item were missing (fixed here) | complete |
| `entity_position_sync`, `move_entity_*`, `teleport_entity`, `set_entity_motion`, `rotate_head` | Interpolate; head lerps over 3 ticks | Interpolates; head yaw snaps | partial |
| `explode` | Sound (unless silent), particle, block debris (`ClientExplosionTracker`), knockback | Particle and knockback only; sound and debris added (fixed here) | complete |
| `add_transient_block` | Briefly render a block where a falling block landed | Ignored | missing |
| `forget_level_chunk` | Drop chunk and light | Same | complete |
| `game_event` | 0 spawn message, 1/2/7/8 weather, 3 game mode, 4 credits, 5 demo, 6 arrow hit, 9 pufferfish sting, 10 elder guardian, 11 immediate respawn, 12 limited crafting, 13 load start | 0, 5 (messages; intro as chat), 6, 9, 10 sound added here; elder guardian ghost particle and demo intro popup missing | partial |
| `game_rule_values` | Feed the in-world game rules screen | Stored | complete |
| `game_test_highlight_pos` | Game test debug renderer | Ignored | ignored-by-design |
| `initialize_border`, `set_border_center`, `set_border_lerp_size`, `set_border_size`, `set_border_warning_delay`, `set_border_warning_distance` | Border lerps over game ticks; walls rendered near it; red vignette inside the warning distance | Lerped in milliseconds (20x too fast), no walls, no vignette; all fixed here | complete |
| `keep_alive` / `ping` | Reply | Reply | complete |
| `level_chunk_with_light` / `light_update` | Chunk and light | Same | complete |
| `level_event` / `level_particles` / `sound` / `sound_entity` / `stop_sound` | Effects and sounds | Same | complete |
| `login` / `respawn` | New level, player reset, insecure-chat toast | Same, without the insecure-chat toast | complete |
| `low_disk_space_warning` | System toast | Was ignored; toast (fixed here) | complete |
| `map_item_data` | Map texture | Same | complete |
| `move_minecart_along_track` | Lerp steps for experimental minecarts | Ignored (feature-flagged) | missing |
| `move_vehicle` | Snap controlled vehicle, echo position | Same | complete |
| `open_book` / `open_sign_editor` | Book / sign screens | Same | complete |
| `pong_response` | Ping debug chart | Ignored | ignored-by-design |
| `player_abilities` / `set_health` / `set_experience` / `set_held_slot` | Player state | Same | complete |
| `player_combat_end` / `_enter` | No-op | No-op | complete |
| `player_combat_kill` | Death screen, or respawn at once when the death screen is off | Death screen; immediate respawn added (fixed here) | complete |
| `player_info_update` / `_remove` | Player list | Same | complete |
| `player_look_at` | Turn toward target | Same | complete |
| `player_position` | Teleport, accept | Same | complete |
| `player_rotation` | Rotate and send `move_player_rot` | Rotated without the reply; reply added (fixed here) | complete |
| `post_effects` | Post-processing chains | Ignored | missing |
| `recipe_book_*`, `update_recipes`, `update_advancements`, `select_advancements_tab` | Books, toasts | Same | complete |
| `remove_entities`, `remove_mob_effect`, `update_mob_effect`, `update_attributes`, `set_entity_data`, `set_equipment` | Entity state | Same | complete |
| `reset_score`, `set_score`, `set_objective`, `set_display_objective` | Ignore unknown objectives; number formats; team colour slots | Stored scores for unknown objectives, ignored formats and team slots; fixed here | complete |
| `set_player_team` | One team per player; prefix, suffix, colour on names | Players could sit in several teams; fixed here, names formatted in sidebar and tab list | complete |
| `resource_pack_push` / `_pop` | As configuration | Same | complete |
| `server_data` | Update MOTD and icon in the server list | MOTD only | partial |
| `set_action_bar_text`, `set_title_text`, `set_subtitle_text`, `set_titles_animation` | Tick-driven fades; subtitle shows only with a title; negative times ignored; changing times restarts a shown title | Subtitle alone showed; stale subtitles resurfaced; negative times stored; fixed here | complete |
| `set_camera` | Spectate through another entity | Ignored | missing |
| `set_chunk_cache_center` / `_radius`, `set_simulation_distance` | View area | Same | complete |
| `set_default_spawn_position` / `set_time` | Spawn, clocks | Same | complete |
| `set_entity_link` | Leash holder; rope rendered | Holder stored, no rope | partial |
| `set_passengers` | Mount; "press shift to dismount" overlay; boats align the rider's yaw | Mount only; overlay and yaw added (fixed here) | complete |
| `start_configuration` | Ack, keep chat | Same | complete |
| `swing_animation` | Swing either hand | Off-hand swings were dropped; fixed here | complete |
| `tab_list` | Header/footer, hidden when empty, wrapped lines | Single line; now wrapped, multi-line (fixed here) | complete |
| `tag_query` | F3+I query answers | Ignored | ignored-by-design |
| `take_item_entity` | Pickup animation and sound | Same | complete |
| `test_instance_block_status` | Feed the test instance screen | Same | complete |
| `ticking_state` / `ticking_step` | Frozen worlds stop non-player entities, particles, weather, border, time; slow tick rates slow the client | Only game time stopped; all fixed here | complete |
| `transfer` | As configuration | Fixed here | complete |
| `update_tags` | Re-tag registries | Same | complete |
| `projectile_power` | Fireball acceleration for client-side motion | Ignored; hurting projectiles only interpolate | missing |
| `custom_report_details` / `server_links` | As configuration | Fixed here | complete |
| `waypoint` | Locator bar | Tracked, no locator bar | partial |
| `clear_dialog` / `show_dialog` | Dialog screens | Same | complete |

Summary (rows; grouped packets count once): configuration 17 rows (15 complete, 1 partial, 1 missing);
play 78 rows (61 complete, 6 partial, 6 missing, 5 ignored-by-design). 23 rows were fixed here.

## Decisions

- **Title and action-bar timing live in `mcr-client`** (`Titles::tick`, `title_alpha`, `overlay_alpha`)
  instead of the HUD, so the rules are unit-tested without a GPU and the HUD only reads them.
- **Scoreboard keeps a member → team map** so membership is exclusive and `team_of` is O(1); total
  memberships stay bounded.
- **Number formats** are applied by `format_score`; a styled format reuses the component NBT parser by
  inserting the score as `text` into the style compound.
- **Transfers are an error** (`ClientError::Transferred`) carrying host, port and cookies; the client
  closes immediately like vanilla, the shell reconnects with `ClientConfig::for_transfer`, and the login
  phase answers cookie requests from the carried cookies.
- **World border walls** are a new effects slot in `mcr-render` reusing the quad vertex path, the existing
  additive `overlay` blend (vanilla's `OVERLAY` blend) and a repeat sampler over `misc/forcefield.png`;
  four camera-relative quads are rebuilt per frame (trivial cost).
- **Vignette** uses `misc/vignette.png` decoded to an alpha mask so the UI's premultiplied blend gives
  vanilla's darkening exactly; the border warning approximates vanilla's per-channel multiply with a red
  term, since the UI pipeline has no multiply blend.
- **Tick freeze** follows vanilla's `runGameElements`: computed each tick, gating border, clocks, entity
  simulation (players and mounts keep ticking), pickups, particles, weather and block-entity ticks; the
  client tick length is `max(50 ms, 1000 / tick rate)` while running.
- **Default key names** stand in for the player's bindings in server-triggered messages (`mount.onboard`,
  demo help); the client crate does not know the bindings.

## Risks

- The red border vignette is an approximation of a multiply blend; colours differ slightly from vanilla.
- Border walls draw within the client render distance; vanilla uses the same distance but its own far
  plane height, which only matters when flying far above the wall.
