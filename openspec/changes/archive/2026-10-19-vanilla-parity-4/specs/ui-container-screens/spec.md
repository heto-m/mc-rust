## ADDED Requirements

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
