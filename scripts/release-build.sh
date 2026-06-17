#!/usr/bin/env bash
# release-build.sh — the release build mechanism for #184
# (docs/release/versioning.md "Release steps"). It stamps the binaries with the
# git-tag version + commit + build date via -ldflags into litd/buildinfo, runs
# --version verification, and generates the changelog draft. A future CI
# pipeline (#182, gated on hosting #318 / CI #284) invokes this; it is fully
# runnable locally today.
#
# Fail-closed: a dev (untagged) or dirty-tree build is stamped dev/-dirty and
# the script refuses to mark it publishable, matching the doc's rule that such
# builds are never released.
#
# Usage: scripts/release-build.sh [output-dir]   (default: bin/)
set -euo pipefail

OUT="${1:-bin}"
PKG="github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/buildinfo"

# Single source of version truth = the git tag.
VERSION="$(git describe --tags --always --dirty 2>/dev/null || echo dev)"
COMMIT="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
if ! git diff --quiet 2>/dev/null || ! git diff --cached --quiet 2>/dev/null; then
  COMMIT="${COMMIT}-dirty"
fi
# Build date: deterministic from the commit when possible (reproducible),
# falling back to now for a dirty tree.
DATE="$(git show -s --format=%cI HEAD 2>/dev/null || date -u +%Y-%m-%dT%H:%M:%SZ)"

LDFLAGS="-X ${PKG}.version=${VERSION} -X ${PKG}.commit=${COMMIT} -X ${PKG}.date=${DATE}"

mkdir -p "$OUT"
echo "==> stamping version=${VERSION} commit=${COMMIT} date=${DATE}"

echo "==> building headless (CGO_ENABLED=0)"
CGO_ENABLED=0 go build -ldflags "$LDFLAGS" -o "$OUT/headless" ./cmd/headless

echo "==> building firstlight"
go build -ldflags "$LDFLAGS" -o "$OUT/firstlight" ./cmd/firstlight

echo "==> --version verification (Source of Truth = the built binaries):"
HV="$("$OUT/headless" -version)"
FV="$("$OUT/firstlight" -version)"
echo "    headless:   $HV"
echo "    firstlight: $FV"
if [ "$HV" != "$FV" ]; then
  echo "ERROR: binaries report different versions" >&2
  exit 1
fi

echo "==> changelog draft (hand-edit before publish):"
go run ./tools/changelog -version "$VERSION" || true

# Publishability gate (doc: dev/dirty/untagged builds are never publishable).
# Publishable only when HEAD is EXACTLY a tag and the tree is clean — a bare
# commit hash (no tag) or any modification refuses upload.
if git describe --tags --exact-match HEAD >/dev/null 2>&1 \
   && git diff --quiet 2>/dev/null && git diff --cached --quiet 2>/dev/null; then
  echo "==> publishable release build: ${VERSION}"
else
  echo "==> NOT PUBLISHABLE (untagged/dev/dirty build): upload refused."
fi
