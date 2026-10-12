## Decisions

- **Menu.** `ConfirmScreen::with_panel` fills an opaque bordered rectangle spanning the title and the buttons after
  the dim background, so Java's buttons under the menu are hidden and the title sits inside the panel above the
  buttons.
- **Resource pack.** The forward list has no `resource_pack` row in configuration or play, so any answer Java sends
  is dropped (`Owned`) and Rust's `PackManager` is the only sender of `ServerboundResourcePackPacket`. Suppressing
  mcr's prompt would leave the server without an answer, so Java's handlers (`handleResourcePackPush`/`Pop`) are
  cancelled at HEAD while `McrNet.bridged()`. Consequence: Java's own resource manager does not load server packs.
- **Lock keys.** winit does not report lock state, so the queue toggles `CAPS`/`NUM` on each non-repeat press of
  scancode 57 and 83, starting off. They survive `release_all`.
- **IME.** The shell tracks `Ime::Enabled`/`Disabled`. While active, text attached to key presses is not sent;
  commits are.
- **Held keys.** The route keeps a bitset of physically held keyboard keys. On `JavaScreen` to `JavaGame`, movement
  mappings (forward, back, left, right, jump, sneak, sprint) of those keys are pressed in Play unless already
  logically active. Attack and use are excluded so a click that closed a screen does not attack.
- **Try again.** `current_exe()` with the same args is spawned, then the shell quits. The prompt appears instead of
  booting Java, so no JVM exists yet. A failed spawn falls back to "Continue".
- **Nested jars.** `read_nested` recurses to depth 2 sharing one budget: 64 MiB, 1024 jars across the tree, and the
  existing per-jar (32 MiB) and per-folder (256 entries) caps.
- **Deferred: lang keys.** `mcr_ui::Lang` loads only `assets/*/lang/*.json` from the user's resource stack. There is
  no store for strings of our own, and `lang:` references resolve against it, so moving the Mods strings needs a
  separate mechanism.
