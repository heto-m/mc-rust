## Why

The first differential harness (font, 38.5% of 3000 cases) and the Horizon benchmark found client gaps
that the earlier audits missed. `vanilla-parity-7` also left three items. This change collects them.
Later harness findings get added here batch by batch.

## What Changes

Each item is recorded in `design.md` with vanilla's behaviour, our code location and a severity:

1. Text wrapping and legacy codes, until the font harness passes 100%.
2. The title screen panorama and the window size flag.
3. The parity-7 leftovers.
4. Text components, until the text harness passes 100%.
5. Section meshing, until the section_mesh harness passes 100%.

## Impact

- Affected specs: `ui-text`, `app-screens`, `chunk-meshing`.
- Code: `mcr-ui` (text, chat), `mcr-app` (title screen, window), `mcr-render`. No protocol changes.
