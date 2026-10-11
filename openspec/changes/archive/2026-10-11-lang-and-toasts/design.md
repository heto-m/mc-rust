## Context

Vanilla stacks `lang/<code>.json` from every pack per namespace, loads `en_us` first and the selected language on top, then applies `deprecated.json`. The language list lives in `pack.mcmeta`.

## Decisions

- `ResourceManager::read_stack` returns the file from every pack in order so languages merge per key instead of per file.
- The launcher asset index supplies the non-English files and `pack.mcmeta`; the jar supplies `en_us` and `deprecated.json`.
- The language screen only edits `options.lang`; the shell and session reload the `Lang` when it differs from the loaded code.
- Toasts live in `mcr-ui` as a pure state machine driven by an explicit clock so timing is unit-testable; the app renders it above all screens and plays the in/out sounds on visibility changes.
- The client emits `ClientEvent::Toast` for newly completed advancements with `show_toast` and for recipe entries flagged for notification.
- Right-to-left languages render in logical order; bidi shaping is not implemented.

## Risks

Chat lines already laid out keep the language they were received in.
