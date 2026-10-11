## Context

Vanilla 26.3 input runs entirely on SDL3. `com.mojang.blaze3d.platform.SDLEventHandler.pollEvents()` drains
`SDL_PollEvent` and marshals each event onto the game thread with `minecraft.execute`:

| SDL event | Vanilla call |
|---|---|
| key down/up (768/769) | `KeyboardHandler.keyPress(long window, int action, KeyEvent(int scancode, int keycode, int mods))`. Action is 1 press, 0 release, -1 repeat |
| text input (771) | `KeyboardHandler.textInput(long, String)` → `charTyped(long, CharacterEvent)` per code point |
| text editing (770) | `KeyboardHandler.textEditing(long, PreeditEvent)` |
| mouse motion (1024) | `MouseHandler.onMove(long, double x, double y, double xrel, double yrel)` |
| mouse button (1025/1026) | `MouseHandler.onButton(long, MouseButtonInfo(button, SDL_GetModState()), action)` |
| wheel (1027) | `MouseHandler.onScroll(long, double dx, double dy)`, with the shift-inverts quirk from `InputQuirks` |
| drop (4096/4098/4099) | `MouseHandler.onDrop(long, List<String>)` |

The `InputConstants.KEY_*` values are SDL scancodes. `KeyEvent` carries SDL scancode, SDL keycode and `SDL_Keymod`.
Other SDL-backed calls:
- `InputConstants.isKeyDown(int)` (`SDL_GetKeyboardState`)
- `InputConstants.grabMouse(Window, x, y)`/`releaseMouse` (`SDL_WarpMouseInWindow`, relative mouse mode)
- `Window.selectCursor(CursorType)`
- `ClipboardManager.getClipboard/setClipboard`
- `TextInputManager.startTextInput/stopTextInput/setTextInputArea`

`MouseHandler.handleAccumulatedMovement()` turns the player via private `turnPlayer(double)` when the mouse is
grabbed.

Fabric API mixes into `MouseHandler`, `KeyboardHandler` and `Screen` (screen-api, events-interaction) and
`KeyMapping`/`Options` (key-mapping-api). Vanilla screens are therefore never "untouched" once Fabric API is
loaded, which is why authority is all-Java in mod mode.

## Decisions

**Delivery.**
- `mcr-app` translates winit events in the mod-mode input layer:
  - `KeyCode` → USB HID usage, which equals the SDL scancode.
  - Keycode is the lowercase code point of `key_without_modifiers` for printable keys, else
    `scancode | 0x40000000` (`SDLK_SCANCODE_MASK`).
  - `ModifiersState` plus side tracking → `SDL_Keymod` bits.
  - Mouse buttons → SDL button numbers (1 left, 2 middle, 3 right, 4/5 extra).
  - Wheel lines → SDL wheel units. Pixel deltas are scaled by 1/120 per platform convention.
- Positions are in window pixels, and xrel/yrel are raw `DeviceEvent::MouseMotion` deltas.
- Messages go into the control ring in arrival order.
- `awaitFrame` delivers them before `runTick`, calling the handlers directly on the Minecraft thread (the same
  thread `minecraft.execute` would use), with `window.handle()` as the window argument.
- SDL's own event queue on the offscreen driver only carries window events and stays drained by vanilla.

**Redirects (mixins).**

| Target | New behaviour |
|---|---|
| `InputConstants.isKeyDown(int)` | read the 512-bit key-state bitset that Rust updates before posting `FrameBegin` |
| `InputConstants.grabMouse`/`releaseMouse` | downcall `set_cursor_grab(bool)`. Rust applies `CursorGrabMode::Locked` (fallback `Confined`) and visibility on the main thread |
| `Window.selectCursor(CursorType)` | downcall `set_cursor(kind)` → winit `CursorIcon` |
| `ClipboardManager.getClipboard/setClipboard` | downcalls to mcr's clipboard |
| `TextInputManager.startTextInput/stopTextInput/setTextInputArea` | winit `set_ime_allowed`/`set_ime_cursor_area` |
| `SDL_GetModState()` call sites in `SDLEventHandler` and `InputQuirks` | `@WrapOperation` → Rust's current modifiers |
| `MouseHandler.turnPlayer(double)` | cancelled. Rust owns rotation through the player shadow |
| `Minecraft.stop()` | TAIL: downcall `request_quit` |

**UI authority.** In mod mode Java is the GUI. Rust shares a `java_screen_open` flag, written by the
`Gui.setScreen` TAIL hook and also true while `Gui.overlay()` is non-null.
- **Screen open:** every input event goes to Java only. Rust releases its held gameplay keys (no stuck movement),
  ignores gameplay bindings, and keeps the cursor as Java requests.
- **No screen:**
  - Key events go to both. Rust acts on gameplay bindings. Java's `Minecraft.handleKeybinds` (filtered by
    `mod-net-bridge`) acts only on screen-opening and mod mappings.
  - Mouse motion goes to Rust for look. It is also forwarded so mods reading `MouseHandler` positions work, and
    `turnPlayer` is cancelled.
  - Buttons go to Rust (attack/use/pick) and to Java, with vanilla attack/use suppressed in Java.
  - Scroll goes to both: Rust handles the hotbar, and Java's change is overwritten by the shadow while mods (zoom)
    still see it.

Keybind split:

| Owner | Mappings |
|---|---|
| Rust (vanilla mapping suppressed in Java) | movement, jump, sneak, sprint, attack, use, pick block, drop, swap offhand, hotbar 1–9, toggle perspective, fullscreen, smooth camera |
| Java (mcr's equivalent disabled in mod mode) | inventory, chat, command, player list, advancements, social interactions, screenshot, spectator outlines, Esc/pause, F3 debug screens, mod mappings |

Server-opened screens (`ClientboundOpenScreenPacket`, sign editor, book, dialogs, death, disconnect) are handled by
Java's listener. mcr's UI layer skips opening native screens in mod mode but keeps its container model for
inventory logic and prediction.

**mcr overlay entry.** The bridge registers Fabric `ScreenEvents.AFTER_INIT` for `TitleScreen` and `PauseScreen` and
adds a small "mcr" button. Its downcall `open_native(kind)` makes mcr draw its native settings or mod-manager screen
above the `GUI` slot. While that overlay is up, input goes to mcr, Java gets nothing, and the Java frame keeps
rendering.

**Title screen buttons.** HEAD hooks on `TitleScreen`'s singleplayer and Realms actions show a toast ("not
supported yet"). Multiplayer is Java's `JoinMultiplayerScreen` over `<data>/java/servers.dat`, and joins flow through
`ConnectScreen` → Rust (`mod-net-bridge`). mcr's own server list is still reachable from the "mcr" overlay.

## Risks

- Keyboard layout fidelity: SDL keycodes depend on layout. Printable code points from winit's
  `key_without_modifiers` match SDL for common layouts. Dead keys and IME are handled through text events.
- Raw mouse deltas versus SDL relative mode scaling: verify that sensitivity matches vanilla, where mods compute
  from deltas.
- winit cursor lock is unsupported on some platforms (X11 confined only). The fallback is confine plus recenter.
