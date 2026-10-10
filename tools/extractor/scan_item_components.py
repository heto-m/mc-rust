#!/usr/bin/env python3
"""Collects per-item default components the entity renderer needs from the data generator reports.

Usage: scan_item_components.py <generated reports dir> <out json>
Output maps item name to {slot, asset_id, glider} for every item with an equippable component, which
vanilla clients know by default and servers therefore never send in the stack's component patch.
"""
import json
import sys
from pathlib import Path

reports = Path(sys.argv[1]) / "minecraft/components/item"
out = Path(sys.argv[2])

result = {}
for path in sorted(reports.glob("*.json")):
    components = json.loads(path.read_text()).get("components", {})
    equippable = components.get("minecraft:equippable")
    if equippable is None:
        continue
    result[f"minecraft:{path.stem}"] = {
        "slot": equippable.get("slot"),
        "asset_id": equippable.get("asset_id"),
        "glider": "minecraft:glider" in components,
    }

out.write_text(json.dumps(result, indent=1, sort_keys=True))
print(f"{len(result)} equippable items -> {out}")
