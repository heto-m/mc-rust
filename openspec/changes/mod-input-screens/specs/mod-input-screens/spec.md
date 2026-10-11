## ADDED Requirements

### Requirement: SDL-equivalent input delivery
In mod mode, keyboard, text, IME pre-edit, mouse motion, button, wheel and file-drop events SHALL reach Java's `KeyboardHandler` and `MouseHandler` with the same SDL3 scancode, keycode, modifier and button values vanilla's SDL event handler would produce, in arrival order, before the next Java tick.

#### Scenario: Typing in a mod screen
- **WHEN** the user types `stone` into Mod Menu's search box
- **THEN** the box contains `stone` and the mod list is filtered

#### Scenario: Shift-click in inventory
- **WHEN** the user shift-clicks a stack in Java's inventory screen
- **THEN** the quick-move happens and the server confirms the slot change

### Requirement: Platform redirects
Java's key-state queries, cursor grab and release, cursor shape, clipboard and text-input requests SHALL be served by mcr's window and input state instead of SDL.

#### Scenario: Paste
- **WHEN** the system clipboard holds `hello` and the user presses Ctrl+V in Java's chat box
- **THEN** the chat box contains `hello`

### Requirement: UI authority in mod mode
In mod mode, Java SHALL own all game screens, the HUD and chat. Rust SHALL own movement, look, attack, use, pick, drop and hotbar input only while no Java screen or overlay is open, and SHALL release held gameplay keys when one opens. Server-opened screens SHALL open only in Java.

#### Scenario: Open inventory
- **WHEN** the user presses E in game
- **THEN** Java's inventory screen opens, mcr opens no native screen, and the player stops moving

#### Scenario: Mod keybind
- **WHEN** a mod registers a key mapping and the user presses it in game
- **THEN** the mod's handler runs once

### Requirement: Native overlay entry
Java's title and pause screens SHALL offer an entry that opens mcr's native settings and mod manager as an overlay that captures input while shown.

#### Scenario: Open mcr settings from pause
- **WHEN** the user clicks the mcr button on Java's pause screen
- **THEN** mcr's settings overlay appears and Java receives no input until it closes

### Requirement: Quit propagation
Quitting from Java SHALL quit mcr.

#### Scenario: Quit game
- **WHEN** the user clicks Quit Game on Java's title screen
- **THEN** mcr shuts down cleanly
