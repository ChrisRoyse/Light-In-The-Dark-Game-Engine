# World Source Form — the VCS-Native Editing Format

**Status:** v1 specification (implements D-2026-06-11-33; companion of the archive
format D-2026-06-11-14 / issue #205).
**Audience:** world authors (human and AI), the M8 editor (#11, #131), `tools/worldpack`
(#10), the engine's world loader (#268).

A world has two forms. **Source form** — this document — is a plain directory of
diffable text plus referenced binary assets: the format people *edit and collaborate
in*, with git or any VCS or none. **Archive form** (`.litdworld`) is the distribution
artifact built deterministically from source form by `tools/worldpack`; it is never
edited directly. The editor reads and writes source form only; archive export is a
build step.

## 1. Directory layout

```
<world-name>/
├── world.toml              # identity + metadata (required)
├── data/                   # gameplay tables, R-AST-1 conventions (optional)
│   ├── units.toml
│   ├── abilities.toml
│   └── upgrades.toml
├── scripts/                # Lua, runs in the R-SEC-1 sandbox (optional)
│   └── main.lua            # entry point; required iff scripts/ exists
├── campaigns/              # campaign definitions with mission-flow hooks (optional)
│   └── first-flame.toml
├── map/                    # terrain + placements, line-stable text (required)
│   ├── terrain.toml        # dimensions, tileset ref, biome
│   ├── pathing.txt         # walk/build/water flags, one pathing row per line
│   ├── height.txt          # height grid, one row per line
│   ├── cliff.txt           # cliff-level grid, one row per line
│   ├── splat.txt           # texture blend-weight grid, one row per line
│   ├── entities.toml       # one unit/entity placement per line
│   └── doodads.toml        # one scenery placement per line
├── locale/                 # string tables, D-17 (optional)
│   └── en.toml
└── assets/                 # binary GLB/OGG/PNG (optional)
    ├── MANIFEST            # provenance ledger, same schema as engine assets/
    └── ...
```

Required: `world.toml`, `map/`. Everything else is optional; absence means "none",
never an error. Unknown files and directories are a **load error** in the engine and
`tools/worldpack` (fail closed — typos must not silently vanish from the archive).

### campaigns/*.toml — mission-flow surface

Campaign definitions are ordinary source-form passthrough files packed into the
archive. The runtime loads them through `litd/campaign`; Lua hooks are resolved
from the world's sandboxed `scripts/main.lua` environment.

```toml
id = "first-flame"
title = "First Flame"
faction = "The Vigil"

[hooks]
on-complete = "OnMissionComplete"
on-fail = "OnMissionFail"

[carry]
heroes = ["Ser Caldus"]
items = ["Ember Ward"]
cache-keys = ["checkpoint"]

[[mission]]
id = "m1"
title = "Kindle the Gate"
archive = "worlds/m1.litdworld"

[[mission]]
id = "m2"
title = "Hold the Dawn"
archive = "worlds/m2.litdworld"
requires = ["m1"]
```

Hook functions return a table such as:

```lua
return {
  next = "m2",
  heroes = {
    { name = "Ser Caldus", level = 4, items = { "Ember Ward" } },
  },
  cache = { "checkpoint" },
  log = { "complete:m1->m2" },
}
```

The carry manifest is an allow-list. Returned heroes, items, and cache keys not
listed there are rejected; listed items/cache keys that are absent at transition
time are skipped with a deterministic hook-log entry. Hooks run in the same
no-io/no-os/no-net Lua sandbox and commit to campaign storage only after the
returned next mission and carry-over data validate.

## 2. world.toml

```toml
format = 1                      # source-form format version (this spec)
id = "first-flame"              # [a-z0-9-]+, unique key; archive + hub identity
name = "loc:world.name"         # display string: literal or locale key (D-17)
description = "loc:world.desc"
authors = ["Paula Ascenzi"]     # free text; unicode permitted
engine = ">=0.4.0 <0.6.0"       # engine-version range the archive header carries (#180)
players = { min = 2, max = 2, suggested = 2 }
seed-policy = "host"            # host | fixed; fixed requires seed = <u64>
```

- `format` gates parsing: a loader seeing a higher `format` than it knows refuses
  loudly (never best-effort).
- `name`/`description` accept either a literal or a `loc:` key resolved against
  `locale/` (the same literal-or-key rule as the public API text surfaces).

### map/terrain.toml — metadata and starts

```toml
width = 8
height = 8
tileset = "vigil-lowlands"
biome = "dawn-splat"

[[start]]
player = 1
cell = [1, 1]

[[start]]
player = 2
cell = [6, 6]
```

`[[start]]` rows are required (`1..8` rows). `player` is the editor-facing slot
number (`1..8`) and must be unique. Source-form start cells are terrain-cell
coordinates; the editor maps each start to the center pathing cell (`x*4+2`,
`y*4+2`) and refuses starts outside the map or on cells that are not buildable
non-water ground in `map/pathing.txt`. Archive export copies these rows into the
manifest header so hub/load tooling can inspect map identity without reading
payload files.

## 3. Text-format rules (the diff-stability contract)

Every text file in a world obeys these rules. The point of each rule is a clean
`git diff` and a mergeable history; the M8 editor's writer (#11) enforces them
mechanically, and `tools/worldpack` validates them.

1. **Canonical key order.** TOML keys within a table are written in the order this
   spec defines for that table (not alphabetical, not insertion). Re-saving an
   unmodified file is byte-identical — a save with no edits produces an empty diff.
2. **One placement per line.** `map/entities.toml` and `map/doodads.toml` hold
   single arrays of inline tables, one element per line. Moving one object changes
   exactly one line; merging two authors' placements conflicts only when they
   touched the same placement.
3. **Stable placement identity.** Every placement carries an `id` (u32, unique
   within its file, assigned once by the editor and never reused). Placements are
   ordered by `id` ascending in the file — insertion order never reshuffles
   neighbours.
4. **Grid files are row-per-line.** `height.txt`, `cliff.txt`, `splat.txt`, and
   `pathing.txt` write one row per line, values space-separated, fixed formatting
   (no scientific notation; heights are the fixed-point integers the sim uses —
   floats never appear in map data). `height.txt`, `cliff.txt`, and `splat.txt`
   are terrain-cell grids. `pathing.txt` is a pathing-cell grid with width and
   height equal to `terrain.toml` dimensions multiplied by 4. `splat.txt` cells
   are canonical four-way blend weights (`a,b,c,d`) that sum to 255. Editing one
   region touches only that region's lines.
5. **No timestamps, no machine names, no editor versions** anywhere in source form.
   Provenance lives in the VCS, not the files.
6. **Numbers are written canonically.** Integers without leading zeros; fixed-point
   values as integers in sim units (the loader's authoring-units conversion — seconds,
   units/second — applies to `data/` tables only, where the human-friendly form is the
   point).
7. **Newline `\n`, UTF-8, no BOM, trailing newline required.** Unicode is permitted
   in every human-facing string field (names, descriptions, authors).

### map/pathing.txt — pathing flags

`pathing.txt` is the canonical source-form authoring layer for placement and
runtime pathing. It uses the same flag byte as `litd/asset/mapdata`: decimal or
`0x...` integer values where `1` means walkable, `2` means buildable, and `4`
means water. Unknown bits are a load error. Water must not be combined with
walkable or buildable. Typical ground is `3`; impassable terrain is `0`; water is
`4`.

Unit placement checks a unit's loaded data row. Ground units test a square
footprint derived from `collision-size` / `CollisionClass` around the center
pathing cell of the selected terrain cell. Building-like unit rows with a
non-zero `footprint` test that authored footprint against buildable ground.
Doodads remain scenery placement by default and are not rejected by pathing flags.

### map/entities.toml — worked example

```toml
# one element per line; ordered by id; ids never reused
entities = [
  { id = 1, type = "footman", player = 0, pos = [4096, 4096], rotation = 16384, scale = 1000 },
  { id = 2, type = "beacon", player = 255, pos = [8192, 8192], rotation = 0, scale = 1000 },
]
```

`pos` is sim fixed-point world units (integers); `rotation` is a BAM angle;
`scale` is an integer per-mille transform scale where `1000` means 1.0x. Legacy
hand-authored `facing` is accepted as an alias for `rotation` and normalized on
save.

### map/doodads.toml — worked example

```toml
# one element per line; ordered by id; ids never reused
doodads = [
  { id = 1, type = "kaykit-hexagon/tree_single_A.glb", pos = [4096, 8192], rotation = 0, scale = 1000 },
]
```

`type` is the doodad/scenery type ID from the data doodad tables; for the current
asset-backed tables this is the referenced asset path. Doodads are render-only by
default and become gameplay state only if runtime scripting promotes them.

## 4. Binary assets

- Binary files (`.glb`, `.ogg`, `.png`) live under `assets/`, whole-file replaced on
  change — standard git behavior applies, and the layout is **git-LFS compatible**
  (binaries are ordinary tracked files; an author may add
  `assets/** filter=lfs` patterns without affecting the engine or worldpack, which
  read the working tree, never git internals).
- `assets/MANIFEST` uses the same provenance schema as the engine's asset ledger
  (path / pack / source / license / sha256, G4.7): every binary listed exactly once,
  hashes verified by `tools/worldpack` at pack time and by the engine at load. The
  license allowlist for distributable worlds is enforced at hub upload (#176), not at
  local load.

## 5. Relationship to the archive (`.litdworld`)

`tools/worldpack` (#10) builds the archive from source form deterministically: sorted
entry order, fixed metadata, content hash in the header. Identical source trees
produce byte-identical archives on any machine — the property the M7 join guard (#74)
and M9 hub (#178) rely on. Unpacking an archive reproduces the source tree
byte-exactly; pack→unpack→pack is a fixed point.

## 6. Collaboration model

Asynchronous, git-style (D-33): authors branch, edit source form, merge with ordinary
text tooling. The format rules above are what make that workable — no real-time
co-editing layer exists or is planned for v1. Nothing in the engine reads VCS state;
"any VCS or none" is literal.

## 7. Validation summary (who enforces what)

| Rule | Enforced by |
|---|---|
| Layout / unknown files | engine loader (#268), `tools/worldpack` (#10) |
| `format` version gate | engine loader, worldpack |
| Key order / line forms / canonical numbers | editor writer (#11); worldpack `--lint` |
| Entity id uniqueness + ordering | worldpack lint, editor |
| Grid dimensions vs `terrain.toml` | engine loader, worldpack |
| Pathing flags, water exclusivity, unit/start placement checks | editor, engine loader |
| MANIFEST 1:1 + hashes | worldpack, engine, assetcheck (#37) |
| Lua parse (no execution) | worldpack lint via vendored VM (#261) |

## 8. Worked sample

A complete minimal world demonstrating every rule — including the unicode, zero-asset,
and diff-locality cases from issue #9's FSV — is committed at
`examples/firstlight-sample/`. Its git history is the living demonstration: see the
commits touching exactly one line of `map/entities.toml`.
