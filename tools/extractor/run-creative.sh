#!/usr/bin/env bash
# Usage: run-creative.sh [version] [out_dir]; dumps creative_tabs.txt. MCR_CACHE overrides the .cache location.
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
dg="$cache/versions/$ver/datagen"
out="${2:-$cache/versions/$ver/extracted}"
build="$here/build"

cp="$(find "$dg/versions" "$dg/libraries" -name '*.jar' | paste -sd:)"
mkdir -p "$build"
"$jdk/javac" -nowarn -cp "$cp" -d "$build" "$here/src/CreativeExtractor.java"
cd "$build" && "$jdk/java" -cp ".:$cp" CreativeExtractor "$out" "$cache/versions/$ver/generated"
