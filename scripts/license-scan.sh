#!/usr/bin/env bash
# License gate (G4.1, D-21): permissive allowlist; copyleft and unknown
# licenses hard-fail. No bypass flags. Run locally or as the CI step:
#   ./scripts/license-scan.sh
#
# Covers: every Go package dependency of ./... (via google/go-licenses,
# which follows the repoes/engine replace directive) + the non-module
# vendored repoes/war3-types tree.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

MODULE=github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine
ALLOWED='^(BSD-2-Clause|BSD-3-Clause|MIT|Apache-2\.0|ISC|Unlicense|CC0-1\.0)$'

# Reviewed dual-license elections. Format: package-prefix|elected|sha256-of-LICENSE.
# The hash pins the exact LICENSE text the election was reviewed against —
# if the dep's LICENSE changes, the gate goes red until re-reviewed.
# freetype: dual "FTL or GPLv2+" (LICENSE offers the choice); we elect FTL
# (BSD-style permissive). Decision record: type:decision issue.
ELECTIONS=(
  "github.com/golang/freetype|FTL (elected from dual FTL|GPLv2+)|d3ba056adc2b7909e95681deaae397fb37c97ed491a920f491214f07b62c41d0"
)

command -v go-licenses >/dev/null || go install github.com/google/go-licenses@v1.6.0
export PATH="$PATH:$(go env GOPATH)/bin"

report="$(mktemp)"
go-licenses report ./... --ignore "$MODULE" > "$report" 2>/tmp/license-scan-warnings.log

echo "=== dependency license inventory ==="
column -s, -t < "$report"
echo
echo "modules in graph: $(go list -m all | wc -l) (incl. main module); report rows: $(wc -l < "$report")"

fail=0
while IFS=, read -r pkg url license; do
  if [[ "$license" =~ $ALLOWED ]]; then
    continue
  fi
  elected=""
  for e in "${ELECTIONS[@]}"; do
    prefix="${e%%|*}"; rest="${e#*|}"
    choice="${rest%|*}"; want_sha="${rest##*|}"
    if [[ "$pkg" == "$prefix" || "$pkg" == "$prefix"/* ]]; then
      modver=$(go list -m -f '{{.Dir}}' "$prefix" 2>/dev/null || true)
      if [[ -n "$modver" && -f "$modver/LICENSE" ]]; then
        got_sha=$(sha256sum "$modver/LICENSE" | cut -d' ' -f1)
        if [[ "$got_sha" == "$want_sha" ]]; then
          elected="$choice"
        else
          echo "FAIL: $pkg: pinned dual-license election stale (LICENSE sha256 $got_sha != reviewed $want_sha)"
          fail=1
          elected="__handled__"
        fi
      fi
      break
    fi
  done
  [[ "$elected" == "__handled__" ]] && continue
  if [[ -n "$elected" ]]; then
    echo "OK (election): $pkg -> $elected"
    continue
  fi
  echo "FAIL: $pkg: license '$license' not in permissive allowlist (copyleft/unknown hard-fail, D-21)"
  fail=1
done < "$report"

# Non-module vendored tree: repoes/war3-types (API basis data, not a Go dep).
if [[ -f repoes/war3-types/LICENSE ]]; then
  if head -1 repoes/war3-types/LICENSE | grep -q "MIT License"; then
    echo "OK: repoes/war3-types LICENSE = MIT"
  else
    echo "FAIL: repoes/war3-types LICENSE not recognized as MIT — review required"
    fail=1
  fi
else
  echo "FAIL: repoes/war3-types/LICENSE missing (run scripts/restore-repoes.sh)"
  fail=1
fi

if [[ $fail -ne 0 ]]; then
  echo "license-scan: FAIL"
  exit 1
fi
echo "license-scan: OK"
