# ui-menu-screens Specification

## Purpose
Chat screen with command suggestions, and the death, pause, disconnected, connecting, title, server list and direct connect screens.

## Requirements

### Requirement: Chat screen
The system SHALL provide a chat input with up and down history, Enter sending through `SendChat` or `SendCommand` actions, Escape closing, a scrollable chat history while open, and click events on chat components surfaced as actions (`OpenUrl` needing confirmation, `RunCommand`, `SuggestCommand`, `CopyToClipboard`).

#### Scenario: Send
- **WHEN** Enter is pressed with text "hi"
- **THEN** `SendChat("hi")` is returned and the screen closes

#### Scenario: Command
- **WHEN** Enter is pressed with text "/gamemode creative"
- **THEN** `SendCommand("gamemode creative")` is returned

### Requirement: Command suggestions
The system SHALL query a `CommandSuggester` as the text changes and show a popup of suggestions, selectable with Up, Down, Tab and Enter, applying the chosen text at the suggestion range.

#### Scenario: Tab completion
- **WHEN** suggestions are shown and Tab is pressed
- **THEN** the selected suggestion replaces the range it applies to

### Requirement: Death screen
The system SHALL show the death title, the score and Respawn and Title Screen buttons that return `Respawn` and `Disconnect`, with a hardcore variant.

#### Scenario: Respawn
- **WHEN** Respawn is clicked
- **THEN** `Respawn` is returned

### Requirement: Pause screen
The system SHALL show Return to Game, Options and Disconnect buttons returning `Pop`, `OpenOptions` and `Disconnect`, and Escape SHALL close it.

#### Scenario: Escape
- **WHEN** Escape is pressed
- **THEN** the screen is popped

### Requirement: Disconnect and connect screens
The system SHALL show a disconnect reason component with a Back button, and a connecting screen with a status line and Cancel.

#### Scenario: Reason
- **WHEN** a disconnect reason component is given
- **THEN** it is wrapped and centred above the Back button

### Requirement: Server list and direct connect
The system SHALL list saved servers from a `ServerStore` trait, allow adding, editing, deleting and reordering them, joining one with `Connect`, and a direct connect screen with an address box.

#### Scenario: Join
- **WHEN** a server is selected and Join is clicked
- **THEN** `Connect(address)` is returned

### Requirement: Title screen
The system SHALL show the title logo over a plain background with Multiplayer, Options and Quit buttons.

#### Scenario: Multiplayer
- **WHEN** Multiplayer is clicked
- **THEN** the server list screen is pushed
