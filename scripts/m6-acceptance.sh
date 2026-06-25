#!/usr/bin/env bash
# Milestone-6 acceptance harness (#656, ultimate-test-plan Phase 6 §10).
# One command: drive cmd/game (the windowed shell, public-API-only) through a full
# AI-vs-AI match and capture a per-phase evidence bundle — screenshot + state JSON
# + StateHash at every checkpoint — under artifacts/m6-acceptance/. The agent then
# MANUALLY reads the PNGs and verifies each against its paired state line (the FSV;
# this script only GATHERS evidence, it does not adjudicate it).
#
# Source is the firstclash world DIRECTORY. The "shipped .litdworld" archive source
# (#648) is blocked on the require-vs-archive sandbox decision (#664); when that
# lands, swap `-world worlds/firstclash` for `-archive worlds/firstclash.litdworld`
# — no other change.
#
# Requires a GL context (DISPLAY). Under headless CI this step is display-gated.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

OUT="artifacts/m6-acceptance"
WORLD="worlds/firstclash"
SEED_A=4242
SEED_B=1337
GAME="bin/m6game"

rm -rf "$OUT"; mkdir -p "$OUT"
echo "m6-acceptance: building cmd/game ..."
go build -o "$GAME" ./cmd/game

# --- Gate 1: import scan (#212 exit 4) — the slice must use only public seams,
#     never the deterministic-core internals (litd/sim) or other private packages.
echo "m6-acceptance: import scan (no non-public litd/ internals) ..."
BAD="$(go list -f '{{range .Deps}}{{println .}}{{end}}' ./cmd/game | grep -E 'litd/sim$|litd/sim/|litd/luabind$' || true)"
# cmd/game may pull litd/sim transitively through litd/api/worldhost; the rule is
# about DIRECT imports in the slice code. Check direct imports only.
DIRECT_BAD="$(go list -f '{{range .Imports}}{{println .}}{{end}}' ./cmd/game | grep -E 'litd/sim$|litd/sim/|litd/luabind$' || true)"
if [ -n "$DIRECT_BAD" ]; then
  echo "m6-acceptance: FAIL — cmd/game directly imports non-public internals:" >&2
  echo "$DIRECT_BAD" >&2
  exit 4
fi
go list -f '{{range .Imports}}{{println .}}{{end}}' ./cmd/game | grep 'litd/' | sort > "$OUT/import-scan.txt"
echo "m6-acceptance: import scan OK (direct litd/* imports → $OUT/import-scan.txt)"

# capture <seed> <localSlot> <exitTick> <label>
capture() {
  local seed="$1" local_slot="$2" tick="$3" label="$4"
  "$GAME" -world "$WORLD" -seed "$seed" -local "$local_slot" -maxspeed -speed 1000 \
    -exit-at "$tick" -out "$OUT" -exit-shot "$label.png" 2>/dev/null \
    | grep '"tag":"exit-at"' | sed "s/^state: //" > "$OUT/$label.json"
  echo "  captured $label: $(grep -o '"tick":[0-9]*\|"phase":"[^"]*"\|"flowResult":"[^"]*"\|"hash":"[^"]*"' "$OUT/$label.json" | tr '\n' ' ')"
}

echo "m6-acceptance: seed A=$SEED_A phase evidence ..."
capture "$SEED_A" 0 600   "A-play"        # mid-play: AI active, no result yet
capture "$SEED_A" 0 24000 "A-terminal-win"  # terminal from slot 0's view (victory)
capture "$SEED_A" 1 24000 "A-terminal-lose" # terminal from slot 1's view (defeat)

echo "m6-acceptance: save/load phase (autotest hash-equality) ..."
"$GAME" -world "$WORLD" -seed "$SEED_A" -autotest -out "$OUT" 2>/dev/null \
  | grep -E '^(FSV save|FSV load|autotest):' > "$OUT/saveload.txt" || true
cat "$OUT/saveload.txt" | sed 's/^/  /'

echo "m6-acceptance: seed B=$SEED_B (independent trace) ..."
capture "$SEED_B" 0 600   "B-play"
capture "$SEED_B" 0 24000 "B-terminal-win"

echo "m6-acceptance: bundle written to $OUT/"
ls -1 "$OUT"
echo "m6-acceptance: GATHERED — now manually read the PNGs and verify each vs its .json (FSV)."
