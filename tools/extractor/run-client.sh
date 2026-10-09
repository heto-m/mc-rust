#!/usr/bin/env bash
# Usage: run-client.sh [version] [out_dir]; dumps entity_models.json, animations.json and entity_renderers.json.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
cache="${MCR_CACHE:-}"
if [ -z "$cache" ]; then
  cache="$repo/.cache"
  [ -d "$cache" ] || cache="$(cd "$repo/../../../.cache" && pwd)"
fi
ver="${1:-26.3}"
jdk="$cache/tools/jdk25/bin"
vdir="$cache/versions/$ver"
out="${2:-$vdir/extracted}"
build="$here/build-client"

"$here/fetch-client-libs.sh" "$cache" "$ver"
cp="$vdir/client.jar:$(find "$cache/libraries" "$vdir/datagen/libraries" -name '*.jar' | paste -sd:)"
mkdir -p "$build"
"$jdk/javac" -nowarn -cp "$cp" -d "$build" "$here/client/EntityModelExtractor.java"
"$jdk/java" -cp "$build:$cp" EntityModelExtractor "$out"
python3 "$here/scan_renderers.py" "$vdir/src" "$out/entity_renderers.json"
