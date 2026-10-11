# Proposal

## Why

The client stores the recipe book, advancements and statistics the server sends, but the player cannot see or use them. Vanilla shows them in the recipe book panel, the advancements screen (L) and the statistics screen, and all three are part of full vanilla-server functionality.

## What Changes

- Recipe book panel beside the inventory, crafting table, furnaces and smoker, with tabs, search, the craftable filter, pages, alternatives overlay, ghost recipes, `PlaceRecipe` and seen-recipe and settings packets.
- Slot display resolver that turns recipe slot displays into the stacks they cycle through.
- Advancements screen with tabs on four sides, a scrolling tree with connectors, frames, progress and hover tooltips.
- Statistics screen with general, item and mob tabs, sorting and `ClientCommand` stat requests.
- Bounds on all server-driven sizes: recipe weights, display nesting, node counts, positions and text.

## Impact

`mcr-client` (gamestate, slot_display, edits, handler), `mcr-ui` (recipe_book, advancements, stats, container screens), `mcr-app` (views and key bindings).
