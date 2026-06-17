package ai

// AIView read-only query surface (#274; jass-mapping/ai-natives.md AIView +
// information-leakage hazard; execution-model.md §6). The read-only projection
// an AIController sees: own-unit and (fog-honest) other-player unit counts —
// the GetUnitCount* / GetPlayerUnitTypeCount family. Getters only: no mutating
// verb, no Game handle, and no wait verb is reachable from a View, so an AI
// script cannot act through it (R-EXEC-3, the same read-only discipline as the
// §4 query filters / EventView).
//
// Fog-honest by default (porting hazard 3): a query about another player counts
// only the units self can currently see; WithFullVision lifts that as the
// explicit insane-difficulty escape hatch — a conscious construction option,
// never ambient. Own units are always fully counted (you see your own).
//
// Layering note (decision): the concrete projection lives here in litd/ai over
// a narrow UnitQuerySource interface, so the package stays decoupled from
// litd/sim — the sim is adapted to the interface at the integration boundary
// (#281), exactly as the render consumers read sim through small interfaces.
// The litd/api.AIView public boundary (#257) reconciles with this at M5.5; that
// is an integration concern, not a dependency of the query logic.
//
// Counting model: a View counts EXISTING unit entities. "Done" distinguishes a
// completed unit from one still under construction (IsUnderConstruction) — the
// in-progress-vs-completed split the GetUnitCount/GetUnitCountDone pair needs.
// Pre-spawn training-queue depth (units not yet entities) is a production-state
// query owned by the production family (#277), not AIView.

// UnitSnapshot is a read-only, pointerless projection of one unit, the unit of
// data a UnitQuerySource hands the View. No handles; copies by value.
type UnitSnapshot struct {
	Owner  int     // owning player slot, -1 if none
	TypeID int     // unit type id
	Done   bool    // false while under construction (pending), true when complete
	X, Y   float32 // world position, for the fog-visibility check
}

// UnitQuerySource is the live, read-only sim projection a View reads each
// Refresh. The sim is adapted to it (converting entity ids and fixed-point at
// the boundary); the View holds no sim state of its own. AppendUnits must
// return live units in a deterministic order (ascending entity id) so counts
// are reproducible.
type UnitQuerySource interface {
	// AppendUnits appends every live unit's snapshot to dst (ascending id) and
	// returns the grown slice. Reuses dst's capacity (0-alloc when warm).
	AppendUnits(dst []UnitSnapshot) []UnitSnapshot
	// VisibleTo reports whether the world point (x,y) is currently visible to
	// player through the fog of war.
	VisibleTo(player int, x, y float32) bool
	// Tick is the current sim tick (the read-only clock).
	Tick() uint32
}

// View is one AI player's read-only, fog-honest query surface. Bind it to the
// controlled player with NewView; call Refresh once per AI phase so all queries
// in that phase read a single consistent snapshot.
type View struct {
	src        UnitQuerySource
	self       int
	fullVision bool

	scratch []UnitSnapshot // reused source buffer (0-alloc when warm)
	snap    []snapUnit     // the phase snapshot queries read
}

// snapUnit is a unit reduced to what a count needs, plus visibility-to-self
// precomputed at Refresh so queries never recompute fog.
type snapUnit struct {
	owner, typeID int
	done          bool
	visible       bool // to self this phase (own units and full-vision are always true)
}

// ViewOption configures a View at construction.
type ViewOption func(*View)

// WithFullVision builds a View that ignores fog — the insane-difficulty / map
// cheating escape hatch. Explicit by construction; never the default.
func WithFullVision() ViewOption { return func(v *View) { v.fullVision = true } }

// NewView binds a read-only query View for player self, reading src, and takes
// its first snapshot. opts may include WithFullVision.
func NewView(src UnitQuerySource, self int, opts ...ViewOption) *View {
	v := &View{src: src, self: self}
	for _, o := range opts {
		o(v)
	}
	v.Refresh()
	return v
}

// Refresh re-snapshots the world for the current AI phase: it pulls the live
// units and precomputes each one's visibility to self. Every query between two
// Refreshes reads this snapshot, so two queries on one tick agree even if the
// sim mutates between phases. Allocation-free once the buffers are warm.
func (v *View) Refresh() {
	v.scratch = v.src.AppendUnits(v.scratch[:0])
	v.snap = v.snap[:0]
	for i := range v.scratch {
		u := v.scratch[i]
		visible := u.Owner == v.self || v.fullVision || v.src.VisibleTo(v.self, u.X, u.Y)
		v.snap = append(v.snap, snapUnit{owner: u.Owner, typeID: u.TypeID, done: u.Done, visible: visible})
	}
}

// Self returns the player this View belongs to.
func (v *View) Self() int { return v.self }

// Now returns the current sim tick.
func (v *View) Now() uint32 { return v.src.Tick() }

// FullVision reports whether this View ignores fog.
func (v *View) FullVision() bool { return v.fullVision }

// count is the shared counting kernel over the phase snapshot.
func (v *View) count(player, typeID int, fogHonest, doneOnly bool) int {
	n := 0
	for i := range v.snap {
		u := &v.snap[i]
		if u.owner != player || u.typeID != typeID {
			continue
		}
		if doneOnly && !u.done {
			continue
		}
		if fogHonest && !u.visible {
			continue
		}
		n++
	}
	return n
}

// OwnUnitCount counts the AI player's own units of typeID, including those still
// under construction (WC3 GetUnitCount). Own units are never fogged.
func (v *View) OwnUnitCount(typeID int) int { return v.count(v.self, typeID, false, false) }

// OwnUnitCountDone counts only the AI player's completed units of typeID,
// excluding ones under construction (WC3 GetUnitCountDone).
func (v *View) OwnUnitCountDone(typeID int) int { return v.count(v.self, typeID, false, true) }

// PlayerUnitCount counts player's units of typeID that self may legally see:
// fog-honest unless the View was built WithFullVision. For player == self it is
// the same as OwnUnitCount (own units are always visible).
func (v *View) PlayerUnitCount(player, typeID int) int {
	return v.count(player, typeID, player != v.self, false)
}

// UnitCount satisfies the litd/ai.AIView contract — a fog-honest count of
// player's units of typeID.
func (v *View) UnitCount(player, typeID int) int { return v.PlayerUnitCount(player, typeID) }
