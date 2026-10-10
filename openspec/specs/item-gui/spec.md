# item-gui Specification

## Purpose
Draw item stacks into GUI slots with vanilla decorations, and show hotbar and offhand items in the HUD.

## Requirements

### Requirement: Item icons
The system SHALL draw an item at a 16x16 GUI slot from its evaluated layers: flat sprites for generated models and block models under the `gui` display transform, orthographic, lit by the vanilla GUI light directions. Each distinct variant SHALL be rasterised once per GUI scale and reused, and a resource reload SHALL drop the cache.

#### Scenario: Cache reuse
- **WHEN** the same item is drawn in 36 slots
- **THEN** it is rasterised once

#### Scenario: Block item
- **WHEN** a grass block is drawn
- **THEN** its top face is tinted green and lighter than its sides

### Requirement: Item decorations
The system SHALL draw a count (bottom right, shadowed) when the count is not 1, a durability bar for damaged items whose width is `round(13 - damage*13/max)` and colour is hue `(max-damage)/max/3` at full saturation and value, a white translucent cooldown overlay covering the remaining fraction from the bottom, and an animated glint over the item silhouette for enchanted items.

#### Scenario: Durability
- **WHEN** an item has damage 0 of 100
- **THEN** no bar is drawn; at damage 50 the bar is 7 pixels wide and yellow-orange

### Requirement: Hotbar items
The HUD SHALL draw the nine hotbar items and, when present, the offhand item from a slot list, in the vanilla slot positions.

#### Scenario: Offscreen hotbar
- **WHEN** a hotbar with varied items is rendered offscreen
- **THEN** the PNG shows each item in its slot, skipped when no GPU or jar exists
