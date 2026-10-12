## 1. Translation
- [x] 1.1 winit → SDL3 scancode, keycode, modifier and button tables, with unit tests covering letters, digits, function keys, numpad, modifiers sides and a non-US printable key
- [x] 1.2 Input messages and ordering through the control ring; key-state bitset

## 2. Java delivery and redirects
- [x] 2.1 Delivery in `awaitFrame` to `KeyboardHandler`/`MouseHandler`
- [x] 2.2 Mixins: `isKeyDown`, grab/release, `selectCursor`, clipboard, text input, `SDL_GetModState` sites, `turnPlayer`, `Minecraft.stop`
- [x] 2.3 Rust side: cursor grab/icon, clipboard, IME, quit handling on the main thread

## 3. Authority
- [x] 3.1 `java_screen_open` flag, mcr gameplay gating, key release on screen open
- [x] 3.2 Keybind split: Java-side suppression list update, mcr-side mod-mode disable list
- [x] 3.3 Skip native screen opening in mod mode while keeping container state
- [x] 3.4 "mcr" button on title and pause screens, native overlay with input capture, singleplayer and Realms toasts

## 4. Verification
- [x] 4.1 Test-input driver for mcr (scripted winit-equivalent events through the same translation layer)
- [x] 4.2 Integration script from the proposal's acceptance test against `xtask server`
- [x] 4.3 Check the forward list against container, anvil, beacon, creative, recipe book, sign, book and suggestions; extend `mod-net-bridge`'s table where the capture shows drops
- [x] 4.4 fmt, clippy, tests, archive
