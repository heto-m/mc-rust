# entity-rendering Specification

## Purpose
Draws entities, name tags, the first-person hand and the inventory player preview from client state, and wires sky, weather, destroy decals and particles into the frame.

## Requirements

### Requirement: Entities are drawn from client state
The app SHALL draw every loaded entity within the entity render distance and the view frustum, interpolated between ticks, and SHALL skip entities outside loaded chunks.

#### Scenario: Entity outside range
- **WHEN** an entity is farther than the entity render distance
- **THEN** it is not submitted to the scene

#### Scenario: Interpolated position
- **WHEN** a frame is drawn at partial tick 0.5
- **THEN** the entity is placed midway between its previous and current tick positions

### Requirement: Name tags follow vanilla visibility
Name tags SHALL show for players and for named entities with a visible custom name, within 64 blocks (32 when sneaking), never for the local player in first person, and SHALL honour team name-tag visibility.

#### Scenario: Hidden by team
- **WHEN** the entity's team hides name tags
- **THEN** no tag is drawn

### Requirement: First-person hand
In first person the app SHALL draw the held item or the player's arm with a separate projection and cleared depth, with equip, swing and use animations.

#### Scenario: Slot change
- **WHEN** the selected hotbar item changes
- **THEN** the hand lowers and raises the new item over several ticks

### Requirement: Inventory preview
The inventory screen SHALL render the local player in its preview rectangle, turning toward the mouse.

#### Scenario: Preview present
- **WHEN** the player inventory is open
- **THEN** the player model is drawn inside the screen's entity rectangle

### Requirement: World effects are wired
The app SHALL configure the environment per dimension, clouds from options, weather while raining, destroy decals and particles from network events.

#### Scenario: Rain
- **WHEN** the server starts rain in the overworld
- **THEN** precipitation is drawn around the camera

### Requirement: Entity event effects
An `entity_event` SHALL produce the client effects of vanilla's most-derived `handleEntityEvent` override for the entity's class: particles, the sounds vanilla plays locally, and the animations the event starts, timed like the entity's own tick. Events SHALL have no effect on classes that do not override them.

#### Scenario: Failed and successful taming
- **WHEN** a wolf, cat, parrot or horse receives event 6, then event 7
- **THEN** seven smoke particles appear around it, then seven hearts

#### Scenario: Shared event id
- **WHEN** event 3 arrives for a snowball and for a zombie
- **THEN** the snowball bursts into snowball particles and the zombie starts its death pose without any burst

#### Scenario: Firework explosion
- **WHEN** a firework rocket with explosions receives event 17
- **THEN** its explosions burst one every other tick in their shapes and colours with a flash and the blast sound

#### Scenario: Animation clocks
- **WHEN** a sheep receives event 10
- **THEN** its head lowers for 40 ticks as vanilla's grazing animation

### Requirement: Tamed pet collars
Tamed wolves and cats SHALL wear a collar tinted with their synced collar colour, and with red when the server has not sent one.

#### Scenario: Freshly tamed wolf
- **WHEN** a wolf's tame flag is set and no collar colour was synced
- **THEN** it renders a red collar

### Requirement: Elytra glide pose
Fall-flying players and mannequins SHALL be drawn like vanilla's `AvatarRenderer.setupRotations`: the body pitches by `fallFlyingScale * (-90 - xRot)` with `fallFlyingScale = clamp(t^2 / 100, 0, 1)` over the fall-flying ticks and rolls by the angle between the horizontal movement and look directions; the limb swing is divided by `speedValue = max(1, (|v|^2 / 0.2)^3)`. Riptide spins and swimming use the same rotation stage.

#### Scenario: Level glide
- **WHEN** a player has glided for 10 ticks looking at the horizon
- **THEN** the model lies flat along its view direction

### Requirement: Elytra wings layer
Every humanoid wearing an equippable with a wings layer SHALL show it 2 pixels behind the body, lowered 3 pixels while crouching, with both wings rotated from `ElytraAnimationState` angles that ease 30% per tick toward the gliding, crouching or resting targets.

#### Scenario: Wings spread in a dive
- **WHEN** a gliding player falls almost straight down for a second
- **THEN** the wings close toward 15 degrees out, and they spread toward 90 degrees when it flies level

### Requirement: Thrown item billboards
Entities drawn by vanilla's `ThrownItemRenderer` SHALL draw their one ground-context item turned to the camera orientation, without bob or spin, at scale 1 (fireball 3, small fireball 0.75), with full block light for the eye of ender and fireballs. The dragon fireball SHALL be a camera-facing quad twice block size at full block light.

#### Scenario: Snowball faces the camera
- **WHEN** a snowball is seen from the front, the side or behind
- **THEN** the same face of the snowball item points at the camera

### Requirement: Fresh projectiles near the camera
Thrown items, fireballs and eyes of ender SHALL stay hidden while younger than 2 ticks within 3.5 blocks of the local player.

#### Scenario: Own throw
- **WHEN** the local player throws a snowball
- **THEN** it is not drawn on its first tick in front of the camera

### Requirement: Lit TNT
Primed TNT and lit TNT minecarts SHALL use vanilla's `fuseRemainingInTicks = fuse - partial + 1`: for the last 10 ticks the block swells by `(1 - fuse / 10)^4 * 0.3` and while `(fuse / 5) % 2 == 0` it is drawn with the white overlay. Primed TNT counts its synched fuse down locally between server updates, a TNT minecart is primed by entity event 70 with an 80-tick fuse, and both puff a `smoke` particle each tick while the fuse burns.

#### Scenario: Flash cadence
- **WHEN** a primed TNT has 3 fuse ticks left
- **THEN** it is drawn white and swollen, and with 35.5 ticks left it is drawn normally

### Requirement: Minecart contents
Minecarts SHALL draw their display block (the custom one from entity data, else TNT, chest, furnace lit by its fuel flag, hopper, spawner or command block by type) at 0.75 scale, lifted by the display offset. Minecarts, boats and arrows SHALL NOT apply the living-entity model drop, so they sit at their entity position.

#### Scenario: TNT minecart
- **WHEN** a TNT minecart is drawn
- **THEN** a TNT block sits inside the cart

### Requirement: Non-living model placement
Entities whose vanilla renderer is not a `LivingEntityRenderer` SHALL be posed by their own renderer's transform stack: the living model drop SHALL NOT be applied to them. The leash knot, llama spit, shulker bullet, wind charges, trident, wither skull and end crystal SHALL sit at their vanilla heights above the entity origin.

#### Scenario: Wind charge
- **WHEN** a wind charge is drawn at an entity position
- **THEN** its model is centred on that position, not 1.5 blocks above it

#### Scenario: Llama spit
- **WHEN** llama spit is drawn with yaw and pitch
- **THEN** it is lifted 0.15 blocks and turned by yaw - 90 about Y and pitch about Z

### Requirement: Projectile model animation
Shulker bullets SHALL tumble with vanilla's sine/cosine angles, draw a 1.5x translucent shell and use full block light; wind charges SHALL counter-rotate their shells and scroll their texture; wither skulls SHALL turn by the projectile's yaw and pitch and use the invulnerable texture when dangerous.

#### Scenario: Dangerous wither skull
- **WHEN** a wither skull's dangerous flag is set
- **THEN** it uses the `wither_invulnerable` texture

### Requirement: Creeper swell
A creeper's client swell counter SHALL advance like `Creeper.tick` (the synched direction or the ignite flag, clamped to 0..30), scale the creeper with vanilla's wobble and draw it with the white overlay on alternate tenths of the swelling progress.

#### Scenario: Fuse lit
- **WHEN** a creeper's fuse direction becomes 1
- **THEN** it swells and flashes white until it explodes or the direction returns to -1
