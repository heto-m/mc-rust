# Design

## Loop rules vs wiring

Level maths (elytra ramp, bee speed curve, guardian charge, nearest-N, start/stop reconciliation) is pure code
in `mcr-audio::loops`. `mcr-app::entity_loops` reads the client state each tick and drives voices.

- Elytra: started when the physics flag turns on; 20 silent ticks, a 20 tick fade-in, volume `|v|^2 / 4`,
  pitch rising above volume 0.8; ends once gliding stops after the first 20 ticks.
- Bees: vanilla feeds the clamped speed straight into `lerp`, so volume is `min(speed, 0.5) * 1.2`. Anger is
  `anger_end_time > game_time` from entity data, switching between `entity.bee.loop` and
  `entity.bee.loop_aggressive` voices. Only the 12 nearest bees hold a voice (vanilla has no cap; this guards the
  voice limit). Speed comes from interpolated position deltas, like minecarts.
- Guardian beam: started by entity event 21, global (no attenuation), volume `scale^2`, pitch `0.7 + 0.5 scale`,
  scale = ticks since the attack target changed / 80 (60 for elder); ends when the guardian is removed.
- Creaking heart: vanilla `animateTick` plays `block.creaking_heart.idle` when `creaking_active` holds, the state
  is not uprooted, 1 in 16, and all six neighbours are pale oak logs. `creaking_active` is the day timeline
  keyframes (true from tick 12600 up to 23401 of the 24000 period) in overworld dimension types. Every other
  creaking heart sound is played by the server.

## Credits

Game event 4 becomes `ClientEvent::ShowCredits`; the app builds the screen from `texts/end.txt`,
`credits.json`, `postcredits.txt` in the resource stack. The screen reports `credits_music()`; the ambience
passes `music.credits` as screen music (replaces the current track) and stops it when the screen closes.
Closing (Esc or scroll end) emits `ScreenAction::Respawn`, which sends perform-respawn.

## Now playing

`Audio` records track starts/stops (language key from the picked file path). The app resolves the title,
`ToastManager::set_song` stores it, and with `musicToast = pause_and_toast` the toast slides in from the left for
5 s x notification time. The pause screen draws a fixed copy for any non-`never` setting while music is audible.
