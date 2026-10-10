# Proposal

## Why

`mcr-ui` draws proof screens only. A playable client needs interactive screens: inventories and every container type, chat with command suggestions, and the pause, death, disconnect, connect, title and server-list screens. The app must be able to drive them without `mcr-ui` knowing about winit, the game state crate or the network.

## What Changes

- Widget toolkit: button, edit box (cursor, selection, clipboard trait, history), slider, cycle button, scrollable list, vanilla tooltip, focus and keyboard navigation, and a winit-independent `UiInput` event type.
- `Screen` trait, `ScreenStack` and `ScreenAction` outputs that the app executes (`SendChat`, `ContainerClick`, `CloseContainer`, `Respawn`, `Disconnect`, `Connect`, ...).
- Container screens for every 26.3 `MenuType` plus horse, llama, camel and nautilus inventories and the lectern book, with vanilla slot positions and the exact vanilla click semantics (pickup, quick move, swap, clone, throw, quick craft drag, pickup all).
- Chat screen with history, a `CommandSuggester` trait driving a suggestion popup, scrolling chat HUD and click events on chat components.
- Pause, death, disconnected, connecting, title, multiplayer server list (stored through a trait) and direct connect screens.

Out of scope: options screens beyond the `OpenOptions` action, advancements, recipe book, creative inventory, the 3D player model and enchanting book, banner and map previews, panorama rendering, sound playback (a click hook is exposed).

## Capabilities

### New Capabilities
- `ui-widgets`: input events, focus, buttons, edit box, slider, cycle button, list, tooltip, screen stack and actions.
- `ui-container-screens`: container layouts, click semantics and rendering for all menu types.
- `ui-menu-screens`: chat screen and the non-container menu screens.

### Modified Capabilities

## Impact

Only `crates/mcr-ui`. Adds a small public API of view traits (`ContainerView`, `GameView`, `Clipboard`, `CommandSuggester`, `ServerStore`) that the app adapts to its game state. No new dependencies.
