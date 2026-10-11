# Proposal

## Why

The client joins vanilla servers but cannot edit a sign, write a book, set a command block or answer a server dialog. These screens are part of the full-functionality milestone, and servers use dialogs to ask players for input in configuration and play.

## What Changes

- Sign editor for standing, wall and hanging signs, front and back, with the sign texture preview, vanilla line widths measured by the font, cursor and selection; closing sends `sign_update`.
- Book and quill editor with pages, page turning, character and line limits, and sign and finalize with a title; written book viewer for `open_book` and lectern books whose links, commands and page references go through the confirm screens.
- Command block, command block minecart, structure block, jigsaw block, test block and test instance block editors, opened by right-clicking as a creative operator, each sending its serverbound packet.
- Server dialogs (`show_dialog` and `clear_dialog` in configuration and play): notice, confirmation, multi action, dialog list and server links, with text and item bodies, text, boolean, single option and number range inputs, command template and custom actions, the waiting-for-response screen and the custom screen warning.

Out of scope: command suggestions in command block fields, rendering sign models in 3D, item components in dialog item bodies, text filtering and IME preedit.

## Capabilities

### New Capabilities

- `sign-book-editors`: sign editing, book editing and signing, written book viewing.
- `operator-block-editors`: command block, minecart, structure, jigsaw, test block and test instance editors.
- `server-dialogs`: parsing, rendering and answering server dialogs in configuration and play.

### Modified Capabilities

- `client-state`: block editor and minecart editor open events, test instance status, dialog channel to the app.

## Impact

`mcr-ui` gains the screens and a multi-line edit box; `mcr-client` gains the dialog channel and editor events; `mcr-app` builds the screens from client state and routes their actions. `mcr-protocol` exposes `ClickEvent::from_nbt`.
