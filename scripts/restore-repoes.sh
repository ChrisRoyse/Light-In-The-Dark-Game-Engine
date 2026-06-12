#!/usr/bin/env bash
# Restores the gitignored repoes/ tree deterministically.
# go.mod replaces github.com/g3n/engine with ./repoes/engine, so a fresh
# checkout cannot build until this has run.
set -euo pipefail

ENGINE_SHA=11eb4fd38acd68afbcb1e627f0c63b2eeb6bb2a9
WAR3_TYPES_SHA=4696fbf4041ce0d89d734e5aa753baeeae63fa82

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

clone_pinned() {
  local url="$1" dir="$2" sha="$3"
  if [ ! -d "$dir/.git" ]; then
    git clone "$url" "$dir"
  fi
  git -C "$dir" fetch --quiet origin "$sha" || true
  git -C "$dir" checkout --quiet "$sha"
}

clone_pinned https://github.com/g3n/engine repoes/engine "$ENGINE_SHA"
clone_pinned https://github.com/cipherxof/war3-types repoes/war3-types "$WAR3_TYPES_SHA"

# Re-apply the LITD-PATCH set on top of the pinned engine SHA.
git -C repoes/engine checkout --quiet -- .
for p in "$root"/patches/engine/*.patch; do
  git -C repoes/engine apply "$p"
  echo "applied: $(basename "$p")"
done

# Restore is not done until the patch markers are actually present.
if ! grep -rn "LITD-PATCH" repoes/engine --include='*.go' | head -5; then
  echo "FATAL: no LITD-PATCH markers found after restore" >&2
  exit 1
fi
echo "restore-repoes: OK (engine=$ENGINE_SHA war3-types=$WAR3_TYPES_SHA)"
