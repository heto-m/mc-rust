#!/usr/bin/env bash
# Downloads the non-native client libraries listed in version.json into $cache/libraries.
set -euo pipefail
cache="$1"
ver="$2"
dest="$cache/libraries"
mkdir -p "$dest"
python3 - "$cache/versions/$ver/version.json" "$dest" <<'PY'
import json, os, sys, urllib.request

vj, dest = sys.argv[1:]
for lib in json.load(open(vj))["libraries"]:
    art = lib.get("downloads", {}).get("artifact")
    if not art or "natives" in lib["name"].split(":")[-1]:
        continue
    path = os.path.join(dest, os.path.basename(art["path"]))
    if not os.path.exists(path):
        urllib.request.urlretrieve(art["url"], path)
PY
