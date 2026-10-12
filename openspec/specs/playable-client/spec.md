# playable-client Specification

## Purpose
Defines the connected play session: joining a server, fixed tick, input, targeting, HUD, chat input, audio and scripted autopilot.

## Requirements

### Requirement: Connect mode
`mcr --connect host[:port] --name <name>` SHALL join the server in offline mode, load `options.txt` from `--game-dir` (default `run/client`), stream the server's chunks into the renderer and run until closed or disconnected.

#### Scenario: Join and render
- **WHEN** the client joins a vanilla server
- **THEN** chunks received from the server are meshed nearest-first and drawn around the player

#### Scenario: Server view distance
- **WHEN** the server view distance is smaller than the configured render distance
- **THEN** only the columns the server provides are meshed and requested chunks are not assumed to exist

### Requirement: Block updates remesh immediately
A block change SHALL remesh the containing section and, on a section edge, its neighbours, ahead of streaming work.

#### Scenario: Break and place
- **WHEN** the server confirms a block change near the player
- **THEN** the affected sections are submitted to the mesher on the next frame regardless of the streaming queue

### Requirement: Fixed tick with interpolated camera
The client SHALL call `Client::tick` at 20 TPS and render the camera at the interpolation of the previous and current tick positions by the partial tick, at the eye height of the current pose (1.62 standing, 1.27 crouching, 0.4 swimming).

#### Scenario: Frame rate independence
- **WHEN** frames are rendered faster than ticks
- **THEN** the camera moves smoothly between tick positions and no more than 20 ticks per second run

### Requirement: Mouse look and field of view
Mouse motion SHALL turn the view by `delta * (s*0.6+0.2)^3 * 8 * 0.15` degrees with `s` the `mouseSensitivity` option, and the vertical FOV SHALL follow the `fov` option scaled by the smoothed sprint or flight modifier according to `fovEffectScale`.

#### Scenario: Sprint FOV
- **WHEN** the player sprints
- **THEN** the FOV widens over a few ticks and returns when sprinting stops

### Requirement: Key bindings
Input SHALL be resolved through the `KeyBindings` of `options.txt` for movement, jump, sneak, sprint (hold or toggle per options), hotbar 1 to 9, scroll wheel, drop, swap offhand, attack, use, pick block, chat, command, player list, hide GUI, F3 and perspective.

#### Scenario: Rebound key
- **WHEN** `options.txt` binds `key_key.forward` to another key
- **THEN** that key moves the player forward and `W` does not

### Requirement: Targeting
The client SHALL cast a ray from the eye along the view up to the interaction range (4.5 blocks, 5 in creative) against the outline shapes of loaded blocks and the bounding boxes of entities, and select the nearest hit.

#### Scenario: Outline
- **WHEN** a block is targeted
- **THEN** its outline shape is drawn in black over the world with depth testing

### Requirement: Digging, placing and using
Holding attack on a block SHALL send start, and after the estimated break time finish, destroy actions; releasing or retargeting SHALL abort. Use SHALL send use-item-on for a targeted block, interact for a targeted entity and use-item otherwise. Pick block SHALL copy the targeted block into the hotbar in creative.

#### Scenario: Creative break
- **WHEN** attack is pressed on a block in creative mode
- **THEN** one start-destroy action is sent and the block is removed when the server confirms

### Requirement: HUD
The HUD SHALL draw the hotbar with items, health, armor, food, air, experience, crosshair, chat, titles, action bar, boss bars and the scoreboard sidebar from client state, the player list while the player-list key is held, and the debug overlay with F3. F1 SHALL hide the HUD.

#### Scenario: Chat from the server
- **WHEN** a chat message arrives
- **THEN** it is shown in the chat HUD as a component, and log output of the same text is sanitised

### Requirement: Chat input
Pressing the chat key SHALL open a text line, Enter SHALL send it as chat (or as a command when it starts with `/`) and close it, and Escape SHALL close it without sending.

#### Scenario: Send a command
- **WHEN** the player types `/time set day` and presses Enter
- **THEN** a chat command without the slash is sent

### Requirement: Audio
Server sound packets and local block break, place and step events SHALL be played through the audio mixer with the listener at the camera and category volumes from the options.

#### Scenario: Missing audio device
- **WHEN** no audio device is available
- **THEN** the client runs silently

### Requirement: Autopilot and screenshots
`--autopilot <script>` SHALL drive the client with scripted inputs through the same input path as the keyboard, and `--screenshot out.png --after-ticks N` SHALL render offscreen after N ticks and write a PNG.

#### Scenario: Scripted capture
- **WHEN** a script contains `shot name.png`
- **THEN** the frame at that point is written to that file

### Requirement: View at pitch limits
The client SHALL clamp pitch to [-90, 90] degrees and SHALL render a consistent, unflipped view at the limits.

#### Scenario: Looking straight down
- **WHEN** the pitch is 90 degrees down at any yaw
- **THEN** screen-up points along the facing direction and screen-right is unchanged from the level view

### Requirement: Client-side light
The client SHALL recompute sky and block light locally when a block changes and SHALL remesh every section whose light or geometry changed, including neighbours; entities SHALL be lit from the light at their eye position.

#### Scenario: Broken block
- **WHEN** a block is broken and the opening exposes the area to sky or a light source
- **THEN** light in the opening updates immediately without waiting for the server

#### Scenario: Roof removed
- **WHEN** the only block roofing a column is removed
- **THEN** every cell in that column reaches full sky light and neighbouring cells fall off by one per block

### Requirement: Local break particles
The client SHALL spawn destroy particles when the local player's own break removes a block, since the server omits the breaker from the broadcast.

#### Scenario: Own break
- **WHEN** the local player breaks a block
- **THEN** destroy particles appear at the block

### Requirement: Entity picking
The client SHALL target only pickable entities (not items, experience orbs, arrows, area effect clouds, markers, display entities or spectators), SHALL widen redirectable projectiles by one block, and SHALL choose an entity only when strictly nearer than the block hit.

#### Scenario: Dropped item
- **WHEN** the crosshair is over a dropped item
- **THEN** the item is not targeted and use actions go to the block behind it

### Requirement: Click consumption and block placement
The client SHALL act on every use and attack click even when the key is released before the next tick, and SHALL predict block placement locally.

#### Scenario: Quick click
- **WHEN** the use key is pressed and released within one tick while looking at a block with a block item held
- **THEN** the block is placed and shown at once

#### Scenario: Replaceable target
- **WHEN** a block is used against tall grass
- **THEN** the grass is replaced by the held block

### Requirement: Idle stability
A stationary player on leaves or in still water SHALL not drift horizontally, and the client SHALL send position packets only at the vanilla cadence.

#### Scenario: Standing on leaves
- **WHEN** the player stands with no input for 30 seconds on leaves
- **THEN** the position is unchanged and no server correction occurs

### Requirement: Locator bar
When waypoints are tracked, the HUD SHALL draw the locator bar in place of the experience bar, except for 100 ticks after the experience progress changes. Each waypoint within 60 degrees of the view SHALL draw as a style sprite chosen by distance, tinted by its colour or a hash of its id, with an arrow when it lies above or below the view.

#### Scenario: Ahead
- **WHEN** a waypoint is straight ahead
- **THEN** its dot is at the middle of the bar

#### Scenario: Behind
- **WHEN** a waypoint is behind the player
- **THEN** it is not drawn

### Requirement: Tab list heads
On an encrypted connection the tab list SHALL draw each player's 8x8 head, with the hat layer when worn, before the name. Skins SHALL come from the `textures` property of the profile when it points at `textures.minecraft.net`, else from the default skin chosen by UUID.

#### Scenario: Foreign host
- **WHEN** the textures property names another host
- **THEN** nothing is downloaded and the default skin is used

### Requirement: Boss event world effects
Boss events with the darken flag SHALL darken the world lightmap, ramping up 0.05 and down 0.0125 per tick; those with the fog flag SHALL limit fog start to 10 and end to 96 blocks.

#### Scenario: Darken
- **WHEN** a darkening boss bar is present for 20 ticks
- **THEN** the darkening factor is 1

### Requirement: Debug charts
The debug overlay SHALL draw rolling frame time, tick time and ping charts of the latest 240 samples.

#### Scenario: Bounded history
- **WHEN** more than 240 samples were recorded
- **THEN** only the newest 240 bars are drawn
