## ADDED Requirements

### Requirement: Creative tab data
The system SHALL ship the vanilla 26.3 creative tab list as extractor output: per tab its key, title key, row, column, alignment, type, icon and the stacks shown in its own tab and in the search tab, each with the features its item requires. Stacks SHALL be stored in network encoding. Tabs SHALL be shown only when every required feature is enabled by the server, and the operator tab only for an operator in creative mode with the operator-items option on.

#### Scenario: Vanilla layout
- **WHEN** the bundled tabs are parsed
- **THEN** they are building blocks, colored blocks, natural blocks, functional blocks, redstone, hotbar, search, tools, combat, food, ingredients, spawn eggs, operator and inventory, in that order

#### Scenario: Feature gating
- **WHEN** an entry requires a feature the server did not enable
- **THEN** it is left out of its tab and of search

### Requirement: Creative screen behaviour
Opening the inventory in creative mode SHALL show the creative screen with a 9x5 item grid, a hotbar row, a scroll bar for tabs with more than 45 items, and tab buttons. Scrolling SHALL move whole rows and clamp. The search tab SHALL filter by name or id, and by item tag when the text starts with `#`. The inventory tab SHALL show armor, offhand, main inventory, hotbar and a destroy slot. The screen SHALL close itself when the player leaves creative mode.

#### Scenario: Picking from the grid
- **WHEN** the player left-clicks a grid stack with an empty cursor
- **THEN** the cursor holds a copy of the shown stack, a shift-click a full stack, and a middle-click a full stack as well

#### Scenario: Same item on the cursor
- **WHEN** the cursor holds the clicked item
- **THEN** a left-click grows it by one up to the maximum and a right-click shrinks it by one

#### Scenario: Deleting
- **WHEN** the cursor holds a different item and a grid slot is left-clicked, or the destroy slot is clicked
- **THEN** the cursor is cleared

#### Scenario: Number keys
- **WHEN** a number key is pressed over a grid stack
- **THEN** hotbar slot N receives a full stack

### Requirement: Creative slot protocol
Changes to the player's inventory made in the creative screen SHALL be applied locally and sent as `set_creative_mode_slot` for every changed inventory slot, throwing items SHALL send slot -1, and the cursor SHALL never be sent. The packet SHALL encode component payloads length-prefixed.

#### Scenario: Placing into the hotbar
- **WHEN** the cursor stack is clicked into a hotbar slot
- **THEN** the server accepts it and sends no correcting slot update

### Requirement: Saved hotbars
Nine saved hotbars SHALL persist across runs. With the save activator held, a hotbar number key SHALL store the current hotbar; with the load activator held it SHALL replace the hotbar with the stored one.

#### Scenario: Round trip
- **WHEN** a hotbar is saved and the store is written and read back
- **THEN** the saved stacks are identical
