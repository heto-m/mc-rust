# recipe-book Specification

## Purpose
Recipe book panel in crafting and smelting screens, and bounded client storage of the recipes the server unlocks.

## Requirements

### Requirement: Recipe book panel

The client SHALL show the vanilla recipe book beside crafting and smelting screens, filter it by tab, search and craftable state, and send `PlaceRecipe` when the player picks a recipe.

#### Scenario: Placing a recipe

- **WHEN** the player clicks a craftable recipe in the panel
- **THEN** the client sends `PlaceRecipe` with the container id, the recipe display id and whether shift was held

#### Scenario: Ghost recipe

- **WHEN** the server sends a ghost recipe for the open container
- **THEN** the client outlines the missing ingredients in the crafting grid

### Requirement: Bounded recipe storage

The client SHALL drop recipe entries whose display weight reaches `MAX_RECIPE_WEIGHT` and SHALL keep the total weight of the recipe book at or below `MAX_RECIPE_BOOK_WEIGHT`.

#### Scenario: Oversized entry

- **WHEN** the server sends a recipe whose slot displays hold more than `MAX_RECIPE_WEIGHT` nodes
- **THEN** the entry is not stored and the other entries in the packet are

#### Scenario: Many heavy entries

- **WHEN** the server keeps adding heavy recipes across many packets
- **THEN** the stored weight never exceeds `MAX_RECIPE_BOOK_WEIGHT`
