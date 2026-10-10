## ADDED Requirements

### Requirement: Menu layouts
The system SHALL provide the slot positions, background texture and window size of every 26.3 menu type (generic 9x1 to 9x6, 3x3, crafter, anvil, beacon, blast furnace, brewing stand, crafting, enchantment, furnace, grindstone, hopper, lectern, loom, merchant, shulker box, smithing, smoker, cartography table, stonecutter), the player inventory, and horse, llama, camel and nautilus inventories, with slot indices in vanilla order.

#### Scenario: Chest rows
- **WHEN** a 9x3 layout is built
- **THEN** it has 27 container slots followed by 36 player slots and is 176x168 pixels

### Requirement: Vanilla click semantics
The system SHALL translate mouse and keyboard input on container slots into the exact vanilla `(slot, button, ClickType)` triples: left and right press pickups, shift quick move, hotbar number keys and offhand key swap (button 0-8 and 40), middle clone in creative, Q and Ctrl+Q throw, click outside throw with slot -999, double click pickup all, and shift double click quick move of all matching stacks.

#### Scenario: Shift click
- **WHEN** an item slot is left clicked with shift held and nothing is carried
- **THEN** a `QuickMove` click with button 0 is produced for that slot

#### Scenario: Throw
- **WHEN** Ctrl+Q is pressed over an item
- **THEN** a `Throw` click with button 1 is produced

### Requirement: Quick craft drag
While an item is carried, pressing a mouse button SHALL start a drag; dragging over eligible slots SHALL collect them; releasing SHALL send a start click, one add click per slot and an end click, all `QuickCraft`, with button `header | type << 2` where header is 0, 1, 2 and type is 0 for left, 1 for right and 2 for middle in creative, and SHALL send a plain pickup when no slot was collected.

#### Scenario: Left drag over two slots
- **WHEN** a stack is dragged with the left button across two empty slots a and b
- **THEN** the clicks are (-999, 0), (a, 1), (b, 1), (-999, 2)

### Requirement: Container rendering
The system SHALL draw the background, labels, slots with hover highlight, items with decorations, the carried item at the cursor, dynamic parts (furnace flame and arrow, brewing progress, enchanting options, anvil cost, beacon buttons, villager offers, selection lists) and item tooltips.

#### Scenario: Furnace
- **WHEN** a furnace has burn data
- **THEN** the flame height and arrow width follow the container data like vanilla
