# ui-container-screens Specification

## Purpose
Container layouts, vanilla click semantics and rendering for every menu type, mount inventories and the lectern.

## Requirements

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

### Requirement: Mount inventories
`mount_screen_open` SHALL open the horse or nautilus inventory screen, and the inventory key while riding a
mount with its own inventory SHALL send the OPEN_INVENTORY player command.

#### Scenario: Horse
- **WHEN** the server opens a mount screen for a saddled horse
- **THEN** the horse inventory screen opens and no close is sent

### Requirement: Screen data from the client state
The game view SHALL provide merchant offers, stonecutter results, loom patterns, enchanting clues, item
tooltips, trim materials, the spectator flag and the player score to the screens.

#### Scenario: Villager
- **WHEN** a merchant screen opens and offers arrive
- **THEN** the trade list shows every offer

#### Scenario: Stonecutter
- **WHEN** stone is placed in a stonecutter
- **THEN** its recipe buttons are listed

### Requirement: Correct player slot mirroring
The client SHALL map the player inventory slots of every menu by the menu's layout, so the crafter's
trailing result slot does not shift the inventory.

#### Scenario: Crafter
- **WHEN** a crafter screen's contents arrive
- **THEN** the hotbar shown in the HUD is unchanged

### Requirement: Anvil rename sync
Placing an item in the anvil SHALL set the name box to its hover name and send the rename packet as
vanilla's responder does.

#### Scenario: Renamed item
- **WHEN** a renamed sword is placed in the anvil
- **THEN** the name box shows the custom name and the result keeps it

### Requirement: Full item tooltips and bundles
Tooltips SHALL list the vanilla lines in order (name, components, enchantments, lore, attribute
modifiers, advanced lines with F3+H) filtered by `tooltip_display`, and bundles SHALL support scroll
selection, the contents tooltip and the fullness bar.

#### Scenario: Enchanted book
- **WHEN** the player hovers an enchanted book
- **THEN** the tooltip lists its stored enchantments
