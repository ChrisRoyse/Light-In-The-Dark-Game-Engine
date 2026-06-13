# M2 Q2 sample-port findings

**Issue:** #17 — tooling: M2 sample-port validation of mapping table (Q2 evidence)
**Decision under test:** [D-2026-06-11-2 (Q2)](../../../docs/prd/01-vision/decisions.md) — idiomatic Go only, no JASS aliases; the generated JASS→Go mapping table is the sole migration aid.
**Reopening condition (from D-2 / Q2):** *"if a table-driven port proves genuinely painful, that finding reopens the question with evidence."*

## What was ported

`reinforce.j` → `reinforce.go`. A representative WC3 World-Editor GUI-generated
"BJ" trigger: while the player owns fewer than five footmen, spawn one at the
start location, heal it, pause it, teleport it via a location handle, unpause.

**Porting rule (per #17):** every name resolved by looking it up **only** in the
generated `docs/prd/03-api/jass-mapping/mapping-table.md`. No reading of jassgen
source, no guessing. Each lookup's outcome is recorded below.

**Provenance / deviation:** the fragment is self-authored in the canonical
Blizzard BJ idiom, **not** extracted from a published map binary. Reason: the M2
manifest maps only six D1–D5 exemplar functions, so no real published map could
compile against the M2 stub surface. The fragment is faithful to real GUI output
(BJ wrappers, `location` handles, `PolledWait`, the `bj_lastCreatedUnit` side
channel) so the Q2 lookup exercise is representative. Deviation filed as a
`type:discovery` issue.

## Lookup-outcome table (Source of Truth: mapping-table.md, read 2026-06-13)

| # | JASS function | table row (rule → disposition) | outcome |
|---|---|---|---|
| 1 | `GetUnitCount` | commonai, D2 → `litd/ai.UnitCount` | **found** |
| 2 | `SetUnitState` | common.j, D5 → `litd/api.Unit.SetLife` | **found** |
| 3 | `PauseUnitBJ` | blizzard.j, D2 → `litd/api.Unit.SetPaused` | **found** |
| 4 | `PolledWait` | blizzard.j, D4 → `litd/api/helpers.PolledWait` | **found** |
| 5 | `SetUnitPositionLoc` | common.j, D3 → `litd/api.Unit.SetPosition` (D3 collapse → SetUnitPosition) | **found** |
| 6 | `IsUnitPausedBJ` | blizzard.j, D1 → `litd/api.Unit.Paused` | **found** |
| 7 | `GetLastCreatedUnit` | blizzard.j, D2 → **tombstoned (superseded)**: "Go return value from Game.CreateUnit replaces the bj_lastCreatedUnit side channel" | **found (tombstone + guidance)** |
| 8 | `GetStartLocationLoc` | common.j, unclassified → _pending (M2 backlog)_ | **missing (pending)** |
| 9 | `GetRectCenter` | blizzard.j, D2 → _pending (M2 backlog)_ | **missing (pending)** |
| 10 | `CreateNUnitsAtLoc` | blizzard.j, D4 → _pending (M2 backlog)_ | **missing (pending)** |
| 11 | `Player` | common.j, unclassified → _pending (M2 backlog)_ | **missing (pending)** |
| 12 | `RemoveLocation` | common.j, unclassified → _pending (M2 backlog)_ | **missing (pending)** |

### Counts

| Outcome | Count |
|---|---|
| found (clear canonical symbol) | 6 |
| found (tombstone with replacement guidance) | 1 |
| found-but-unclear (mapping present but ambiguous) | 0 |
| missing (returned `pending`, no canonical symbol yet) | 5 |
| **not in table at all (a true gap)** | **0** |
| **total functions used** | **12** |

**"Never a gap" guarantee held:** all 12 lookups returned a row. None was absent;
the five misses are explicit `pending` cells, not silent holes.

## Edge cases (the three required by #17)

### Edge 1 — tombstoned function (`GetLastCreatedUnit`): does replacement guidance work?

```jass
// BEFORE (JASS, the bj_lastCreatedUnit side-channel idiom):
call CreateNUnitsAtLoc(1, 'hfoo', Player(0), spawn, bj_UNIT_FACING)
set u = GetLastCreatedUnit()
```
```go
// AFTER (Go): the table's tombstone row says the Go creator RETURNS the handle,
// so there is no side channel to read. In the port, `u` IS that returned handle
// (passed in here because CreateNUnitsAtLoc itself is still pending):
func reinforceActions(u litd.Unit, staging litd.Vec2) {
```
**Verdict: guidance worked.** The tombstone told the porter exactly what to do
(read the return value, drop the global) — no ambiguity, no source-reading.

### Edge 2 — D4 helper (`PolledWait`): resolved to `helpers.*`?

```jass
// BEFORE:
call PolledWait(2.0)
```
```go
// AFTER:
helpers.PolledWait(2.0)
```
**Verdict: resolved.** Table pointed at `litd/api/helpers.PolledWait`; the import
and call are mechanical.

### Edge 3 — `...Loc` variant (`SetUnitPositionLoc`): resolved to the Vec2 collapse?

```jass
// BEFORE (location handle):
local location staging = GetRectCenter(bj_mapInitialPlayableArea)
call SetUnitPositionLoc(u, staging)
```
```go
// AFTER (Vec2 value type absorbs the location handle; D3 collapse → SetUnitPosition):
func reinforceActions(u litd.Unit, staging litd.Vec2) {
    ...
    u.SetPosition(staging)
```
**Verdict: resolved.** `SetUnitPositionLoc`, `SetUnitPosition`, `SetUnitX`,
`SetUnitY` all collapse to one `Unit.SetPosition(Vec2)` — exactly the dedup
policy's intent. The `location` handle has no separate Go type; `Vec2` is it.

## Q2 verdict

> **Q2 stays DECIDED — idiomatic-only (D-2). The reopening condition did NOT fire.**

For every function with a resolved disposition (7 of 12), the table answered
unambiguously and the idiomatic Go name was findable **without** reading jassgen
source or guessing. The three dedup-policy edge cases — a tombstone with
replacement guidance, a D4 BJ→helper rename, and a `...Loc`→`Vec2` collapse — all
produced clean, idiomatic, **compiling** Go. That is the opposite of "genuinely
painful." Aliases would have bought nothing here.

The five `pending` misses are **not** Q2 (naming) pain. They are M2
exit-criterion-1 incompleteness (`unmapped != 0`): the manifest currently maps
only the six D1–D5 exemplar symbols, a thin vertical slice. The *mechanism* (look
up a JASS name, get an idiomatic symbol / tombstone / explicit pending) is sound;
the *coverage* is the open audit gate, tracked separately.

**Caveat (logged, not a reopening):** this exercise validates the table mechanics
on the mapped slice. A full-confidence Q2 verdict — the kind that could surface
table-driven pain at scale — requires re-running this port once the M2 unmapped
backlog is closed (`unmapped == 0`). Until then: no evidence of pain → no reopen.

## Reproduce

```bash
go build ./tools/jassgen/sampleport/...   # compiles against M2 stubs (panic bodies)
go vet   ./tools/jassgen/sampleport/...
# then re-do the lookups manually against docs/prd/03-api/jass-mapping/mapping-table.md
```
