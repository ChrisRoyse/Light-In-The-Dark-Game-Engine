#!/usr/bin/env bash
# pack-world.sh — the make-level world-archive producer (#209 deliverable;
# #205 "packaging step (make-level)"). It stages a skirmish map's data, a
# world's own data tables, and Lua into one tree and packs a deterministic
# `.litdworld` via the worldpack producer, then validates the result with
# `assetcheck archive`.
#
# Fail-closed posture (R-FMT-2, doctrine §2.4): the archive is packaged ONLY
# from inputs that already pass assetcheck (the map is re-validated in `data`
# mode before staging), and the produced archive must pass `assetcheck archive`
# with zero findings — any failure aborts and removes the partial output. No
# archive is left behind that has not been validated.
#
# Usage:
#   scripts/pack-world.sh <map-dir> <world-dir> <out.litdworld> \
#       <engine-range> <author> <title> <description>
#
# Example (the First Flame slice):
#   scripts/pack-world.sh data/maps/firstflame worlds/firstflame \
#       worlds/firstflame.litdworld ">=0.1.0 <0.2.0" \
#       "Light in the Dark" "First Flame" "Two-player beacon duel on the ashen veil"
#
# Example (a MAPLESS world — pass "-" for the map dir; First Clash, #648):
#   scripts/pack-world.sh - worlds/firstclash \
#       worlds/firstclash.litdworld ">=0.1.0 <0.2.0" \
#       "Light in the Dark" "First Clash" "Two-CPU melee that always decides"
#
# World data lands under `data/`, map data under `data/maps/<name>/`, and world
# Lua under `scripts/` — the layout the in-engine loader mounts. Map name is
# taken from the basename of <map-dir>.
set -euo pipefail

if [ "$#" -ne 7 ]; then
  echo "usage: $0 <map-dir> <world-dir> <out.litdworld> <engine-range> <author> <title> <description>" >&2
  exit 2
fi

MAP_DIR="$1"
WORLD_DIR="$2"
OUT="$3"
ENGINE_RANGE="$4"
AUTHOR="$5"
TITLE="$6"
DESCRIPTION="$7"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# A mapless world (e.g. firstclash, the AI-vs-AI melee dogfood) passes "-" as the
# map dir: it ships its own data tables + scripts and synthesizes its battlefield
# from match.toml at load, so there is no skirmish map to stage (#648).
MAPLESS=false
if [ "$MAP_DIR" = "-" ] || [ "$MAP_DIR" = "none" ]; then
  MAPLESS=true
else
  [ -d "$MAP_DIR" ] || { echo "pack-world: map dir $MAP_DIR not found" >&2; exit 2; }
fi
[ -d "$WORLD_DIR" ] || { echo "pack-world: world dir $WORLD_DIR not found" >&2; exit 2; }

# 1. Re-validate the map data in `data` mode — packaging only from
#    assetcheck-passing inputs (#209 constraint). Fail loud on any finding.
if [ "$MAPLESS" = true ]; then
  echo "pack-world: mapless world ($WORLD_DIR) — no map to validate"
else
  MAP_NAME="$(basename "$MAP_DIR")"
  echo "pack-world: validating map inputs ($MAP_DIR) ..."
  go run ./tools/assetcheck --json "$MAP_DIR" >/tmp/pack-world-mapcheck.json
  if [ "$(cat /tmp/pack-world-mapcheck.json)" != "[]" ]; then
    echo "pack-world: map inputs failed assetcheck — refusing to package:" >&2
    cat /tmp/pack-world-mapcheck.json >&2
    exit 1
  fi
fi

# 2. Stage world data + map data + world Lua into a clean tree.
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/data" "$STAGE/scripts"
if [ -d "$WORLD_DIR/data" ]; then
  cp -R "$WORLD_DIR/data/." "$STAGE/data/"
fi
if [ "$MAPLESS" != true ]; then
  mkdir -p "$STAGE/data/maps/$MAP_NAME"
  cp -R "$MAP_DIR/." "$STAGE/data/maps/$MAP_NAME/"
fi
# World Lua chunks (no stray editor files) plus the match descriptor, which the
# loader reads from the script root (archiveScriptFS mounts scripts/).
find "$WORLD_DIR" -name '*.lua' -type f -print0 | while IFS= read -r -d '' f; do
  rel="${f#"$WORLD_DIR"/}"
  mkdir -p "$STAGE/scripts/$(dirname "$rel")"
  cp "$f" "$STAGE/scripts/$rel"
done
if [ -f "$WORLD_DIR/match.toml" ]; then
  cp "$WORLD_DIR/match.toml" "$STAGE/scripts/match.toml"
fi

# 3. Pack deterministically with real hosting metadata (D-23).
echo "pack-world: packing $OUT ..."
go run ./tools/worldpack pack \
  --engine "$ENGINE_RANGE" \
  --author "$AUTHOR" --title "$TITLE" --description "$DESCRIPTION" \
  "$STAGE" "$OUT"

# 4. Validate the produced archive — zero findings required, else abort + remove.
echo "pack-world: validating archive $OUT ..."
if ! go run ./tools/assetcheck archive "$OUT"; then
  echo "pack-world: produced archive failed validation — removing $OUT" >&2
  rm -f "$OUT"
  exit 1
fi

echo "pack-world: OK — $OUT validated (map=${MAP_NAME:-<none>}, world=$WORLD_DIR)"
