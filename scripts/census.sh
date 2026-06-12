#!/bin/bash
# Census driver: runs animtest per GLB, measures screenshot byte-diffs and
# non-emptiness, emits one TSV row per model.
#
# CACHED: results keyed by sha256 of the GLB in artifacts/census/census-cache.tsv.
# A model whose bytes are unchanged is never re-rendered — its cached row is
# reused. Delete a cache line (or the file) to force re-census of that model.
cd /home/paula/projects/light-in-the-dark-game-engine
OUTDIR=artifacts/census
CACHE="$OUTDIR/census-cache.tsv"   # sha256<TAB>rel<TAB>status<TAB>...row fields
OUT="$OUTDIR/census.tsv"
mkdir -p "$OUTDIR"
touch "$CACHE"
: > "$OUT"
n=0; hit=0; miss=0
find assets -name "*.glb" | LC_ALL=C sort | while IFS= read -r f; do
  n=$((n+1))
  rel="${f#assets/}"
  sha=$(sha256sum "$f" | cut -d' ' -f1)
  cached=$(grep -m1 -P "^$sha\t" "$CACHE" | cut -f2-)
  if [ -n "$cached" ]; then
    hit=$((hit+1))
    printf '%s\n' "$cached" >> "$OUT"
    echo "[$n] $rel CACHED"
    continue
  fi
  miss=$((miss+1))
  slug=$(echo "$rel" | tr '/.' '__')
  log=$(timeout 90 ./bin/animtest -glb "$f" -out "$OUTDIR/$slug" 2>&1)
  rc=$?
  meshes=$(echo "$log" | grep -oE '[0-9]+ meshes' | grep -oE '^[0-9]+')
  skins=$(echo "$log" | grep -oE '[0-9]+ skins' | grep -oE '^[0-9]+')
  anims=$(echo "$log" | grep -oE '[0-9]+ animations' | grep -oE '^[0-9]+')
  joints=$(echo "$log" | grep -oE '\([0-9]+ joints\)' | grep -oE '[0-9]+' | head -1)
  clips=$(echo "$log" | grep -cE '^anim: ')
  shots=$(echo "$log" | grep -cE '^event: screenshot')
  err=$(echo "$log" | grep -E "^error:|panic:" | head -1 | tr '\t' ' ')
  d01=0; d12=0; colors=0
  if [ "$shots" = "3" ]; then
    d01=$(cmp -l "$OUTDIR/$slug-0.png" "$OUTDIR/$slug-1.png" 2>/dev/null | wc -l)
    d12=$(cmp -l "$OUTDIR/$slug-1.png" "$OUTDIR/$slug-2.png" 2>/dev/null | wc -l)
    colors=$(python3 -c "
from PIL import Image
print(len(set(Image.open('$OUTDIR/$slug-0.png').convert('RGB').getdata())))" 2>/dev/null)
  fi
  if [ "$rc" != "0" ] || [ "$shots" != "3" ]; then
    status="R1-FAIL-RUN"
  elif [ "${colors:-0}" -le 2 ]; then
    status="R1-EMPTY-RENDER"
  elif [ "${anims:-0}" -gt 0 ] && [ "$d01" = "0" ] && [ "$d12" = "0" ]; then
    status="R1-STATIC-WITH-CLIPS"
  elif [ "${anims:-0}" -gt 0 ]; then
    status="OK-ANIMATED"
  else
    status="OK-STATIC"
  fi
  row=$(printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s' \
    "$rel" "$status" "${meshes:-0}" "${skins:-0}" "${joints:-}" "${anims:-0}" "$d01" "$d12" "${colors:-0}" "$rc" "$err")
  printf '%s\n' "$row" >> "$OUT"
  printf '%s\t%s\n' "$sha" "$row" >> "$CACHE"
  echo "[$n] $rel $status"
done
echo "CENSUS DONE: $(wc -l < "$OUT") rows ($(grep -c CACHED /tmp/census-progress.log 2>/dev/null || true) from cache)"
