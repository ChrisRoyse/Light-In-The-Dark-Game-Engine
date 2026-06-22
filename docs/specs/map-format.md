# Skirmish Map Data Format

**Status:** v1 (`FormatVersion = 1`). **Owner:** `litd/asset/mapdata`.
**Validator:** `go run ./tools/assetcheck --json data/maps/<map>/` (the `data`
mode — see [Validation §3.4](../prd/06-assets/validation-and-data.md)).
**Reference fixture:** [`data/maps/_fixture/`](../../data/maps/_fixture) — the
smallest map that exercises every table; load it to see a canonical example.

This is the authored, immutable (R-AST-1) terrain data a skirmish map ships.
The sim reads walkability and cliff levels from it; it never sees the render
mesh (R-SIM-5). All data loads once before the first tick into read-only
structs; the loaded map's `Fingerprint` is folded into the match state-hash
preamble so lockstep peers and replays verify they ran identical terrain
(R-SIM-2).

## Directory layout

```
data/maps/<map>/
  terrain.toml    # metadata, start locations, beacon placements
  pathing.txt     # per-cell pathing flags          (terrain grid)
  cliff.txt       # per-cell cliff level + ramps     (terrain grid)
  height.txt      # per-vertex heightmap             (terrain grid)
  splat.txt       # per-tile ground-texture weights  (terrain grid)
  doodads.toml    # static prop placements (render-only by default, D-13)
```

All six files are required. A missing file, a parse error, or any
cross-reference violation is a hard load error — the loader never partial-loads
or substitutes defaults (fail-closed, no silent fallback).

## Coordinate systems

Two grids, related by `pathing-scale` (fixed at **4**, the `PathingScale`
constant):

- **Tile grid** — `width × height` tiles. `terrain.toml` dimensions are tiles;
  `splat.txt` is one entry per tile.
- **Pathing grid** — `(width·4) × (height·4)` cells. `pathing.txt` and
  `cliff.txt` are one entry per pathing cell. `[[start]]` / `[[beacon]]` cell
  coordinates and doodad cells are pathing-grid cells.
- **Vertex grid** — `(width+1) × (height+1)` height samples (`height.txt`),
  one more than tiles in each axis so every tile has four corner heights.

Dimensions are bounded `1 ≤ width,height ≤ 512` tiles.

## §5.2 decision — terrain grids are TEXT, split per layer

[validation-and-data.md §5.2](../prd/06-assets/validation-and-data.md) left
open whether map terrain grids are text or packed binary, to be decided "with
the Terrain M4 representation choice (tile-palette maps favor text)."

**Decision: text, one file per layer, run-length-encoded.** The early spec
sketch named a single binary `terrain.grid`; the shipped format instead uses
four plain-text layer files (`pathing.txt`, `cliff.txt`, `height.txt`,
`splat.txt`).

**Rationale.**
- LitD terrain is a discrete tile-palette model (discrete cliff levels + ramps,
  per-cell pathing flags, four-way splat weights) — exactly the case the spec
  flags as favoring text.
- Hand-authorable and diffable: a map author edits these by hand and reviews
  changes in a normal diff; a packed binary blob is neither.
- Run-length encoding (below) keeps large uniform maps compact without a
  binary container — the dominant case is big runs of identical ground.
- Determinism does not need binary: the canonical-bytes fingerprint
  (`Map.Fingerprint`) is computed from the *parsed* values, not the file bytes,
  so whitespace/RLE-grouping differences never change a map's identity.

Trade-off accepted: parsing is marginally slower than a `mmap` of packed bytes,
irrelevant for a load-once-per-match asset.

## terrain.txt grid encoding (shared by all four layers)

Each grid file is rows top-to-bottom, one logical row per line, values
space-separated left-to-right. Two compression forms, both expand before
validation so the grid is always checked at full resolution:

- **Cell run-length:** `atom*N` repeats `atom` N times across the row
  (`3*256` = 256 cells of `3`). N must be a positive integer.
- **Row repeat:** a line `@repeat N <row>` emits `<row>` N times
  (`@repeat 120 3*256` = 120 identical rows). N must be positive.

Every expanded row must have exactly the layer's width; the file must have
exactly the layer's height. Too few/many values or rows is an error naming the
offending row. An empty physical line is an error (no blank-line tolerance).

### pathing.txt — `PathFlags` bitfield per pathing cell

Integer (decimal or `0x…`) bitfield: `1`=walkable, `2`=buildable, `4`=water.
- Unknown bits → error.
- Water (`4`) must not also set walkable/buildable → error.
- Typical ground is `3` (walkable+buildable); impassable is `0`; water is `4`.

### cliff.txt — cliff level + ramp per pathing cell

Decimal level `0..126`. Optional leading `r` marks the cell a ramp
(`r0`, `r1`). Levels above 126 → error. A ramp cell must connect two cliff
levels that differ by exactly one; a ramp whose neighbours disagree is a
`ramp at (x,y)` error (cross-checked after the grid loads, `validateRamps`).

### height.txt — `int32` height per vertex

Signed decimal, one per vertex on the `(width+1)×(height+1)` grid. No range
limit beyond `int32`.

### splat.txt — four ground-texture weights per tile

`a,b,c,d` — four `0..255` weights, comma-separated, **summing to exactly 255**.
A sum ≠ 255 is an error. One entry per tile on the `width×height` grid.

## terrain.toml

```toml
version = 1                # must equal FormatVersion (1)
width = 8                  # tiles, 1..512
height = 8                 # tiles, 1..512
biome = "vigil-lowlands"   # required, non-empty
pathing-scale = 4          # optional; only 4 supported (default 4)

[lighting]                  # optional; omitted => documented canonical noon default
ambient-color = [0.82, 0.88, 1.0]   # RGB floats, each 0..1
ambient-intensity = 0.62            # 0..8
sun-color = [1.0, 0.96, 0.86]       # RGB floats, each 0..1
sun-intensity = 1.05                # 0..8
sun-azimuth = 180                   # degrees, [0,360)
sun-elevation = 65                  # degrees, [-90,90]

[[start]]                  # ≥1 required
player = 0                 # 0..15, unique per map
cell = [4, 4]              # pathing-grid cell; must be buildable, non-water

[[beacon]]                 # optional; capturable control points (#200)
id = 1                     # unique per map
cell = [16, 16]            # pathing-grid cell; must be in bounds
# owner omitted => BeaconNeutral (-1, uncontrolled)

[[beacon]]
id = 2
cell = [16, 8]
owner = 0                  # 0..15: a player starts holding this beacon
```

- `version` must equal the loader's `FormatVersion`; any other value is
  rejected (no forward/backward silent acceptance).
- **Start locations:** ≥1 required; `player` in `[0,15]` and unique; `cell` in
  pathing bounds and on buildable, non-water ground (a start on water/unbuildable
  is an error). Returned sorted by player.
- **Lighting:** optional table. Omission loads the canonical noon-like default
  shown above. When `[lighting]` is present, every field is required so a partial
  authored override cannot accidentally mix a new sun with old ambient values.
  Colors are normalized RGB floats in `[0,1]`; intensities are finite floats in
  `[0,8]`; `sun-azimuth` is degrees in `[0,360)`; `sun-elevation` is degrees in
  `[-90,90]`. Lighting is part of map identity — the `Fingerprint` covers every
  lighting field because two peers with different sun/ambient values would render
  a different match presentation.
- **Beacons:** optional; `id` unique; `cell` in pathing bounds; `owner` is
  `BeaconNeutral` (`-1`) when omitted, else a player index `[0,15]`. Out-of-bounds
  cell, out-of-range owner, or duplicate id is an error. Returned sorted by id.
  Beacons are the light/territory win lever ([identity.md §3]); the beacon-hold
  victory in #200 reads these. They are part of map identity — the `Fingerprint`
  covers beacon id/position/owner, so two maps that differ only in beacon
  placement have distinct fingerprints.
- Unknown keys are rejected by the strict TOML decoder.

## doodads.toml

```toml
[[doodad]]
id = 1                     # unique per map
asset = "pack/model.glb"   # must resolve under assets/ ; relative, no ".."
cell = [12, 12]            # pathing-grid cell, in bounds (incl. footprint)
rotation = 0               # BAM, 0..65535
destructible = true
footprint = [2, 2]         # optional [w,h], positive; default [1,1]
```

Doodads are render-only by default (promotable to sim collidables per D-13).
A duplicate id, missing/`..`-containing/unresolvable asset, out-of-bounds cell,
a footprint that leaves the map, or rotation outside `[0,65535]` is an error.
A map with no props ships a `doodads.toml` with no `[[doodad]]` tables (the
reference fixture does this so it has no dependency on the gitignored
`assets/` binaries).

## Validation & FSV

`assetcheck` in `data` mode loads every `data/maps/<map>/` through
`mapdata.Load` and reports a `MAPDATA` finding per failing map; zero findings =
the map is well-formed and cross-consistent. Because the loader is the single
validator, the rules above are enforced in exactly one place.

The regression suite (`litd/asset/mapdata/*_test.go`) loads the fixture and the
`test64` map, dumps the parsed state, and asserts known inputs map to known
outputs (start/beacon coordinates, pathing/cliff/height/splat samples, and
fingerprint stability + sensitivity), plus the rejection paths above.
