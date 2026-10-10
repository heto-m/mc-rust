# entity-scene Specification

## Purpose
Render live entities of a Minecraft world: map each entity type and its data to models, textures, layers and animation, and draw them in a caller's render pass.

## Requirements

### Requirement: Scene API
The system SHALL expose an `EntityScene` that accepts entity views each frame and renders them into a caller-provided render pass with caller-specified colour and depth formats and reverse-Z depth.

#### Scenario: Render a mixed set
- **WHEN** a pig, a zombie and a player view are updated and rendered offscreen
- **THEN** each appears with its own texture at its interpolated position

#### Scenario: Camera-relative positions
- **WHEN** entities far from the origin are rendered with a camera position
- **THEN** their positions are subtracted in double precision before upload

### Requirement: Type mapping
The system SHALL map every entity type of the version to a model and texture set, SHALL choose baby models and variant textures from entity data resolved through a caller-supplied registry lookup, and SHALL draw a hitbox cube for types it cannot render.

#### Scenario: Cat variants
- **WHEN** cat views carry the red, black and tabby variant ids
- **THEN** each uses the matching variant texture

#### Scenario: Unknown variant id
- **WHEN** a variant id has no registry entry
- **THEN** the default texture of the type is used

#### Scenario: Unsupported type
- **WHEN** a type has no renderer mapping
- **THEN** a cube sized to its hitbox is drawn

### Requirement: Equipment layers
The system SHALL render worn armor, saddles, harnesses and body armor from equipment assets, with dyed leather colours and trim palette swaps, and SHALL render held items using item model display transforms.

#### Scenario: Trimmed diamond armor
- **WHEN** a zombie wears a diamond chestplate with a trim component
- **THEN** a diamond armor layer and a palette-swapped trim layer are drawn over the body

#### Scenario: Sword in hand
- **WHEN** a zombie holds a sword in its main hand
- **THEN** the item model is drawn at the hand bone

### Requirement: Item-like entities
The system SHALL draw dropped items with bobbing and spin, stack-size clones, item frame contents, falling blocks, block displays and paintings from block and item models.

#### Scenario: Stack clones
- **WHEN** a dropped item has a count of 40
- **THEN** several offset copies are drawn

### Requirement: Animation
The system SHALL port vanilla's procedural animation for biped, quadruped, chicken, villager, creeper, spider and slime models, SHALL drive extracted keyframe animations from pose and state, and SHALL apply hurt tint and death rotation.

#### Scenario: Crouching player
- **WHEN** a player view has the crouching pose
- **THEN** the body leans forward and the head drops

#### Scenario: Death
- **WHEN** a living entity's death time grows
- **THEN** it rotates toward lying on its side

### Requirement: Players
The system SHALL choose one of the 18 vanilla default skins from the player UUID when no skin is supplied, SHALL select slim or wide arm models, and SHALL honour the skin layer bitmask.

#### Scenario: Offline UUID
- **WHEN** a player view has no skin
- **THEN** the default skin selected by the UUID is used

### Requirement: Name tags
The system SHALL report the screen-space position and text of visible name tags for the UI to draw.

#### Scenario: Behind the camera
- **WHEN** an entity is behind the camera
- **THEN** no name tag is reported

### Requirement: Batched performance
The system SHALL draw instances in one call per pipeline, model and texture array and keep per-frame preparation of 2000 mixed entities within about one millisecond of CPU time.

#### Scenario: Stress scene
- **WHEN** 2000 mixed entities are prepared
- **THEN** preparation takes about a millisecond or less on the reference machine

### Requirement: Bounded resources
The system SHALL bound per-frame instances, bone matrices, texture array layers, baked item geometry and bakes per frame regardless of entity data, item stacks and counts sent by the server, dropping the farthest instances first.

#### Scenario: Hostile stacks
- **WHEN** thousands of item entities with distinct item ids, dye colours and counts are updated
- **THEN** cached geometry, uploaded models and instances stay within their caps

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
