## Context

`mcr-ui` has a batched immediate-mode `UiFrame` (fills, sprites, text, scissor), item icons (`ItemIcons`, `StackView`) and HUD/chat proof screens. The game-state crate is being written in parallel, so screens must not depend on it.

## Decisions

- **Everything in GUI pixels.** `UiInput` carries GUI-scaled coordinates; the app divides physical positions by `UiFrame::scale()`. Double clicks are detected by `ScreenStack` from `Env::now_ms` (250 ms, same button).
- **View traits instead of state.** Screens read live data through `GameView` (slot stacks as `&ItemStack`, carried stack, container data ints, creative flag, merchant offers, stonecutter options, book pages). The app implements it over its game state; tests implement it over a `Vec`. Other app services are traits too: `Clipboard`, `CommandSuggester`, `ServerStore`.
- **Actions out, never effects.** `Screen::input` pushes `ScreenAction`s. Navigation (`Push`, `Replace`, `Pop`) is handled by `ScreenStack`; everything else is returned to the app. `ContainerClick` carries only `(slot, button, ClickType)`; the app computes the changed-slot hashes it sends.
- **Click state machine separate from rendering.** `ClickMachine` owns hover, drag, double-click and skip-release state and is pure over layout plus `GameView`, so the click tables are unit tested without fonts or a GPU. Mouse buttons are left=0, right=1, middle=2; the drag header/type mask follows vanilla (`header | type << 2`). Vanilla's drag set is a hash set; an insertion-ordered list is used instead. Press with a carried stack starts a drag and the pickup is sent on release, exactly like vanilla.
- **Layout is data.** `ContainerLayout::new(kind)` returns slot positions, size, texture and label positions in vanilla order (`MenuKind::from_registry_id` maps the `open_screen` id); per-menu overlays (furnace flame, brewing bubbles, enchanting options, ...) are drawn by a `match` on the kind. Mount inventories take chest columns and equipment slots from the app because those come from the entity, not the menu type.
- **Container textures are runtime sprites.** `ScreenAssets::load` decodes `textures/gui/container/*.png`, title and menu images and mob effect icons from the resource manager into the sprite atlas with `Sprites::add_raw` (oversized images are box-filtered to fit a page); draws use `sprite_region` windows so the 256x256 sheets are blitted 1:1.
- **Widgets own their state, draw through `UiFrame`.** `WidgetSet` holds buttons, sliders, cycle buttons and edit boxes with shared focus (Tab, Shift+Tab, Up and Down, Enter and Space). Labels written `lang:key` are resolved when drawn so screens can be built without the language file. Screens lay out buttons at render time when sizes depend on text.
- **Edit box.** Cursor and selection are in characters; width-dependent scrolling and click placement use glyph widths cached at the last render. Vanilla's chat character filter applies to typing and pasting.
- **Chat.** `ChatHud` is shared (`Rc<RefCell<_>>`) between the app and `ChatScreen`; it keeps scroll position, recent messages and per-line click spans (`flatten_events`, bounded like text expansion, innermost span wins). Run-command clicks are sent directly like vanilla; open-URL clicks go through a confirmation screen and only http and https are accepted.
- **Hostile input.** Server-provided counts (offers, stonecutter options, loom patterns, suggestions, text lines) are clamped before loops; arithmetic on server ints is saturating or widened; tooltip lines and suggestion texts are truncated.

## Known gaps

- 3D previews (player model, horse, armor stand, enchanting book, banner, map contents) are exposed through `Screen::entity_rect` or left empty.
- Loom pattern buttons show no banner pattern preview; enchantment option text is pseudo-random galactic words, not vanilla's seeded sequence.
- Server icons are a placeholder square; the title screen has no panorama or splash text.
- No recipe book, creative inventory, bundle tooltips, hover events in chat, signed-chat drafts or advanced item tooltips beyond what `GameView::item_tooltip` supplies.
- Options screens are not implemented; `OpenOptions` is only an action.
