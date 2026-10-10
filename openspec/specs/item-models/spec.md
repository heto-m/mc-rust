# item-models Specification

## Purpose
Parse vanilla item model definitions, evaluate them for an item stack and context, and bake the resulting models into quads.

## Requirements

### Requirement: Item model definition parsing
The system SHALL parse every `items/<id>.json` into a tree supporting `model`, `composite`, `condition`, `select`, `range_dispatch`, `empty`, `special` and `bundle/selected_item` nodes, with tint sources `constant`, `dye`, `grass`, `firework`, `potion`, `map_color`, `team` and `custom_model_data`. Unknown node types SHALL be an error naming the file.

#### Scenario: Whole jar
- **WHEN** every `items/*.json` of the vanilla jar is parsed
- **THEN** all parse without error

### Requirement: Item model evaluation
The system SHALL evaluate a definition against an `ItemProps` implementation (components) and an `ItemContext` (display context, use state, time, dimension, hand) and return an ordered list of layers, each a model identifier or a typed special model, with resolved ARGB tints. Missing context SHALL fall back to the definition's `fallback`/`on_false` branches.

#### Scenario: Bow pulling
- **WHEN** the bow is evaluated while using the item with a use duration of 15 ticks
- **THEN** the layer is `item/bow_pulling_2`

#### Scenario: Compass default
- **WHEN** the compass is evaluated with no tracker and no angle
- **THEN** a compass frame model is returned without error

#### Scenario: Leather dye
- **WHEN** a leather helmet has a dyed colour
- **THEN** the layer tint is that colour; undyed it is the definition's default

### Requirement: Display transforms and builtin parents
The system SHALL resolve `display` transforms (inheriting per context from parents), `gui_light`, and record whether a model descends from `builtin/generated` or `builtin/entity`.

#### Scenario: Block item gui transform
- **WHEN** `block/cube_all` is resolved
- **THEN** its `gui` transform rotates by 30, 225, 0 and scales by 0.625

### Requirement: Generated model baking
The system SHALL bake `builtin/generated` models with textures `layer0..layerN` into a front and a back face per layer plus edge faces along the sprite outline wherever an opaque pixel borders a transparent one, with the layer index as tint index.

#### Scenario: Edge faces
- **WHEN** a sprite with a single opaque pixel is baked
- **THEN** it has four edge faces plus front and back

### Requirement: Special models
Special item models (chest, shield, banner, head, bed, conduit, decorated pot and similar) SHALL be reported as a typed marker carrying the `base` model, so that callers render the base model's icon.

#### Scenario: Chest
- **WHEN** the chest item is evaluated
- **THEN** the layer is a special marker of kind `chest` with base `item/chest`
