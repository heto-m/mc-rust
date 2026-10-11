## ADDED Requirements

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
