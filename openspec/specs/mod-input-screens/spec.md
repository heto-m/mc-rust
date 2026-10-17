# mod-input-screens Specification

## Purpose
In mod mode, mcr delivers its window input to the embedded Java client as SDL3-equivalent events, serves Java's platform requests from its own window, and splits UI and gameplay authority between Java's screens and mcr.

## Requirements

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
- **WHEN** the system clipboard holds `hello` and the user presses Ctrl+V in a focused Java text field
- **THEN** the field contains `hello`

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
- **THEN** mcr's menu overlay (settings and mod manager) appears and Java receives no input until it closes

### Requirement: Quit propagation
Quitting from Java SHALL quit mcr.

#### Scenario: Quit game
- **WHEN** the user clicks Quit Game on Java's title screen
- **THEN** mcr shuts down cleanly

### Requirement: Lock modifier bits
The modifier state sent to Java SHALL include the SDL Caps Lock and Num Lock bits, toggled by each press of those keys.

#### Scenario: Caps Lock
- **WHEN** the user presses Caps Lock once
- **THEN** Java's key events carry the Caps Lock modifier until it is pressed again

### Requirement: Single text source with an IME
While an IME composition is in progress, text SHALL reach Java only through IME commits, never also through key press text. Text input being enabled without a composition SHALL NOT suppress key press text.

#### Scenario: Composed character
- **WHEN** the IME commits a character that winit also reports as key text
- **THEN** Java receives it once

#### Scenario: Text input without an input method
- **WHEN** the compositor enables text input with no input method running and the user types "hi" into Java's chat
- **THEN** Java's chat box contains "hi"

### Requirement: Held keys survive a screen closing
Movement keys still held when a Java screen closes SHALL keep moving the player without a re-press.

#### Scenario: Closing the inventory while walking
- **WHEN** the user holds W, a Java screen opens and closes while W stays down
- **THEN** the player walks forward again when the screen closes

### Requirement: One resource-pack prompt
In mod mode, a server resource pack SHALL produce exactly one prompt and exactly one response packet to the server.

#### Scenario: Server pushes a pack
- **WHEN** the server pushes a resource pack while Java is attached
- **THEN** only mcr's prompt shows and the server receives a single answer

### Requirement: Native menu layout
The native "mcr" menu SHALL draw on an opaque panel with its title above its buttons, hiding Java's controls beneath it.

#### Scenario: Pause screen
- **WHEN** the user presses the "mcr" button on Java's pause screen
- **THEN** the menu's title and buttons are drawn without Java's buttons showing through

### Requirement: Key mapping sync
Vanilla key mappings SHALL agree between mcr and Java in both directions, with mcr's `options.txt` as the only persisted copy. Rebinding a vanilla mapping in any Java screen SHALL change mcr's binding; rebinding in mcr's controls SHALL change Java's `KeyMapping`. Mappings that mods add SHALL stay Java-only.

#### Scenario: Rebinding in Java
- **WHEN** a Java screen (vanilla Controls or a mod such as Controlling) binds jump to J
- **THEN** mcr's jump binding becomes J and is saved in mcr's `options.txt`

#### Scenario: Rebinding in mcr
- **WHEN** the user binds forward to the up arrow in mcr's controls
- **THEN** Java's forward mapping becomes the up arrow before Java's next frame

#### Scenario: Java's own file is stale
- **WHEN** Java's `options.txt` holds an older binding for a vanilla mapping at startup
- **THEN** mcr's binding wins and mcr's file is not changed

### Requirement: Lock keys from the keyboard
On Linux, the Caps Lock and Num Lock modifier bits sent to Java SHALL start from the keyboard's lock LEDs when the mod host starts.

#### Scenario: Num Lock on at startup
- **WHEN** mcr starts with mods while Num Lock is on
- **THEN** Java's first key events carry the Num Lock modifier

### Requirement: Menu backdrop
The native "mcr" menu drawn over Java's GUI SHALL cover the whole screen behind its panel so that no Java control shows around it.

#### Scenario: Wide window
- **WHEN** the "mcr" menu opens from Java's pause screen at 1280x720 and GUI scale 3
- **THEN** none of Java's pause buttons are visible around the panel

### Requirement: Movement in Java screens
While a Java screen is open, Java SHALL report which movement keys a mod let through to the player's input, and mcr SHALL move its player with exactly those keys. Without such a mod the player SHALL not move while a Java screen is open.

#### Scenario: InvMove-style mod
- **WHEN** a mod keeps the movement keys active in the inventory and the user holds forward with the inventory open
- **THEN** mcr's player walks forward

#### Scenario: No such mod
- **WHEN** the user holds forward with the inventory open and no mod lets movement through
- **THEN** mcr's player stands still

### Requirement: Raw SDL event hooks
Key and mouse button events SHALL also pass through vanilla's SDL event handler methods as SDL events, with vanilla's own handling there skipped, so that mods hooking those methods see them. An event a mod cancels there SHALL not reach Java's keyboard or mouse handler.

#### Scenario: Hotkey library
- **WHEN** a mod hooks vanilla's SDL key event handler and cancels E
- **THEN** pressing E in game does not open the inventory

#### Scenario: No hook
- **WHEN** no mod hooks the SDL event handler
- **THEN** keys and buttons reach Java exactly once, as before

### Requirement: Perspective key in mod mode
While Java leads the perspective, the toggle-perspective key SHALL reach Java's vanilla key mapping and mcr SHALL NOT cycle its own perspective on it. Rebinding the key in a Java screen SHALL rebind mcr's mapping as for every vanilla key.

#### Scenario: F5 with mods
- **WHEN** Java draws the world and the user presses F5
- **THEN** Java's toggle-perspective mapping is clicked once, and mcr's perspective changes only by following Java

#### Scenario: Rebound in Controlling
- **WHEN** the user binds toggle perspective to P in a Java screen and presses P in game
- **THEN** the perspective changes
