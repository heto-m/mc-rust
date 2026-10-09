#!/usr/bin/env python3
"""Scans decompiled renderer sources (reference only) for per-entity-type static render info.

Usage: scan_renderers.py <decompiled src dir> <out json>
Anything chosen at runtime (variants, baby models, per-state textures) is not derivable here and is
flagged with dynamic_texture.
"""
import json
import re
import sys
from pathlib import Path

src = Path(sys.argv[1]) / "net/minecraft/client/renderer/entity"
out = Path(sys.argv[2])
fields = json.loads((out.parent / "model_layer_fields.json").read_text())

by_name = {}
for p in src.rglob("*.java"):
    by_name.setdefault(p.stem, p)

reg_text = (src / "EntityRenderers.java").read_text()
regs = re.findall(r"register\(\s*EntityTypes\.(\w+),(.*?)\);\n(?=\s*(?:register\(|\}))", reg_text, re.S)

TEXTURE = re.compile(r'"(textures/[^"]*)"')
LAYER = re.compile(r"ModelLayers\.(\w+)")
SUPER = re.compile(r"\bsuper\(((?:[^;]|\n)*?)\);")
TRAILING_FLOAT = re.compile(r"([-\d.]+)F\s*$")
SHADOW_FIELD = re.compile(r"shadowRadius\s*=\s*([-\d.]+)F")
EXTENDS = re.compile(r"class\s+\w+(?:<[^{]*?>)?\s+extends\s+(\w+)")
FLOAT_ARG = re.compile(r"(?<![\w.])(\d+(?:\.\d+)?)F\b")


def chain(name):
    seen = []
    while name in by_name and name not in seen:
        seen.append(name)
        m = EXTENDS.search(by_name[name].read_text())
        if not m:
            break
        name = m.group(1)
    return seen


def renderer_class(body):
    m = re.search(r"new\s+([\w.]+)\s*(?:<[^()]*>)?\s*\(", body) or re.search(r"([\w.]+)::new", body)
    return m.group(1).split(".")[-1] if m else None


def uniq(items):
    return list(dict.fromkeys(items))


result = {}
for etype, body in regs:
    cls = renderer_class(body)
    classes = chain(cls) if cls else []
    texts = [by_name[c].read_text() for c in classes]
    textures = uniq(TEXTURE.findall(body) + [t for s in texts for t in TEXTURE.findall(s)])
    layers = uniq(LAYER.findall(body) + [l for s in texts for l in LAYER.findall(s)])
    shadow = None
    for s in texts:
        m = SHADOW_FIELD.search(s)
        if m:
            shadow = float(m.group(1))
            break
        for sm in SUPER.finditer(s):
            fm = TRAILING_FLOAT.search(sm.group(1).strip())
            if fm and "context" in sm.group(1):
                shadow = float(fm.group(1))
                break
        if shadow is not None:
            break
    result["minecraft:" + etype.lower()] = {
        "renderer": cls,
        "textures": [t for t in textures if t.endswith(".png")],
        "model_layers": [fields[l] for l in layers if l in fields],
        "model_layer_fields": layers,
        "shadow_radius": shadow,
        "ctor_floats": [float(f) for f in FLOAT_ARG.findall(body)],
        "dynamic_texture": not any(t.endswith(".png") for t in textures) or any(not t.endswith(".png") for t in textures),
    }

out.write_text(json.dumps(dict(sorted(result.items())), indent=1))
print(f"{len(result)} entity renderers")
