# Proposal

## Why

Java's screens and HUD are visible after `mod-frame-compose` but unusable. They need winit input delivered as
if SDL produced it, and mcr needs one rule for who owns the UI at any moment.

## What Changes

- Input routing: winit events become SDL3 scancode, keycode and modifier values (a static table) and go through
  the control ring into the same handlers vanilla's `SDLEventHandler` calls:
  - `KeyboardHandler.keyPress`, `textInput`, `textEditing`
  - `MouseHandler.onButton`, `onMove`, `onScroll`, `onDrop`
- Platform redirects:
  - `InputConstants.isKeyDown` reads a shared key-state bitset.
  - `InputConstants.grabMouse`/`releaseMouse` drive winit cursor grab.
  - `Window.selectCursor` drives winit cursor icons.
  - `ClipboardManager` uses mcr's clipboard.
  - `TextInputManager` start/stop drives winit IME.
  - The SDL modifier-state reads in `SDLEventHandler`/`InputQuirks` use Rust's modifiers.
- UI authority in mod mode: Java owns screens, HUD and chat. Rust owns gameplay input (movement, look, attack,
  use, pick, drop, hotbar) only while no Java screen is open, and always owns the camera.
  - Server-opened screens (containers, signs, books, dialogs, death) open in Java only.
  - Java's vanilla screen-opening keybinds stay active and mcr's equivalents are disabled.
  - Java's title and pause screens get an "mcr" button that opens mcr's native settings and mod manager as an
    overlay.
- Quit and singleplayer: Java's Quit stops mcr. Singleplayer and Realms buttons are disabled until the
  singleplayer milestone.
- The `mod-net-bridge` forward list is checked against real screen use.

Out of scope:
- Gamepad input.
- Native mcr screens for vanilla screens while mods are loaded.
- Touch.
- Per-screen ownership exceptions (later, data-driven).

Acceptance test, scripted through the mcr test-input driver against a local vanilla server with Fabric API and
Mod Menu:
- Open Mod Menu from the title screen and its config screen, and type in its search box (IME off).
- Join, open the inventory with E and move a stack by click (the server confirms the slot change).
- Open chat with T and send a message (the server log shows it).
- Bind a test-mod key and press it in game (the test mod reports the press).
- Esc opens Java's pause screen and releases the cursor.

## Capabilities

### New Capabilities
- `mod-input-screens`: input translation and delivery, platform redirects, UI authority rules, quit and
  overlay entry points.

### Modified Capabilities

## Impact

`mcr-modbridge` (input tables), `mcr-app` (routing, overlay), `java/mcr-bridge` (mixins, buttons). Adds
`arboard` to `mcr-app` if it has no clipboard crate.
