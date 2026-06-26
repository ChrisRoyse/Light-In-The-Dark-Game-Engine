# Destructables & Doodads — JASS → Go Mapping

> Part of the [JASS API mapping](README.md). Governing rules: PRD [§4.2 dedup D1–D5, §4.3 API shape](../../../PRD.md).

## Surface size (grep survey, 2026-06-11)

| Source | Approx. count | Notes |
|---|---|---|
| `common.j` natives | **~33** | destructable CRUD/life/animation, occluder height, doodad animation (`SetDoodadAnimation`), enum-in-rect |
| `blizzard.j` BJs | **~35** | `...Loc`/percent wrappers, gate/elevator macros (`ModifyGateBJ`, `ChangeElevatorHeight`), random-tree helpers |

## Representative JASS signatures

```jass
native CreateDestructable       takes integer objectid, real x, real y, real face, real scale, integer variation returns destructable
native KillDestructable         takes destructable d returns nothing
native SetDestructableLife      takes destructable d, real life returns nothing
native SetDestructableAnimation takes destructable d, string whichAnimation returns nothing
native EnumDestructablesInRect  takes rect r, boolexpr filter, code actionFunc returns nothing
native SetDoodadAnimation       takes real x, real y, real radius, integer doodadID, boolean nearestOnly, string animName, boolean animRandom returns nothing

function CreateDestructableLoc takes integer objectid, location loc, real facing, real scale, integer variation returns destructable
function SetDestructableLifePercentBJ takes destructable d, real percent returns nothing
function ModifyGateBJ takes integer gateOperation, destructable d returns nothing
function SetDestAnimationSpeedPercent takes destructable d, real percent returns nothing
```

## Canonical Go surface

```go
type Destructable struct{ /* opaque handle */ }
type DestructableType string

func (g *Game) CreateDestructable(typ DestructableType, pos Vec2, opts ...DestrOption) Destructable
// opts: Facing(a), Scale(s), Variation(n), Dead() (CreateDeadDestructable collapse)

func (d Destructable) Life() float64
func (d Destructable) SetLife(v float64)
func (d Destructable) MaxLife() float64
func (d Destructable) Kill()
func (d Destructable) Resurrect(life float64, playBirth bool)
func (d Destructable) Remove()
func (d Destructable) PlayAnimation(name string, opts ...AnimOption) // + speed percent
func (d Destructable) SetInvulnerable(b bool)
func (d Destructable) SetOccluderHeight(h float64)

func (g *Game) DestructablesIn(r Rect, filter func(Destructable) bool) []Destructable

// Doodads — full WC3 parity via optional handles (D-2026-06-11-13). Doodads stay in
// render-only storage until a script first addresses one; first touch promotes it to a
// lightweight handle-bearing entity, so the zero-cost case stays zero-cost:
type Doodad struct{ /* opaque handle; promoted on first script touch */ }
type DoodadType string

func (g *Game) DoodadsIn(r Rect, typ DoodadType) []Doodad // promotes matches to handles
func (d Doodad) PlayAnimation(name string, opts ...AnimOption)
func (d Doodad) Show(b bool)
func (d Doodad) SetPosition(p Vec2)
func (d Doodad) SetFacing(a Angle)

// SetDoodadAnimation's area-addressed form (the JASS shape) also maps canonically;
// nearestOnly/animRandom flags become options:
func (g *Game) PlayDoodadAnimations(center Vec2, radius float64, typ DoodadType, name string, opts ...DoodadAnimOption)
```

*Revised 2026-06-11 per D-2026-06-11-13* — doodads now get the full scripting surface
(show/hide, animate, reposition), not position-addressed workarounds.

Per R-EXEC-4, `EnumDestructablesInRect` + `GetEnumDestructable` + filter boolexpr
collapse into the slice-returning `DestructablesIn`.

## Dedup rules applied

| Rule | Application | Example |
|---|---|---|
| **D1** | passthroughs dropped | `KillDestructableBJ`, `RemoveDestructableBJ` → methods |
| **D2** | defaulted creates collapse | `CreateDestructableLoc`, `CreateDeadDestructableLocBJ` → `CreateDestructable(..., Dead())` |
| **D3** | Loc/percent variants → value types | `SetDestructableLifePercentBJ` → `SetLife(d.MaxLife()*pct)` — percent variant tombstoned as trivially derivable; XY+Loc enum variants → `DestructablesIn(Rect)` |
| **D4** | gate/elevator state machines kept once | `ModifyGateBJ`/`ChangeElevatorHeight`/`ChangeElevatorWallBlocker` → `helpers.Gate(d).Open()/Close()/Destroy()`, `helpers.Elevator(d).SetHeight(n)` |
| **D5** | invulnerable get/set pair → accessor pair | `SetDestructableInvulnerable`/`IsDestructableInvulnerable` |

## Subsystem dependencies

- **sim** (primary): destructables are static-ish ECS entities with life + pathing blockers; trees feed harvest logic; death updates the pathing grid (R-SIM-5) deterministically. Promoted doodads (D-2026-06-11-13) join the sim as lightweight entities; unpromoted doodads never touch it.
- **render**: model + death animation; occluder height affects the occlusion system (`EnableOcclusion`, see [effects-and-graphics](effects-and-graphics.md)); tree chunks are prime candidates for static mesh merging (R-RND-3).
- **asset**: destructable-type rows in `data/` (life, pathing footprint, model, variations); GLB props from KayKit/Kenney packs.

## Porting hazards

1. **Doodad promotion must be deterministic.** *(Revised 2026-06-11 per D-2026-06-11-13; supersedes the old "doodads have no handles, keep the asymmetry" guidance.)* Doodads default to render-only storage and gain a handle only on first script touch. Promotion is sim-visible state: it must occur in script/sim context (never from the render thread), draw IDs from the deterministic entity allocator, and serialize with saves — a doodad promoted on one client but not another desyncs lockstep (D-5). A promoted doodad with a pathing footprint that is hidden or repositioned must update the pathing grid in-tick, same discipline as hazard 2.
2. **Pathing-grid invalidation**: destructable death/resurrection must update pathfinding *within the same tick*, deterministically ordered with unit movement — classic source of replay divergence.
3. **Tree-harvest coupling**: "is this a tree" is data classification used by AI/harvest orders. Encode in the type table; don't replicate WC3's order-string hack (`HarvestTree`) detection.
4. **Gates/elevators are destructables with magic animation names and pathing side effects** ("open" disables the blocker). The D4 helpers must own both animation and pathing toggles together or maps will desync visually vs logically.
5. **Widget overlap**: destructables participate in `widget` death events and effect attachment — implement the shared `Widget` interface per [units.md hazard 1](units.md).
