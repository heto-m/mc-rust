## Context

The second sweep ran popular Fabric client mods for 26.3 that the first sweep did not cover, downloaded from
Modrinth (game version 26.3, loader fabric) into a worktree-private cache that is never committed. Each mod booted
alone to Java's title screen with its libraries, then joined a vanilla 26.3 server (ports 25568, later 25567) with an autopilot
script (HUD, inventory hovers, F3, third person, chat, movement), then all mods from both sweeps booted and joined
together. Results are in `docs/mod-compat.md`.

## Findings that led to fixes

- **MiniHUD and Item Scroller hotkeys did nothing.** malilib reads keys in vanilla's SDL event handler, which
  mcr's input never passed through. Key and button events now also go through `SDLEventHandler`'s key and button
  methods with vanilla's body skipped, and an event a mod cancels there stops before Java's keyboard handler.
- **InvMove could not walk** with the inventory open (movement in screens, below).
- **Presence Footsteps doubled the player's steps** with mcr's (footsteps, below).
- **Freelook turned the player.** It cancels `Entity.turn` and turns its own camera inside `alignWithEntity`,
  which the bridge skipped. Vanilla's alignment now runs with Rust's values swapped in, and a probe after mods'
  `turn` hooks reports when the turn was consumed (`FSTATE_TURN`), so mcr stops turning its player. In the sweep's
  first run the single autopilot motion after joining was dropped by vanilla's `ignoreFirstMove`, as in vanilla;
  two motions turn the camera.
- **Shoulder Surfing showed the plain third-person camera.** It redirects the pull-back `move` and offsets from the
  current position, which was Rust's finished camera (and `move` was zeroed). Java now starts from Rust's eye and
  pulls back by Rust's distance (`FRAME_CAMERA_DISTANCE`). Its own perspective is only reached through F5 with
  `replace_default_perspective = true`, because mcr cycles perspectives itself and drains the vanilla key.
- **Camera rolls.** The handoff carried only yaw and pitch, so a rolled camera rotation was lost; it now carries
  the roll (`FSTATE_CAMERA_ROLL`). Camera Overhaul turned out to roll the level pose in `bobHurt` instead, which
  mcr replaces with its own view, so its roll still only shows on the hand.
- **The culled block-entity section** of the first sweep is legitimate: the report now names the block entity, and
  in natural terrain it is dungeon spawners and mineshaft chests deep underground. No false culling was found in
  the reached-sections path; a test pins that the camera's section and its face neighbours are always reached.

## Decisions

The ABI additions of this change were first numbered 11; they landed on top of `mod-frame-pipeline` (12), so the
bridge is version 13. `FSTATE_CAMERA_ROLL` sits at 112, after `FSTATE_PROF`.

- **Movement in screens (ABI 13).** InvMove sets the movement mappings from the keyboard while a screen is open,
  right before `LocalPlayer.aiStep` ticks the input. The bridge already wipes Java's movement input after that
  tick, because mcr moves the player. Just before the wipe it now records the `Input` as `MOVE_*` bits, only while
  a screen is open, and publishes them in the input block (`ISTATE_SCREEN_MOVES`, the former padding at 92).
  While a Java screen has input authority and no mcr screen is open, mcr's player input takes these bits, so the
  player walks with the inventory open exactly when a mod allows it. Without such a mod the bits stay 0.
- **Footsteps a mod took over (ABI 13).** Presence Footsteps cancels `Entity.walkingStepSound` and plays its own
  sounds from the entity's motion, including for the local player, while mcr plays vanilla footsteps for the local
  player natively. Once a second Java calls `walkingStepSound` on its shadow player; the bridge's
  `Player.playStepSound` hook notes that the call arrived and cancels it, so nothing plays. If it never arrived, a
  mod replaced the footsteps: Java sets `FSTATE_STEPS` and mcr skips its own footstep and climbing cues (swimming,
  splashes and landings stay). This needs no list of mods, the same approach as the weather replacement.
- **Third-person eye.** Java derives the eye as Rust's camera plus its distance along the view axis, in doubles
  (Minecraft's table-based `Mth` trigonometry would put the result outside the handoff's 1e-4 tolerance), so
  without a mod the alignment reproduces Rust's camera and nothing is handed off.
- **Autopilot drags.** `down x y [right]` and `up x y [right]` press and release a button at GUI coordinates;
  `hover` between them moves the cursor with the button held, which is what drag handlers see. `wheel <lines>`
  scrolls. In a Java screen they go through the same input routing as real mouse events.

## Not fixed

- Camera Overhaul's roll (a pose-stack roll inside `bobHurt`, see above). Carrying it would need Java to separate a
  mod's share of the bob pose from vanilla's hurt tilt and view bobbing, which mcr draws itself.
- Shoulder Surfing's perspective cycling and Freelook's third-person view: mcr owns the perspective, so a mod that
  changes Java's camera type is overruled. Following Java's perspective would change how F5 works for everyone.
- Riding: mcr places the local passenger 0.4 blocks below the vehicle's position for every vehicle, which puts the
  eye inside a horse's head (seen with Better Mount HUD). This is native riding code, not the bridge.
