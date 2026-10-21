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

### Requirement: Leash ropes
A leashed entity SHALL draw a 24-segment rope from its leash offset to its holder's rope-hold position in alternating brown shades, and SHALL draw nothing for ropes longer than 64 blocks.

#### Scenario: Leashed mob
- **WHEN** an entity has a leash holder
- **THEN** pixels of the rope appear between the two attachment points

### Requirement: Thrown trident glint
A thrown trident with its foil flag set SHALL draw an additive enchantment glint layer over its model.

#### Scenario: Foil
- **WHEN** the synched foil flag is true
- **THEN** the plan holds a second, additive part using the item glint texture

### Requirement: Cushion and ominous item spawner
A cushion SHALL render its model in its dye colour facing its direction. An ominous item spawner SHALL render its item cluster growing over its first 50 ticks while spinning, fully lit.

#### Scenario: Cushion colour
- **WHEN** a cushion's colour is light blue
- **THEN** it uses `entity/cushion/light_blue_cushion`

### Requirement: Display billboards
Display entities SHALL orient by their billboard constraint: fixed uses the entity's rotation, vertical the camera yaw, horizontal the camera pitch and center both.

#### Scenario: Center
- **WHEN** the constraint is center
- **THEN** the display faces the camera

### Requirement: Ender dragon lean
The ender dragon SHALL turn by the yaw seven ticks ago and lean by ten degrees per block of height difference between five and ten ticks ago.

#### Scenario: Climbing
- **WHEN** the dragon gained 0.5 blocks between ten and five ticks ago
- **THEN** it leans 5 degrees

### Requirement: Elder guardian curse
The elder guardian curse event SHALL draw the guardian translucently in front of the camera for 30 ticks, fading in and out.

#### Scenario: Alpha
- **WHEN** the curse is halfway through
- **THEN** its alpha is 0.55

### Requirement: Text display layout
A text display SHALL draw its text component wrapped to its line width (default 200, clamped to 1..1024), at most 64 lines, as glyph geometry lit by the entity's light. Lines SHALL be aligned left, right or centered by the alignment flags and SHALL have a shadow when the shadow flag is set.

#### Scenario: Wrapping and alignment
- **WHEN** a text display has a line width of 60 and a text longer than 60 pixels
- **THEN** it draws several lines, none wider than 60 pixels, each aligned by the alignment flags

#### Scenario: Hostile text
- **WHEN** the text has tens of thousands of line breaks or the line width is `i32::MAX`
- **THEN** at most 64 lines and 1024 pixels of width are laid out

### Requirement: Text display colours and depth
The background of a text display SHALL be its ARGB colour, or black at the background opacity option when the default-background flag is set, and SHALL span one pixel past the text on the left and top. Text alpha SHALL be the opacity byte, where values below 4 draw opaque like vanilla. The see-through flag SHALL draw without depth testing.

#### Scenario: See-through
- **WHEN** a see-through text display is behind a wall
- **THEN** it is drawn over the wall

#### Scenario: Default background
- **WHEN** the default-background flag is set and the background option is 0.5
- **THEN** the background is black with alpha 127

### Requirement: Text display orientation
A text display SHALL follow its billboard constraint and its interpolated transformation, and its text SHALL be readable from the display's +Z side.

#### Scenario: Center billboard
- **WHEN** the constraint is center
- **THEN** the text faces the camera

### Requirement: Display interpolation
Block, item and text displays SHALL interpolate their transformation between the previous and the current value over the entity's interpolation duration, starting after its start delay. Translation and scale SHALL interpolate linearly and rotations spherically. A change without a duration SHALL apply at once.

#### Scenario: Halfway
- **WHEN** the translation changes from 0 to 10 over 10 ticks and 5 ticks have passed
- **THEN** the displayed translation is 5

### Requirement: Text display interpolation
Text displays SHALL interpolate the opacity as a signed byte and the background per ARGB channel over the same duration. Transformation components SHALL be clamped to 64 and non-finite values SHALL be treated as 0.

#### Scenario: Hostile transformation
- **WHEN** the scale is infinite and a rotation is NaN
- **THEN** the displayed transformation is finite

### Requirement: Display brightness and range
A display with a brightness override SHALL use its block and sky light instead of the world's. A display SHALL be drawn only within its view range times 64 blocks times the entity distance scaling, and SHALL skip frustum culling when its width or height is 0.

#### Scenario: Override
- **WHEN** the brightness override is block 15, sky 0
- **THEN** the display is lit with block light 15 and sky light 0

### Requirement: Texture layer reuse
The entity renderer SHALL return the texture-array layer of a replaced or evicted texture to its size class and SHALL NOT reuse it until the frames in flight have completed. Freeing a layer twice SHALL NOT make two textures share it.

#### Scenario: Replaced map
- **WHEN** a map image is replaced
- **THEN** its previous layer is freed and is reusable after four frames

#### Scenario: Freed layer still in flight
- **WHEN** a layer is freed and a texture is added in the same frame
- **THEN** the new texture gets a different layer

### Requirement: Bounded runtime textures
Skin and armor-trim textures composed at runtime SHALL be evicted when unused for 1800 frames, and SHALL be limited to 384 live textures by evicting the least recently used ones not used this frame. A texture a live entity uses SHALL NOT fall back to the missing texture because of eviction.

#### Scenario: Skin churn
- **WHEN** 10000 distinct skins are registered one per frame
- **THEN** at most 384 layers are live and each skin resolves to its own texture while in use

#### Scenario: Map table full
- **WHEN** a new map arrives while 256 maps are held
- **THEN** the least recently drawn map and its layer are released and the new map is kept

### Requirement: Display teleport duration
A display entity SHALL move to a new server position and rotation over its `teleport_duration` ticks (clamped to 0..59), covering `1 / remaining` of the way left each tick with the shortest-path yaw. With a duration of 0 it SHALL snap.

#### Scenario: Interpolated move
- **WHEN** a display with a teleport duration of 4 is told to move 8 blocks
- **THEN** it is 2, 4, 6 and 8 blocks along after four ticks

#### Scenario: Hostile duration
- **WHEN** the synced duration is `i32::MAX`
- **THEN** 59 ticks are used

### Requirement: Text display draw order
Text displays SHALL draw from the farthest to the nearest, each with its background and glyphs together, whether or not they are see-through.

#### Scenario: Overlapping displays
- **WHEN** two translucent text displays overlap on screen
- **THEN** the nearer one blends over the farther one

### Requirement: Passenger seats
A passenger SHALL stand at its vehicle's position plus the vehicle's passenger attachment for the passenger's index in the vehicle's passenger list, minus the passenger's own vehicle attachment, each scaled by age and the `scale` attribute. This SHALL apply to the local player and to remote riders. Boats, camels and happy ghasts SHALL use their per-seat positions rotated by the vehicle yaw.

#### Scenario: Player on a horse
- **WHEN** the local player rides an adult horse
- **THEN** the player's feet are 1.44375 - 0.6 blocks above the horse's position

#### Scenario: Two in a boat
- **WHEN** two players sit in an oak boat facing south
- **THEN** the first sits 0.2 blocks ahead of the boat's centre and the second 0.6 blocks behind it

### Requirement: Invisible entities
An invisible entity's body SHALL not be drawn unless the viewer may see invisible entities (spectator or
a team that sees friendly invisibles), in which case it SHALL be translucent. Armour and held items SHALL
still draw.

#### Scenario: Invisibility potion
- **WHEN** another player drinks Invisibility while wearing no armour
- **THEN** nothing of that player is drawn except held items

### Requirement: Use and pose states
First-person and third-person rendering SHALL animate item use for the hand that uses it, show the bow,
shield and crossbow use states on other entities, seat the local player when riding, and show burning
entities on fire.

#### Scenario: Off-hand shield
- **WHEN** the player blocks with an off-hand shield
- **THEN** the off-hand shield is raised in first person

### Requirement: Camera effects
The world view SHALL bob with view bobbing, tilt on damage, warp under nausea and in portals, and apply the
bow, death and fluid FOV changes with the vanilla clamp.

#### Scenario: Drawing a bow
- **WHEN** the player draws a bow fully
- **THEN** the FOV narrows by 15%

### Requirement: Particle providers
Every vanilla particle type SHALL have its own behaviour instead of the generic drift, including falling
leaves, tinted falling dust and dust pillars.

#### Scenario: Cherry leaves
- **WHEN** a cherry leaves block spawns a particle
- **THEN** it falls with gravity and lives up to 300 ticks

### Requirement: Curse overlay in view
The elder guardian curse SHALL draw the elder guardian model in front of the camera as vanilla's
`ElderGuardianParticle` does.

#### Scenario: Ocean monument
- **WHEN** an elder guardian applies mining fatigue
- **THEN** its face appears over the view and fades out

### Requirement: Hand-held rockets, elytra capes and boat water
An attached firework rocket SHALL sit at its holder's hand, elytra SHALL use the skin's elytra or cape
texture, and boats SHALL mask the water inside their hull.

#### Scenario: Boat on water
- **WHEN** a boat floats on water
- **THEN** no water surface shows inside the hull

### Requirement: Particles near the camera and map label order
Smoke particles at the camera SHALL stay soft translucent sprites, and map decoration names SHALL be drawn
with their icon in decoration order.

#### Scenario: Campfire smoke
- **WHEN** the camera stands in campfire smoke
- **THEN** no dark square covers the view

### Requirement: Spear hit feedback
The first-person spear SHALL dip by vanilla's hit feedback curve after a kinetic hit.

#### Scenario: Charge hit
- **WHEN** a spear charge hits a mob
- **THEN** the held spear dips and returns
