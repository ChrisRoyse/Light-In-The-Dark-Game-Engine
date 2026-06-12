#!/usr/bin/env bash
# Appends assets/MANIFEST entries for every file under assets/<subdir>.
# Usage: scripts/manifest-add.sh <subdir> <pack-name> <source-url> [retrieved-date]
# Generation helper only — verification is tools/assetcheck, never this script.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

subdir=$1; pack=$2; source_url=$3; retrieved=${4:-$(date +%F)}

find "assets/$subdir" -type f | LC_ALL=C sort | while IFS= read -r f; do
  rel="${f#assets/}"
  sha=$(sha256sum "$f" | cut -d' ' -f1)
  cat >> assets/MANIFEST <<EOF

[[asset]]
path = "$rel"
pack = "$pack"
source = "$source_url"
license = "CC0-1.0"
retrieved = "$retrieved"
sha256 = "$sha"
EOF
done
echo "added $(find "assets/$subdir" -type f | wc -l) entries for assets/$subdir"
