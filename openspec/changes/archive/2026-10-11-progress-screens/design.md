# Design

## Recipe book

Client state keeps entries keyed by display id with a revision counter so the UI re-filters only on change. Each entry has a weight (display nodes plus listed ids). Entries over `MAX_RECIPE_WEIGHT` are dropped and the whole book is capped at `MAX_RECIPE_BOOK_WEIGHT`, evicting old entries, so a hostile server cannot hold gigabytes across many packets. Slot displays resolve with a depth limit, a node budget and an output cap.

## Advancements

Tabs come from root advancements with a background, in the server's order, up to 18. Parent chains are walked with a depth limit, so cycles end. Positions are clamped to a finite range, nodes per tab are capped and tooltip text is truncated before wrapping.

## Statistics

Stats come from `AwardStats` into a bounded map. The screen requests them with `ClientCommand` on open and lays out the three vanilla tabs.
