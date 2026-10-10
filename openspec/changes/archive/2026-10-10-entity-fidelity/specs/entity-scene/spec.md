## ADDED Requirements

### Requirement: Held item placement
The system SHALL place held items at the hand as vanilla does: the family-specific arm offset, rotation of -90 degrees about X then 180 degrees about Y, a (+-1, 2, -10) sixteenth-block translation (baby offsets for babies) and then the item's third-person display transform.

#### Scenario: Skeleton bow
- **WHEN** an aggressive skeleton holds a bow
- **THEN** both arms are raised toward the target and the bow sits in the hand of the main arm

#### Scenario: Left-handed mirror
- **WHEN** a left-handed player holds a sword
- **THEN** the item is mirrored using the left-hand display transform at the left arm

### Requirement: Mob animation coverage
The system SHALL animate fish tails, squid tentacles, bats, blaze rods, ghast tentacles, slime squish, wolf tail and sitting pose, enderman carried block and open mouth, golem arms, guardian tail, phantom wings, the ender dragon, wither heads, horse rearing, llama chests, strider legs, hoglin, ravager, shulker peek, vex and parrot from entity state.

#### Scenario: Sitting wolf
- **WHEN** a tamed wolf view has the sitting flag
- **THEN** its body is tilted back and its legs folded

#### Scenario: Puffed pufferfish
- **WHEN** a pufferfish view has puff state 2
- **THEN** the large puffed model is drawn

### Requirement: Projectile and effect entities
The system SHALL draw thrown items, experience orbs, fishing bobbers with their line, firework rockets, end crystals and lightning bolts, with sprite entities facing the camera.

#### Scenario: Snowball
- **WHEN** a snowball view is rendered
- **THEN** a flat item sprite facing the camera is drawn

### Requirement: Entity shadows
The system SHALL draw a blob shadow under each entity sized by its type's shadow radius and projected onto the top of the block found by the caller-supplied height callback, fading with height above the surface.

#### Scenario: Entity above ground
- **WHEN** a pig stands on a block whose top is the entity's feet height
- **THEN** a dark translucent quad lies on the block top beneath it

### Requirement: Enchantment glint
The system SHALL overlay a scrolling additive glint on enchanted armor and held items.

#### Scenario: Enchanted helmet
- **WHEN** a zombie wears an enchanted helmet
- **THEN** the helmet layer is drawn again with an animated glint texture

### Requirement: Update cost
The system SHALL resolve 2000 mixed entities in under one millisecond of CPU time per update on the reference machine without per-frame heap allocation in steady state.

#### Scenario: Stress scene
- **WHEN** 2000 mixed entities are updated repeatedly
- **THEN** each update takes under one millisecond
