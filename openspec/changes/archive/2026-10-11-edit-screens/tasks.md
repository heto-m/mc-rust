# Tasks

## 1. Widgets and text editing

- [x] 1.1 Multi-line edit box with wrapping, selection, clipboard, character and line limits, tests
- [x] 1.2 Scaled sprite region, sign and hanging sign textures in the screen assets, cycle button label lookup

## 2. Sign and book screens

- [x] 2.1 Sign editor with width limits and `SubmitSign`, unit and render tests
- [x] 2.2 Book editor and signing, written book viewer with click events, shared page layout, lectern clicks

## 3. Operator block editors

- [x] 3.1 Command block, minecart, structure, jigsaw, test block and test instance screens with tests
- [x] 3.2 Client events for opening editors, test instance status, `ClickEvent::from_nbt`

## 4. Server dialogs

- [x] 4.1 Dialog model and bounded parser, command templates, tests
- [x] 4.2 Dialog screen, waiting screen, custom screen warning
- [x] 4.3 Dialog channel in the client, registry resolution, config and play handling, tests
- [x] 4.4 Shell and play wiring for requests, replies, clear and replace

## 5. App wiring and verification

- [x] 5.1 Build screens from client state and send editor packets in the app
- [x] 5.2 Offscreen render tests for every screen
- [x] 5.3 Live checks against a vanilla server: sign text, book signing, command block run, structure, jigsaw and test block editors, dialogs with input substitution, waiting and clear
- [x] 5.4 Format, clippy, tests and spec validation
