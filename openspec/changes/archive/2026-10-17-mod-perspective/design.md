## Context

mcr computes its player camera from its own perspective (first person, third back, third front) and sends it to
Java each frame with `FRAME_DETACHED` and the pull-back distance (`FRAME_CAMERA_DISTANCE`). Until now Java's
camera mixin set `Options.cameraType` to match mcr at every alignment, mcr cycled the perspective on F5 and
`GameplayKeys` drained Java's `keyTogglePerspective`. Mods that change the camera type were overruled:

- Shoulder Surfing 5.2.0 consumes `keyTogglePerspective` clicks in its own handler at the start of
  `Minecraft.tick` (before the bridge drains them) and steps through its perspectives. Its shoulder perspective
  is `THIRD_PERSON_BACK` plus a flag of its own; its `Options.setCameraType` hook turns every other camera-type
  write into a perspective change, so the bridge's forcing write threw it back to first person.
- Freelook 1.4.4 sets the camera type from its config while its key is held and restores it on release.
- Camera Overhaul 2.1.2 injects at `GameRenderer.bobHurt` HEAD and rotates the pose stack about Z by its roll.
  `bobHurt` builds both the level pose (`renderLevel`) and the hand pose; the bridge replaces the level
  projection with mcr's, so only the hand rolled.

## Decisions

### Java's camera type leads

Java publishes `FSTATE_PERSPECTIVE` at the end of each frame: 0 when it does not lead (outside the lockstep or
without a level), else 1 first person, 2 third back, 3 third front, derived from `isFirstPerson` and
`isMirrored`, so a mod perspective layered on a vanilla camera type, or a mod's own enum constant, maps to the
closest vanilla one. While Java draws the world (`world_active`), mcr sets its perspective from it before it
computes the frame's player camera and before the third-person distance goes into `FrameBegin`. A change of
first-person-ness re-checks the spectator post effect, as mcr's own F5 did. mcr's F5 handler does nothing while
Java leads. `GameplayKeys` no longer drains `keyTogglePerspective` while mcr's camera paces Java, so vanilla's
`handleKeybinds` or a mod cycles it. The key itself is a vanilla mapping, so `MSG_KEYS` and `REPORT_KEYS` already
keep it in sync with mcr, and Controlling rebinds it.

Alternatives considered: sending mcr's F5 to Java as a command and keeping mcr's enum authoritative would need a
list of mod perspectives; letting each frame carry Java's camera type into mcr without the frame state would need a
second channel. The frame state already carries every other per-frame Java report.

When Java does not lead (no mods, booting, detached after a stall, Java gone), mcr cycles its own perspective on
F5 exactly as before. After a stall the last followed perspective stays until Java leads again; mcr's own F5
presses in between are lost when Java resumes, because Java's camera type is what it shows.

### The switch frame

Java changes its camera type during a tick, inside the frame whose `FrameBegin` still carries mcr's old
perspective. Rust's values only describe Rust's perspective, so the alignment no longer swaps them in when Java's
camera type and mcr's `FRAME_DETACHED` disagree on first person: vanilla's own alignment (position, mirror,
`getMaxZoom` against Java's level) runs for that frame. The handoff sees a different camera and reports it, mcr
renders that frame's terrain from it and, in the same mcr frame, already follows the new perspective. One frame
later Java's camera equals mcr's again and the handoff ends. A switch between back and front keeps
`FRAME_DETACHED` and shows the new side one frame later, without a handoff.

### Mod roll of the level pose (`FSTATE_VIEW_ROLL`)

A hook placed after `bobHurt` would see Camera Overhaul's roll mixed with vanilla's hurt and death tilt, which mcr
draws itself, and any other mod's change of the bob. Instead the bridge takes the pose `renderLevel` multiplies
into its projection (the argument of the first `Matrix4f.mul`) and rebuilds the pose vanilla alone would make from
the same camera render state and options with the same `PoseStack` calls. The mods' share is
`M = A · V⁻¹`, exact for changes applied before vanilla's (Camera Overhaul injects at `bobHurt` HEAD) and a close
approximation for later ones, since vanilla's pose is a small tilt. Its twist about the view axis,
`2·atan2(q.z, q.w)` of `M`'s rotation, is published when it exceeds 1e-5 rad. View bobbing's translation, and
pitch or yaw a mod adds to the pose, are not carried (mcr has no world view bobbing). A mod that cancels vanilla's
hurt tilt reports the opposite of that tilt, so mcr drops it too.

mcr multiplies the roll in front of its own death and hurt tilt (`Rz(roll) · death · hurt`), the order of Java's
`P · Rmod · hurt · viewRot`. Java's level projection is mcr's `P · V`, so Java's entities roll with the terrain.
The roll is one frame late, like every camera handoff value; Camera Overhaul's roll is smoothed, so the latency
does not show.

### ABI 14

`FSTATE_PERSPECTIVE` (u32) at 116 and `FSTATE_VIEW_ROLL` (f32, radians in mcr's view space) at 120, inside the
existing 128-byte frame state. `BRIDGE_VERSION` 14 (`mod-frame-cost-2` merged without an ABI change).

## Risks

- A mod that sets the camera type every frame to something other than what the user chose would now win over
  mcr's F5; that is the vanilla behaviour such a mod expects.
- Rust's third-person distance is computed along the player's look; a mod that turns the third-person camera
  (Freelook) is pulled back by that distance along its own direction, so close to walls the camera can clip
  slightly differently than in vanilla.

## Tests

- Unit tests: frame-state perspective and view-roll decoding (unknown values, non-finite and out-of-range rolls).
- Test mod: `-Dmcr.testmod.perspective=true` consumes F5 at the start of the tick and cycles first person → third
  front → third back → first person (a cycle mcr never uses); `-Dmcr.testmod.poseRoll=true` rotates the bob pose
  10 degrees about Z at `bobHurt` HEAD.
- Live tests (`entity_scene`): F5 with and without the perspective mod, mcr following into third front and the
  camera looking back at the player; the pose roll reaching mcr's view; the existing third-person, shoulder and
  roll tests unchanged.
- Real jars offscreen against the vanilla server: Shoulder Surfing with its default config, Freelook's held key,
  Camera Overhaul while strafing.
