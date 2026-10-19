## ADDED Requirements

### Requirement: HUD elements
The HUD SHALL draw status effect icons, the attack indicator per option, the selected item name, mount
hearts and the jump bar, hunger icons, zero-saturation jitter, vanilla air bubbles, the off-hand slot on the
main-hand side, tab list hearts, the jukebox now-playing message, subtitles and the sleep fade, as `Hud`
does.

#### Scenario: Speed potion
- **WHEN** the player has Speed
- **THEN** its icon shows in the beneficial row at the top right

### Requirement: Paletted atlas textures
Item textures that exist only as the output of an atlas `paletted_permutations` source, such as the
armour trim overlays and their darker variants, SHALL be generated from the base texture and the key and
permutation palettes, as vanilla's atlas loader does.

#### Scenario: Gold-trimmed netherite helmet
- **WHEN** a netherite helmet carries a gold trim
- **THEN** its icon shows the gold trim overlay, not the missing texture

### Requirement: Crosshair and overlays
The crosshair SHALL use an inverting blend and draw only in first person (and in spectator only over a
menu provider). Camera overlays (pumpkin, powder snow, portal, nausea, fire, in-block, underwater,
spyglass, vignette) SHALL draw beneath the HUD.

#### Scenario: Scoping
- **WHEN** the player scopes with a spyglass
- **THEN** the hotbar and chat stay visible over the scope mask

### Requirement: Spectator menu
In spectator mode the item hotbar SHALL be replaced by the spectator menu, whose number keys teleport to
players and teams.

#### Scenario: Teleport to player
- **WHEN** a spectator opens the menu and picks a player
- **THEN** a teleport-to-entity packet with that player's UUID is sent
