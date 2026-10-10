## ADDED Requirements

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
