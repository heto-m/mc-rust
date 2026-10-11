## ADDED Requirements

### Requirement: Title timing
The client SHALL show a title for fade-in plus stay plus fade-out ticks after a `set_title_text`, fading in and out like vanilla, and clear both title and subtitle when that time runs out. A subtitle alone SHALL NOT show. `set_titles_animation` SHALL ignore negative values and restart the timer of a title on screen. `clear_titles` SHALL keep the action bar, and with `reset_times` restore 10, 70 and 20 ticks.

#### Scenario: Subtitle without title
- **WHEN** only `set_subtitle_text` arrives
- **THEN** nothing is drawn until a title arrives

#### Scenario: Expired subtitle
- **WHEN** a title expires and a new title arrives without a subtitle
- **THEN** the old subtitle is not shown

### Requirement: Boss bar overlay
Boss bars SHALL stack in the order they were first added, re-adding an id SHALL keep its position, progress changes SHALL ease over 100 ms, and stacking SHALL stop once the next bar would start at a third of the screen height.

#### Scenario: Arrival order
- **WHEN** bars with ids 9, 1 and 5 are added in that order
- **THEN** they are drawn top to bottom as 9, 1, 5

### Requirement: Scoreboard state
Scores and objective changes for objectives the client never saw SHALL be ignored, a display slot naming an unknown objective SHALL be cleared, and a player SHALL belong to at most one team.

#### Scenario: Moving teams
- **WHEN** a player in team red joins team blue
- **THEN** the player is only in team blue

### Requirement: Score number formats
Score text SHALL use the score's number format, else the objective's, else red in the sidebar, yellow in the tab list and plain below names.

#### Scenario: Fixed format
- **WHEN** a score has a fixed number format
- **THEN** the fixed text is drawn instead of the number

### Requirement: Sidebar parity
The sidebar SHALL prefer the slot of the local player's team colour, sort by score descending then owner name ignoring case, hide `#` owners, show at most 15 rows, and format names with the owner's team prefix, suffix and colour.

#### Scenario: Team colour slot
- **WHEN** the local player is on a red team and objectives are shown in both the sidebar and the red team slot
- **THEN** the red team slot's objective is drawn

### Requirement: Tab list parity
The tab list SHALL order listed players by list order descending, spectators last, team name, then profile name ignoring case, at most 80; show spectators in italics at reduced opacity; draw list-slot scores; and wrap the header and footer to the screen width minus 50, hiding them when empty.

#### Scenario: Ordering
- **WHEN** a player has list order 5 and a spectator has list order 0
- **THEN** the list-order 5 player is first and the spectator last

### Requirement: Item cooldown overlay
Items covered by a cooldown SHALL show vanilla's white overlay in the hotbar, off-hand slot and container slots, shrinking with partial ticks.

#### Scenario: Ender pearl
- **WHEN** a 20-tick cooldown starts for ender pearls
- **THEN** the overlay covers the whole icon and is gone after 20 ticks

### Requirement: World border presentation
Border size changes SHALL lerp over the given number of game ticks. When the camera is within render distance of a wall the client SHALL draw the scrolling force field, blue when stationary, red while shrinking and green while growing, with opacity `(1 - distance / render distance)^4`. While the vignette option is on the client SHALL draw vanilla's light-dependent vignette and tint it red inside the border warning distance.

#### Scenario: Lerp in ticks
- **WHEN** the border lerps from 100 to 50 over 200 ticks
- **THEN** it is 75 wide after 100 ticks

### Requirement: Tick freeze and rate
While the server's tick manager is frozen and not stepping, the client SHALL stop non-player entity simulation (except mounts), item pickup animations, particles, weather effects, block-entity ticks, game time and the border. A tick rate under 20 SHALL lengthen the client tick to `1000 / rate` ms; faster rates SHALL keep 50 ms.

#### Scenario: Step while frozen
- **WHEN** the world is frozen and `ticking_step` asks for 2 ticks
- **THEN** game time advances by exactly 2

### Requirement: Animation and effect packets
`animate` action 1 and 2 SHALL emit crit and enchanted-hit particles around the entity for 3 ticks; `swing_animation` SHALL swing either hand. Explosions SHALL play their sound at volume 4 unless silent and spawn up to 512 block debris particles per tick shared by block count. Totem activations SHALL play `item.totem.use` and show the death-protection item held by the player.

#### Scenario: Off-hand swing
- **WHEN** `swing_animation` names the off hand
- **THEN** the entity swings its off hand

### Requirement: Game event reactions
Game event 0 SHALL post `block.minecraft.spawn.not_valid`; 5 SHALL post the demo help messages; 6, 9 and 10 (param 1) SHALL play the arrow hit, pufferfish sting and elder guardian curse sounds at the player; with the death screen disabled (event 11) a combat kill of the local player SHALL request a respawn at once.

#### Scenario: Immediate respawn
- **WHEN** event 11 turned the death screen off and the local player is killed
- **THEN** a perform-respawn command is sent

### Requirement: Mount and rotation reactions
Starting to ride a vehicle SHALL show `mount.onboard` (`mount.sit` on a cushion) on the action bar and align the player's yaw with a boat or raft. A `player_rotation` SHALL be answered with `move_player_rot`.

#### Scenario: Already riding
- **WHEN** a passengers update repeats a vehicle the player already rides
- **THEN** no hint is shown again

### Requirement: Common packet parity
`transfer` SHALL close the connection and reconnect to the target with the transfer intent, carrying stored cookies, from play and configuration; login cookie requests SHALL be answered from carried cookies. Only `http` and `https` server links without whitespace SHALL be kept. Custom report details SHALL be kept (32 at most). `reset_chat` SHALL clear the chat. `low_disk_space_warning` SHALL show the low disk space toast.

#### Scenario: Transfer with cookies
- **WHEN** a server stores a cookie and then sends `transfer`
- **THEN** the client closes and the next connection carries the cookie
