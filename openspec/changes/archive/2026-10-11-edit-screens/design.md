# Design

## Context

`mcr-ui` has a screen stack, widgets, a single-line edit box and confirm screens; the client already sends `sign_update` and `edit_book` and parses the packets involved. Fonts are only reachable while rendering, so input handling cannot measure text directly.

## Goals / Non-Goals

**Goals:**
- Vanilla layout and limits for every editor, driven by language files.
- Screens independent of the network: they emit `ScreenAction`s; the app owns packets.
- Hostile dialog data bounded everywhere.

**Non-Goals:**
- Command suggestions in command block fields, 3D sign models, item components in dialog bodies, text filtering, IME preedit.

## Decisions

- **Measured widths without fonts in `input`.** Screens cache glyph advances at render and wrap or measure with the cache during input (unknown glyphs count 6 px), then re-check at the next render and revert an edit that overflowed. Alternative: add fonts to `Env`, which touches every call site.
- **`MultiLineEdit`** holds chars, wraps with the shared wrapper, enforces character and line limits by snapshot and revert, and serves books and multiline dialog inputs.
- **Editor packets as one `EditorPacket` action**, mapped to serverbound packets in the app, keeping `mcr-ui` free of connection code.
- **Block editors open client-side**, like vanilla: the client raises an event on right-click when the player is a creative operator; the app reads the block entity NBT the client stores (update packet first, chunk copy second) and builds the screen.
- **Dialogs travel over a channel** (`DialogLink`/`DialogEnd`, like resource packs) so configuration-time dialogs reach the shell while the connect thread runs. The client resolves registry ids and tags to inline compounds one level deep before forwarding; the app parses with `Dialog::from_nbt`, which rejects whole dialogs over any limit.
- **Dialog replies** (`custom_click_action`, commands, nested dialogs, disconnect) go back over the same channel and are sent in whichever state the client is in; commands are dropped outside play.
- **Written book layout** is shared by the viewer and the lectern container screen so both support click events.

## Risks / Trade-offs

- A glyph first seen at input time may overflow by one frame; the render-time check removes it.
- Dialog item bodies draw at 16x16 centered even for larger sizes and ignore item components.
- Static `run_command` actions in a dialog are sent immediately like vanilla, without a confirmation, because the server defined the dialog and the player pressed its button; commands inside book and chat text still ask first.
