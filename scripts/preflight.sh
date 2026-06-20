#!/usr/bin/env bash
# preflight.sh — local pre-merge gate. Replaces all GitHub Actions workflows
# (removed 2026-06-19: the project runs no CI; account-level Actions billing is
# unresolved per #284 and the policy is now "gate locally before merge").
#
# This script consolidates every check the old workflows ran:
#   - ci.yml          : vet, build, test, assetcheck, zero-alloc, importcheck,
#                       determlint, apilint, license-scan, benchharness
#   - determinism.yml : 10k-tick sim + AI determinism traces (plus -race)
#   - jassgen-audit.yml: manifest emit/check/audit/revclosure + jassgen tests
#   - world-archive.yml: First Flame archive pack + repack reproducibility
#
# Fail-closed: the first failing gate aborts the run with a nonzero exit. Run
# this and see "PREFLIGHT: ALL GATES GREEN" before merging any branch to main.
#
# Usage:
#   scripts/preflight.sh            # full gate (default) — run before merge
#   scripts/preflight.sh --fast     # skip slow gates (bench, 10k determinism,
#                                    # world-archive) for a quick inner-loop pass
#   scripts/preflight.sh --no-race  # full gate but skip the -race determinism cell
set -uo pipefail

cd "$(dirname "$0")/.."
ROOT="$(pwd)"

FAST=0
RACE=1
for arg in "$@"; do
  case "$arg" in
    --fast)    FAST=1 ;;
    --no-race) RACE=0 ;;
    *) echo "unknown arg: $arg" >&2; exit 64 ;;
  esac
done

FAILED=()
GREEN='\033[0;32m'; RED='\033[0;31m'; DIM='\033[2m'; NC='\033[0m'

# step <name> <cmd...> — run a gate, record failure, keep going so the operator
# sees the full list of what's broken in one pass (each gate still fail-closes
# the overall exit code).
step() {
  local name="$1"; shift
  printf "${DIM}── %s${NC}\n" "$name"
  if "$@"; then
    printf "${GREEN}✓ %s${NC}\n\n" "$name"
  else
    printf "${RED}✗ %s (exit %d)${NC}\n\n" "$name" "$?"
    FAILED+=("$name")
  fi
}

# softstep <name> <cmd...> — advisory gate: reports findings but does NOT fail the
# overall run. Use only for checks blocked on tracked debt that hosted CI never
# actually enforced. Promote back to step() when the debt issue closes.
softstep() {
  local name="$1"; shift
  printf "${DIM}── %s (advisory)${NC}\n" "$name"
  if "$@"; then
    printf "${GREEN}✓ %s${NC}\n\n" "$name"
  else
    printf "${RED}! %s reported findings (advisory — not blocking)${NC}\n\n" "$name"
  fi
}

# --- prerequisites -----------------------------------------------------------
if [ ! -d repoes/engine ] || [ ! -d repoes/war3-types ] || [ ! -d repoes/gopher-lua ]; then
  echo "repoes/ incomplete — restoring vendored clones (pinned SHAs + LITD-PATCH)…"
  ./scripts/restore-repoes.sh || { echo "restore-repoes failed" >&2; exit 1; }
fi

echo "preflight: $([ $FAST -eq 1 ] && echo FAST || echo FULL) gate @ $(git rev-parse --short HEAD)"
echo

# --- core gates (ci.yml linux job) ------------------------------------------
step "go vet"                go vet ./...
step "go build"             go build ./...
step "go test"              go test ./...

# assetcheck is ADVISORY until #445 (author the per-asset MANIFEST `category`
# field for 627 entries + decide the over-budget CC0-pack waiver policy) lands:
# the 627 BUDGET-UNCATEGORIZED findings are that tracked debt, and hosted CI never
# enforced this (assets/ is gitignored there → step was a no-op). The load-time
# per-category gate (#424) is done; this stays advisory only for the directory
# MANIFEST data gap. Promote to a hard step() when #445 closes.
softstep "assetcheck (#445 debt)" bash -c '
  if ls assets/*/*.glb >/dev/null 2>&1; then
    go run ./tools/assetcheck ./assets
  else
    echo "NOTICE: no asset binaries in checkout (gitignored). Run assetcheck locally against real assets/ before committing MANIFEST changes."
  fi'

step "zero-alloc (R-GC-1/5)" go test ./litd/sim/ -run TestZeroAlloc -count=1
step "import-graph (sim ⊥ render/G3N/GL)" go run ./tools/importcheck
step "determinism lint"     go run ./tools/determlint ./litd/...
step "API-surface lint (R-API-1..6)" go run ./tools/apilint ./litd/api
step "license-scan"         ./scripts/license-scan.sh

# --- jassgen audit (jassgen-audit.yml) --------------------------------------
step "jassgen build"        go build -o /dev/null ./tools/jassgen/
step "jassgen -emit (schema gate)"        go run ./tools/jassgen -emit
step "jassgen -check (reproducibility)"   go run ./tools/jassgen -check
step "jassgen -audit (M2 dedup gate)"     go run ./tools/jassgen -audit
step "jassgen -revclosure"                go run ./tools/jassgen -revclosure
step "jassgen tests"        go test ./tools/jassgen/

# --- determinism traces (determinism.yml) -----------------------------------
step "10k-tick sim determinism" go test ./litd/sim/ -run 'TestDeterminism10k$'
step "10k-tick AI determinism"  go test ./litd/ai/ -run 'TestAIDeterminism10k$|TestAISaveRestore$|TestAIDeterminismGOMAXPROCS$'

if [ $FAST -eq 0 ]; then
  if [ $RACE -eq 1 ]; then
    step "10k-tick sim determinism (-race)" go test ./litd/sim/ -run 'TestDeterminism10k$' -race
  fi

  # --- bench budgets (ci.yml bench job) -------------------------------------
  step "benchmark budgets (R-GC-5)" go run ./tools/benchharness

  # --- world archive (world-archive.yml) ------------------------------------
  step "world-archive pack + reproducibility" bash -c '
    set -e
    scripts/pack-world.sh data/maps/firstflame worlds/firstflame \
      /tmp/preflight-a.litdworld ">=0.1.0 <0.2.0" \
      "Light in the Dark" "First Flame" \
      "Two-player beacon duel on the ashen veil"
    scripts/pack-world.sh data/maps/firstflame worlds/firstflame \
      /tmp/preflight-b.litdworld ">=0.1.0 <0.2.0" \
      "Light in the Dark" "First Flame" \
      "Two-player beacon duel on the ashen veil"
    if ! cmp -s /tmp/preflight-a.litdworld /tmp/preflight-b.litdworld; then
      echo "world archive not reproducible (byte diff across repacks)" >&2; exit 1
    fi
    echo "repack reproducible: $(sha256sum /tmp/preflight-a.litdworld | cut -d" " -f1)"'
else
  echo "FAST mode: skipped -race, benchharness, world-archive."
  echo
fi

# --- verdict ----------------------------------------------------------------
if [ ${#FAILED[@]} -eq 0 ]; then
  printf "${GREEN}PREFLIGHT: ALL GATES GREEN${NC} — safe to merge.\n"
  exit 0
else
  printf "${RED}PREFLIGHT: %d GATE(S) FAILED${NC} — do not merge:\n" "${#FAILED[@]}"
  printf "  ${RED}✗${NC} %s\n" "${FAILED[@]}"
  exit 1
fi
