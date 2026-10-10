# Proposal

## Why

In creative mode the inventory key opened the survival inventory, so players had no way to take items from the vanilla creative tabs, search them, or save hotbars.

## What Changes

- A creative inventory screen with the vanilla tabs, scrolling, name and `#tag` search, destroy slot, survival inventory tab and saved-hotbar tab, opened by the inventory key in creative mode.
- Tab contents come from an extractor that runs the game's own `CreativeModeTabs` against the data-pack registries, written to `creative_tabs.txt` and filtered at runtime by the server's enabled features and operator permission.
- Clicks follow vanilla: the cursor is client-side, inventory changes are sent as `set_creative_mode_slot`, drops as slot -1; the packet now uses the length-prefixed untrusted stack encoding.
- Saved hotbars persist in `hotbar.txt`, saved and loaded with the activator keys and a hotbar number key.

## Capabilities

### New Capabilities
- `creative-inventory`: tab data, screen behaviour and the creative slot protocol.

## Impact

`crates/mcr-ui` (new `gui/creative`), `crates/mcr-data` (tab data), `crates/mcr-client` (creative actions, permission level), `crates/mcr-protocol` (`set_creative_mode_slot`), `crates/mcr-app` (wiring), `tools/extractor`.
