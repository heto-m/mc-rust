## Why

The client only knows `en_us` and hard-codes English in several places. Players expect the language option, translated UI, and the toast notifications for advancements and recipes.

## What Changes

- Load every language from the asset index and resource packs, merged like vanilla with `en_us` fallback and the renamed/removed key table.
- Language selection screen reachable from Options; `lang` in `options.txt` applies live.
- Remove the hard-coded chat pattern table; chat types translate through the language files.
- Toast manager (top-right, slide animation, five slots, queue) with advancement, recipe and system toasts and the `ui.toast.*` sounds.

## Capabilities

### New Capabilities
- `languages`: language discovery, merge, fallback and live switching.
- `toasts`: toast queue, timing and the advancement/recipe/system toast kinds.

### Modified Capabilities

## Impact

`mcr-assets` (resource stacks), `mcr-ui` (lang, toasts), `mcr-client` (toast events), `mcr-app` (options screen, wiring).
