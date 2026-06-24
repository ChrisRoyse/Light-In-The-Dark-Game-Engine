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
# Plus the locally-measurable halves of the no-CI-framed gate issues (the render/
# asset/multi-OS halves stay deferred on their own issues, never proxied here):
#   - #310: firstlight binary-size ceiling
#   - #210: headless .litdreplay record -> verify file round-trip determinism
#
# Fail-closed: the first failing gate aborts the run with a nonzero exit. Run
# this and see "PREFLIGHT: ALL GATES GREEN" before merging any branch to main.
#
# Usage:
#   scripts/preflight.sh            # full gate (default) — run before merge
#   scripts/preflight.sh --fast     # quick inner-loop pass: runs the unit suite
#                                    # with `go test -short` (heavy 10k/5k-tick
#                                    # e2e + save/load + stress tests self-skip),
#                                    # runs core 10k sim + AI golden subtest
#                                    # once as explicit steps, and skips the
#                                    # heavier AI save/restore + GOMAXPROCS edge
#                                    # sweep, -race/bench/world-archive gates.
#   scripts/preflight.sh --no-race  # full gate but skip the -race determinism cell
#
# Inner-loop tip: for a single package, `go test -short ./litd/<pkg>/` skips the
# long e2e/determinism fixtures (gated by testing.Short()) — seconds, not minutes.
# The FULL gate (no -short) always runs them before a main merge. Wall-clock
# budget tests are isolated into explicit preflight steps instead of normal
# package-parallel `go test ./...`.
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
if [ $FAST -eq 1 ]; then
  echo "FAST mode: skipped go build (go test -short compiles all packages; FULL gate still runs go build)."
  echo
else
  step "go build"             go build ./...
fi
# FAST runs the unit suite with -short: the heavy e2e/determinism variants
# (10k/5k-tick golden + GOMAXPROCS + save/load stress + table-bomb) self-skip via
# testing.Short(), cutting the inner-loop suite ~2x. Quality is preserved: the
# FULL gate (main-push) runs without -short, AND the core 10k sim/AI determinism
# is a SEPARATE explicit step below that runs in BOTH modes (never -short'd).
if [ $FAST -eq 1 ]; then
    step "go test (-short)"     go test -short ./...
else
    step "go test"              go test ./...
fi

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
# PRD2 sim-primitive acceptance goldens + save/resume parity (#559 timers,
# #567 unit-groups). Fast and always run so a cross-platform hash divergence
# in a primitive is caught even in --fast.
step "PRD2 primitive acceptance (timers/groups)" go test ./litd/sim/ -run 'TestTimerScenarioGolden|TestGroupScenarioGolden|TestGroupSaveResumeParity|TestGroupTwoRunDeterminism' -count=1
step "import-graph (sim ⊥ render/G3N/GL)" go run ./tools/importcheck
step "determinism lint"     go run ./tools/determlint ./litd/...
step "API-surface lint (R-API-1..6)" go run ./tools/apilint ./litd/api
step "presentation-trigger lint (#449/#471)" go run ./tools/presentlint litd/render litd/audio
step "save-unsafe-timer lint (#557, R-TMR-8)" go run ./tools/timerlint abilities
step "license-scan"         ./scripts/license-scan.sh

# --- jassgen audit (jassgen-audit.yml) --------------------------------------
step "jassgen build"        go build -o /dev/null ./tools/jassgen/
step "jassgen -emit (schema gate)"        go run ./tools/jassgen -emit
step "jassgen -check (reproducibility)"   go run ./tools/jassgen -check
step "jassgen -audit (M2 dedup gate)"     go run ./tools/jassgen -audit
step "jassgen -revclosure"                go run ./tools/jassgen -revclosure
step "jassgen -eventcov (EVENT_ coverage)" go run ./tools/jassgen -eventcov
step "jassgen tests"        go test ./tools/jassgen/

# Generated-artifact drift guard. The -emit/-audit/-eventcov steps above REGENERATE
# these tracked files IN PLACE, so a prior commit that changed the API without
# regenerating them leaves them dirty vs HEAD now. `jassgen -check` alone is masked
# when a local -audit run already overwrote the on-disk file (it then compares disk
# vs regenerated, never the committed blob) — which is exactly how audit-report.json
# reached main stale at 585 exported verbs while the real surface was 588. Comparing
# the post-regen working tree against HEAD is immune to that: a stale commit goes red.
step "generated artifacts in sync" git diff --exit-code HEAD -- api-manifest.json audit-report.json audit-report.md docs/api/event-coverage.json

# Public Lua API reference (#187): the committed docs/api/lua-reference.md must
# stay in sync with api-manifest.json. This is the no-CI equivalent of "drift
# between manifest and published docs fails the build".
step "lua-api-doc drift (#187)" go run ./tools/luadoc -check

# --- determinism traces (determinism.yml) -----------------------------------
# The 10k-tick sim+AI fixtures self-skip under -short. In FAST mode the main
# `go test -short` step above skipped them, so run the core golden-hash gates
# explicitly here. The heavier AI save/restore and GOMAXPROCS fixtures stay
# full-gate only; they expand to several extra 10k-equivalent matches and are
# already covered by the main no-short `go test ./...` step before main merges.
# In FULL mode the main step already ran the non-race cell and the block below
# adds the -race cell.
if [ $FAST -eq 1 ]; then
  step "10k-tick sim determinism" go test ./litd/sim/ -run 'TestDeterminism10k$'
  step "10k-tick AI determinism (golden)" go test ./litd/ai/ -run '^TestAIDeterminism10k$/^Golden$'
fi

if [ $FAST -eq 0 ]; then
  if [ $RACE -eq 1 ]; then
    step "10k-tick sim determinism (-race)" go test ./litd/sim/ -run 'TestDeterminism10k$' -race
  fi

  # Wall-clock worst-tick gates are sensitive to go test's package-level
  # parallelism, so run this gate alone and require explicit opt-in in the test.
  step "battle-500 tick budget (isolated)" env LITD_TICK_GATE=on GOMAXPROCS=1 go test ./litd/sim/bench -run '^TestBattle500TickBudget$' -count=1 -p 1

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

  # --- local resource + replay gates (reframed from the no-CI #210/#237/#310) --
  # There is no CI (operator decision, permanent); the gates those issues asked
  # for live here, in the local preflight, for the parts that are honestly
  # measurable headlessly. The render/asset/multi-OS halves stay deferred on
  # their own issues — see each issue body. We do NOT dress a headless proxy up
  # as a whole-game measurement.
  #
  # #310 (binary+assets size budget): tools/sizecheck sums the firstlight binary
  # size + the per-category asset bytes declared in assets/MANIFEST (the Bytes
  # field, #539 — so the gitignored asset files need not be present) and gates the
  # total against binary_assets_bytes_max in budgets.toml (300 MB, the §5.3
  # ceiling). The single budget literal lives ONLY in budgets.toml. Asset Bytes
  # are largely unspecified today, so the number is binary-dominated (~8 MiB) and
  # tightens automatically as MANIFEST entries gain Bytes. Cold-start/map-load
  # timing stay gated on render + the firstflame archive (#209).
  step "size budget binary+assets (#310)" bash -c '
    set -e
    out="$(mktemp -d)/firstlight"
    go build -o "$out" ./cmd/firstlight
    go run ./tools/sizecheck -bin "$out"'

  # #210 (full-match replay verified headlessly): the determinism MECHANISM —
  # record a command stream to a versioned .litdreplay, then re-simulate and
  # compare the full checkpoint trace, fail-closed on any divergence. This gates
  # the replay-artifact round-trip (file write -> re-sim -> trace), distinct from
  # the in-process TestDeterminism10k cells above. The "real First Flame match
  # over 3 OSes" CI matrix is impossible under the no-CI policy on one platform;
  # that part stays deferred on #210.
  step "headless replay record+verify (#210: file round-trip determinism)" bash -c '
    set -e
    d=$(mktemp -d)
    go build -o "$d/headless" ./cmd/headless
    printf "%s\n" "10 0 0 40 40" "200 0 1 80 80" "500 0 2 60 20" > "$d/cmds.txt"
    "$d/headless" -ticks 2000 -units 64 -seed 7 -cmds "$d/cmds.txt" -replay "$d/m.litdreplay" >/dev/null
    out=$("$d/headless" -verify "$d/m.litdreplay")
    echo "$out" | tail -1
    echo "$out" | grep -q "verify: OK" || { echo "replay verify FAILED (determinism regression)" >&2; exit 1; }'
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
