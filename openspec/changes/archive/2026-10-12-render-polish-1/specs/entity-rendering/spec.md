## ADDED Requirements

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
