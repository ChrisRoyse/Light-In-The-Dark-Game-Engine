# DEC-1526: Repo-size history-rewrite runbook (#1526 pack bloat + #542 renderevidence purge)

Status: DECISION DOCUMENT — runbook only. No rewrite has been executed. Operator runs this in a
coordinated window. Refs: #1526, #542, #1492, #780, #800.

## 1. Verified current state (2026-07-11, all numbers from actual command output)

| Fact | Value | Source command |
|---|---|---|
| Pack size | **314.01 MiB** (26,271 objects, 4 packs) | `git count-objects -vH` |
| GitHub server-side repo size | **320,294 KB (~313 MiB)** | `gh api repos/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine -q .size` |
| `docs/readme/` working tree | **111 PNGs, 158,520,361 bytes (~151.2 MiB)** | `ls docs/readme/*.png \| wc -l`; `du -sb docs/readme` |
| Unique PNG blobs in ALL history | **267 blobs, 247,655,739 bytes (~236 MiB)** | `git rev-list --objects --all \| git cat-file --batch-check ...` (awk sum over unique blob SHAs with `.png` path) |
| Stale (superseded) PNG bytes in history | **~89.1 MiB** (247.6 MB − 158.5 MB) | derived from the two rows above |
| `renderevidence` ELF blob | **3,533,165 bytes (3.5 MiB)**, blob `473d70ce`, added `11c9da83`, untracked `0901f8ee`, still in history | `git cat-file -s 473d70ce...`; `git log --all --oneline -- renderevidence` |
| Largest PNGs | GP01-town.png 1,833,099 B; GP18-faction-war.png 1,789,850 B; B05-leviathan.png 1,756,515 B | `ls -S docs/readme/*.png \| head -3 \| xargs du -b` |
| git-lfs | **3.7.1 installed** | `git lfs version` |
| git-filter-repo | NOT installed (`git: 'filter-repo' is not a git command`); Python 3.14 + pip 26.0.1 available | probes |
| oxipng | NOT installed | probes |
| Remotes | `origin` = github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine (private); **second remote `public`** = github.com/ChrisRoyse/Light-In-The-Dark-Game-Engine | `git remote -v` |
| Backup bundle | `C:/code/GameEditor-pre-ultracode-backup.bundle` — verifies, "records a complete history", head `eaf22d44` (= current main tip, 2026-07-11) | `git bundle verify` |
| Commit count on main | 1,685 | `git rev-list --count HEAD` |

Key structural fact: PNG blobs are **~75% of the pack** (236 of 314 MiB). PNGs are already
DEFLATE-compressed, so git's pack zlib gains ~nothing on them — pack bytes ≈ blob bytes.
`renderevidence` alone is only 3.5 MiB (~1.1% of the pack): purging it without addressing the
PNGs is not worth a history rewrite on its own, which is why #542 and #1526 must ride the
SAME rewrite.

## 2. Options analysis (research-backed)

### A. Re-encode PNGs WITHOUT history rewrite — REJECTED
A new commit with smaller PNGs **grows** the pack: the 267 old blobs stay reachable from
history forever; only clones truncated with `--depth` would benefit. Confirmed by first
principles (git packs are append-only object stores keyed by content hash) and by the state
table above (267 historical blobs vs 111 live files — 89 MiB is *already* stale-blob waste).

### B. Rewrite-purge-only (renderevidence + drop stale PNG history) — viable, partial
`git filter-repo` purge of `renderevidence` (−3.5 MiB) plus dropping *superseded* PNG
versions (−~89 MiB) → pack lands ≈ 220 MiB. Current 151 MiB of PNGs stays in-pack and every
future screenshot update re-accumulates stale blobs. Fixes nothing durably.

### C. Lossless re-encode (oxipng --zopfli) + rewrite — viable, partial
oxipng/zopfli re-runs PNG filter+DEFLATE only; output decodes to **bit-identical pixels**
(lossless by construction — satisfies the #780/#800 quality-floor requirement). Measured
industry results: 5–15% on already-optimized exports, 20–60% on UI screenshots; zopfli adds
2–5% over plain oxipng at large CPU cost (sources §7). These are engine screenshots (likely
already well-compressed by the capture path), so plan on ~10–20%: pack ≈ 190–210 MiB after
combining with B. Still leaves ~130+ MiB of image bytes in the pack, and the pack regrows
with every future screenshot.

### D. Git LFS migrate + renderevidence purge — RECOMMENDED
`git lfs migrate import --everything` rewrites history moving every PNG blob to LFS pointer
files; combined with the filter-repo purge of `renderevidence` in the same window:
- Pack after rewrite: **≈ 65–75 MiB** (314 − 236 PNG − 3.5 ELF + ~13 KB of pointers) — a ~78% cut, by far the largest.
- GitHub LFS quota (docs.github.com, verified 2026-07): **Free/Pro: 10 GiB storage + 10 GiB
  bandwidth/month included; Team/Enterprise: 250 GiB each**; metered overage beyond that
  (community-reported bandwidth overage ≈ $0.0875/GB; data packs 50 GiB/$5/mo still purchasable).
  Our LFS footprint: **236 MiB storage (2.3% of the free quota)**; a full fresh clone
  downloads only the ~151 MiB of *current* objects → ~67 fresh clones/month inside free
  bandwidth. Single developer, single machine: quota risk is negligible.
- Durability: future screenshots land in LFS automatically via `.gitattributes`; the pack
  never regrows from images. This is the only option that fixes the *class*, not the instance.
- Costs/risks: (i) any machine cloning must have git-lfs installed (already 3.7.1 here);
  (ii) if the monthly LFS budget is $0 and quota is ever exceeded, LFS downloads block for
  the rest of the month; (iii) the `public` mirror remote consumes ITS OWN LFS quota on the
  ChrisRoyse personal account (also 10 GiB free) when force-pushed.

**Recommendation: D**, with an optional oxipng lossless pass folded in beforehand (shrinks
LFS storage/bandwidth ~10–20% for one CPU-bound batch job; strictly optional since LFS
already removes the clone-cost problem). B alone is not durable; C alone buys the full
rewrite pain for a fraction of the win; A is a no-op on the pack.

## 3. Runbook — ONE combined rewrite (operator-executed, quiet window)

All commands PowerShell unless noted. Nothing below has been run.

### 3.0 Preconditions
- Quiet window: no other agents/sessions writing to `C:\code\GameEditor`; all worktrees
  under `.claude\worktrees\` merged or abandoned; all local branches pushed or exported.
- `git status` clean on main; `git stash list` empty (or exported).
- Decide LFS budget: org Settings → Billing → set a nonzero LFS budget or confirm free
  quota is acceptable (236 MiB ≪ 10 GiB — it is).

### 3.1 Backup (do BOTH even though a bundle exists)
```powershell
cd C:\code\GameEditor
git bundle verify C:\code\GameEditor-pre-ultracode-backup.bundle   # existing bundle: complete history, head eaf22d44
git bundle create C:\code\GameEditor-pre-rewrite-$(Get-Date -Format yyyyMMdd).bundle --all
git bundle verify C:\code\GameEditor-pre-rewrite-*.bundle          # must print "records a complete history"
```

### 3.2 Baseline FSV capture
```powershell
git count-objects -vH                                  # expect ~314 MiB — record exact
git log --all --oneline -- renderevidence              # expect 2 commits (0901f8ee, 11c9da83)
# pixel-hash manifest of every current PNG (decoded pixels, not file bytes):
python - <<'PY'
import hashlib, pathlib
from PIL import Image                                   # pip install pillow
for p in sorted(pathlib.Path('docs/readme').glob('*.png')):
    h = hashlib.sha256(Image.open(p).convert('RGBA').tobytes()).hexdigest()
    print(f"{h}  {p.name}")
PY
# save output as pixel-manifest-before.txt
```
NOTE: `Get-FileHash` (file bytes) is the right check for steps that must NOT touch PNG bytes
(purge, LFS migrate). It is the WRONG check after oxipng — re-encoding legitimately changes
file bytes; only the decoded-pixel sha256 above proves losslessness.

### 3.3 Fresh working clone for the rewrite (filter-repo refuses non-fresh clones)
```powershell
pip install git-filter-repo                             # Python 3.14 present; installs `git filter-repo`
cd C:\code
git clone https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine.git GameEditor-rewrite
cd GameEditor-rewrite
```

### 3.4 (Optional but recommended) lossless oxipng pass — BEFORE the LFS migrate
```powershell
cargo install oxipng
oxipng -o max --zopfli --strip safe docs/readme/*.png   # hours of CPU; lossless by construction
# FSV: decoded-pixel hashes must equal pixel-manifest-before.txt EXACTLY:
python <pixel-hash script from 3.2>  > pixel-manifest-after-oxipng.txt
git diff --no-index pixel-manifest-before.txt pixel-manifest-after-oxipng.txt   # must be empty
# any mismatch = GATE_FAILED: step=oxipng detail=<file> — revert that file, do not proceed lossy
git add docs/readme; git commit -m "#1526: lossless oxipng --zopfli re-encode of docs/readme PNGs (pixel-hash verified)"
```

### 3.5 Purge renderevidence from all history
```powershell
git filter-repo --invert-paths --path renderevidence
# NOTE: filter-repo deletes the 'origin' remote as a safety measure — re-added in 3.7.
git log --all --oneline -- renderevidence               # FSV: MUST be empty
```

### 3.6 LFS migrate (second half of the same rewrite)
```powershell
git lfs install
git lfs migrate import --include="docs/readme/*.png,g3nd/**/*.png" --everything
# --everything rewrites all local refs; writes .gitattributes with the LFS filter
# g3nd/ is a DELETED directory whose 44 historical PNG blobs (6,001,120 B) are still in
# the pack (verified: git rev-list --objects --all | grep '\.png$' -> 223 docs/ + 44 g3nd/
# paths). Migrating them to LFS strips them from the pack; alternatively purge them
# outright in 3.5 with: git filter-repo --invert-paths --path g3nd --path renderevidence
# (they are dead content — purge is the smaller end-state; LFS-migrate keeps history intact).
@(git lfs ls-files).Count                              # FSV: tracked LFS file count (122 as of 2026-07). DERIVE it - do not trust a stale literal, and never count rows with a blank-line-blind counter that silently undercounts (#1556).
git count-objects -vH                                   # FSV: size-pack ~65-75 MiB (record exact)
# byte-hash check (migrate must not alter content): Get-FileHash on all docs/readme/*.png
# must equal the same hashes taken right after 3.4's commit.
```

### 3.7 Push (uploads LFS objects, then force-pushes rewritten refs)
```powershell
git remote add origin https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine.git
git push origin --force --all
git push origin --force --tags
# optional mirror (consumes ChrisRoyse account's own LFS quota):
git remote add public https://github.com/ChrisRoyse/Light-In-The-Dark-Game-Engine.git
git push public --force --all; git push public --force --tags
```
GitHub caveat: old commits remain fetchable by SHA server-side until GitHub GC; for a
guaranteed server purge of `renderevidence`, open a support ticket asking them to run GC.
Low urgency for a private repo.

### 3.8 Re-sync the working machine (single dev = single machine to coordinate)
```powershell
# The old C:\code\GameEditor now has divergent history. Do NOT pull. Replace:
Rename-Item C:\code\GameEditor C:\code\GameEditor-old
git clone https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine.git C:\code\GameEditor
cd C:\code\GameEditor; git lfs pull
# restore untracked local state from GameEditor-old: .claude\settings.local.json, local-asset-vault
# if unversioned, target/ cache is at C:\code\GameEditor\target (CARGO_TARGET_DIR) - copy back to keep the warm cache (#1498)
# re-create any hooks (.git/hooks) that the old clone had; delete GameEditor-old after a green gate run
```

### 3.9 Post-rewrite FSV gate (all four must pass)
1. `git count-objects -vH` in the fresh clone: size-pack ≤ 80 MiB (record exact number in the closing comment).
2. `git log --all --full-history --oneline -- renderevidence` → empty (closes #542).
3. Decoded-pixel manifest of all 111 PNGs == `pixel-manifest-before.txt` (closes the #780/#800 lossless requirement).
4. Full test gate green (`scripts/` gate) + `cargo build --bin litd-editor` on the fresh clone.

### 3.10 Rollback (any step fails)
```powershell
# The rewrite clone is disposable - just delete C:\code\GameEditor-rewrite.
# If a bad force-push already landed:
git clone C:\code\GameEditor-pre-rewrite-<date>.bundle C:\code\GameEditor-restore
cd C:\code\GameEditor-restore
git remote add origin https://github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine.git
git push origin --force --all; git push origin --force --tags     # restores pre-rewrite history exactly
```
The bundle is the single source of rollback truth; it verifies today ("records a complete
history", head `eaf22d44`). Keep both bundles until the post-rewrite gate has been green for
a week.

## 4. Edge cases audited
- **Empty/no-op input**: `git lfs migrate import` with a pattern matching nothing rewrites
  nothing but still churns refs — the include pattern was checked against the 111 live
  files and the 267 historical blob paths. CORRECTION (adversarial verify, 2026-07-11):
  the historical blobs are NOT all under `docs/readme/` — 223 blobs (241,654,619 B) are
  `docs/readme/`, 44 blobs (6,001,120 B) are under the deleted `g3nd/` tree. A
  `docs/readme/*.png`-only pattern strands ~6 MiB in the pack, and the earlier
  "~89.1 MiB stale" figure mixed the two scopes (correct: ~83.1 MiB stale under
  docs/readme). §3.6 now includes `g3nd/**/*.png` (or purge g3nd in §3.5). The ≤80 MiB
  post-rewrite gate and the overall recommendation are unchanged.
- **Boundary**: largest file GP01-town.png (1.83 MB) is far under GitHub's 100 MB hard
  limit either way; the 3.5 MB ELF is the only non-PNG blob >1 MB besides the intentional
  api-manifest.json (per #542's sweep).
- **Invalid format**: any PNG that Pillow cannot decode fails the 3.2 manifest step BEFORE
  the rewrite starts (loud error, no silent skip) — the manifest is the precondition gate.
- **Second remote**: `public` mirror must be force-pushed too or it silently diverges;
  its LFS storage bills to the personal account, not the org.

## 5. Sources
- GitHub LFS billing/quotas: https://docs.github.com/billing/managing-billing-for-git-large-file-storage/about-billing-for-git-large-file-storage (Free/Pro 10 GiB + 10 GiB/mo; Team/Enterprise 250 GiB; metered overage; $0 budget ⇒ LFS blocked on overage)
- LFS metered billing FAQ (overage ≈ $0.0875/GB bandwidth, 50 GiB/$5 data packs): https://github.com/orgs/community/discussions/61362
- git-lfs-migrate man page (`--everything`, force-push requirement, divergent-history warning): https://github.com/git-lfs/git-lfs/blob/main/docs/man/git-lfs-migrate.adoc
- git-filter-repo install (pip on Windows) + fresh-clone safety: https://github.com/newren/git-filter-repo/blob/main/INSTALL.md , https://www.git-tower.com/learn/git/faq/git-filter-repo
- oxipng (lossless, bit-identical pixels): https://github.com/oxipng/oxipng ; measured savings ranges: https://op111.net/posts/2025/09/png-compression-oxipng-optipng-fileoptimizer-cwebp/ , https://z.tools/blog/oxipng-lossless-png-pixel-perfect
- zopfli 3–8% over max zlib: https://blog.codinghorror.com/zopfli-optimization-literally-free-bandwidth/
