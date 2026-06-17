package ai_test

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// ---------------------------------------------------------------------------
// Part A — View counting logic FSV against a CONTROLLED source.
//
// The unit under test is the View. Its source of truth is the exact set of
// units and the exact visibility predicate I hand it — known input, known
// expected count (X+X=Y). The real sim is exercised in Part B; here every
// count is checked against a hand-computed golden table so a counting bug can
// hide nowhere.
// ---------------------------------------------------------------------------

// ctrlSource is a controlled UnitQuerySource: a fixed unit list, a tick, and a
// visibility predicate I dictate. AppendUnits is allocation-free (copies into
// the caller's buffer), so it also backs the zero-alloc check.
type ctrlSource struct {
	units   []ai.UnitSnapshot
	tick    uint32
	visible func(player int, x, y float32) bool
}

func (s *ctrlSource) AppendUnits(dst []ai.UnitSnapshot) []ai.UnitSnapshot {
	return append(dst, s.units...)
}
func (s *ctrlSource) VisibleTo(player int, x, y float32) bool {
	if s.visible == nil {
		return false
	}
	return s.visible(player, x, y)
}
func (s *ctrlSource) Tick() uint32 { return s.tick }

const (
	footman = 100 // a type id
	knight  = 200
	grunt   = 300 // enemy type
)

// TestViewOwnCountsIgnoreFogFSV — own units are always counted, fog or not, and
// OwnUnitCount includes under-construction units while OwnUnitCountDone excludes
// them. Golden table, self=1.
func TestViewOwnCountsIgnoreFogFSV(t *testing.T) {
	src := &ctrlSource{
		tick: 7,
		// self=1: 2 footmen done, 1 footman under construction, 1 knight done.
		units: []ai.UnitSnapshot{
			{Owner: 1, TypeID: footman, Done: true, X: 0, Y: 0},
			{Owner: 1, TypeID: footman, Done: true, X: 1, Y: 0},
			{Owner: 1, TypeID: footman, Done: false, X: 2, Y: 0}, // rising
			{Owner: 1, TypeID: knight, Done: true, X: 3, Y: 0},
		},
		visible: func(int, float32, float32) bool { return false }, // total fog
	}
	v := ai.NewView(src, 1)
	t.Logf("FSV self=1 tick=%d footmen all=%d done=%d | knights all=%d done=%d",
		v.Now(), v.OwnUnitCount(footman), v.OwnUnitCountDone(footman),
		v.OwnUnitCount(knight), v.OwnUnitCountDone(knight))
	if v.Now() != 7 {
		t.Fatalf("Now=%d want 7", v.Now())
	}
	if got := v.OwnUnitCount(footman); got != 3 {
		t.Fatalf("OwnUnitCount(footman)=%d want 3 (own units never fogged)", got)
	}
	if got := v.OwnUnitCountDone(footman); got != 2 {
		t.Fatalf("OwnUnitCountDone(footman)=%d want 2 (rising one excluded)", got)
	}
	if got := v.OwnUnitCount(knight); got != 1 {
		t.Fatalf("OwnUnitCount(knight)=%d want 1", got)
	}
	if got := v.OwnUnitCount(grunt); got != 0 {
		t.Fatalf("OwnUnitCount(grunt)=%d want 0 (own none)", got)
	}
}

// TestViewEnemyFogHonestVsFullVisionFSV — mandated edge 1. An enemy unit
// standing in fog is excluded from a default (fog-honest) View and included by
// a WithFullVision View. Same source, two Views, two golden counts.
func TestViewEnemyFogHonestVsFullVisionFSV(t *testing.T) {
	// self=1, enemy=2 has two grunts: one at x=10 (visible), one at x=99 (fog).
	src := &ctrlSource{
		tick: 0,
		units: []ai.UnitSnapshot{
			{Owner: 2, TypeID: grunt, Done: true, X: 10, Y: 0},
			{Owner: 2, TypeID: grunt, Done: true, X: 99, Y: 0},
		},
		// player 1 can see only the x<50 half of the map.
		visible: func(player int, x, y float32) bool { return player == 1 && x < 50 },
	}
	fog := ai.NewView(src, 1)
	full := ai.NewView(src, 1, ai.WithFullVision())
	t.Logf("FSV enemy grunts: fog-honest=%d (want 1, far one hidden) full-vision=%d (want 2)",
		fog.PlayerUnitCount(2, grunt), full.PlayerUnitCount(2, grunt))
	if got := fog.PlayerUnitCount(2, grunt); got != 1 {
		t.Fatalf("fog-honest enemy count=%d want 1 (fogged grunt must be excluded)", got)
	}
	if got := full.PlayerUnitCount(2, grunt); got != 2 {
		t.Fatalf("full-vision enemy count=%d want 2 (cheat sees through fog)", got)
	}
	if fog.FullVision() {
		t.Fatal("default View must NOT have full vision (fog is the default)")
	}
	if !full.FullVision() {
		t.Fatal("WithFullVision View must report FullVision()")
	}
	// UnitCount (the litd/ai.AIView contract method) is fog-honest like the default.
	if got := fog.UnitCount(2, grunt); got != 1 {
		t.Fatalf("UnitCount=%d want 1 (AIView contract is fog-honest)", got)
	}
}

// TestViewSeenThenFoggedFSV — mandated edge 2. A unit revealed on one phase and
// then lost to fog on the next must drop out of the fog-honest count after the
// next Refresh: a View reflects CURRENT vision, it does not remember.
// (Last-seen "ghost" memory is the sim's lastSeen concern, not AIView's.)
func TestViewSeenThenFoggedFSV(t *testing.T) {
	seen := true
	src := &ctrlSource{
		units:   []ai.UnitSnapshot{{Owner: 2, TypeID: grunt, Done: true, X: 60, Y: 0}},
		visible: func(player int, x, y float32) bool { return seen },
	}
	v := ai.NewView(src, 1)
	before := v.PlayerUnitCount(2, grunt)
	seen = false  // the grunt walks into fog (or our scout dies)
	v.Refresh()   // next AI phase re-snapshots
	after := v.PlayerUnitCount(2, grunt)
	t.Logf("FSV enemy grunt seen→fogged: count before=%d (want 1) after Refresh=%d (want 0)", before, after)
	if before != 1 || after != 0 {
		t.Fatalf("seen-then-fogged: before=%d after=%d want 1/0 (View tracks current vision only)", before, after)
	}
}

// TestViewSnapshotConsistencyOnDeathFSV — mandated edge 3. Two queries in the
// same AI phase read one consistent snapshot: if an own unit dies in the middle
// of a phase, both queries still agree (the View froze the snapshot at Refresh).
// Only the next Refresh observes the death. This is the within-phase
// consistency guarantee that keeps a controller's decisions coherent.
func TestViewSnapshotConsistencyOnDeathFSV(t *testing.T) {
	src := &ctrlSource{
		units: []ai.UnitSnapshot{
			{Owner: 1, TypeID: footman, Done: true},
			{Owner: 1, TypeID: footman, Done: true},
			{Owner: 1, TypeID: footman, Done: true},
		},
		visible: func(int, float32, float32) bool { return true },
	}
	v := ai.NewView(src, 1) // snapshot taken: 3 footmen
	q1 := v.OwnUnitCount(footman)
	// A footman dies mid-phase — the live source now has 2, but we do NOT Refresh.
	src.units = src.units[:2]
	q2 := v.OwnUnitCount(footman)
	t.Logf("FSV mid-phase own death: query1=%d query2=%d (same snapshot, want 3/3)", q1, q2)
	if q1 != 3 || q2 != 3 {
		t.Fatalf("within-phase queries disagree: %d then %d, want 3/3 (snapshot must be frozen)", q1, q2)
	}
	v.Refresh() // next phase
	q3 := v.OwnUnitCount(footman)
	t.Logf("FSV after Refresh: query3=%d (want 2, death now visible)", q3)
	if q3 != 2 {
		t.Fatalf("post-Refresh count=%d want 2 (death must be observed next phase)", q3)
	}
}

// TestViewEmptyPlayerZeroFSV — mandated edge 4. Queries against a player with no
// units (or a defeated/never-present slot) return 0 and never panic, including
// an entirely empty world.
func TestViewEmptyPlayerZeroFSV(t *testing.T) {
	// Empty world.
	empty := &ctrlSource{visible: func(int, float32, float32) bool { return true }}
	ve := ai.NewView(empty, 1)
	t.Logf("FSV empty world: own=%d enemy=%d", ve.OwnUnitCount(footman), ve.PlayerUnitCount(2, grunt))
	if ve.OwnUnitCount(footman) != 0 || ve.PlayerUnitCount(2, grunt) != 0 {
		t.Fatalf("empty world counts not zero: own=%d enemy=%d", ve.OwnUnitCount(footman), ve.PlayerUnitCount(2, grunt))
	}
	// Populated world, but query a defeated player (7) that holds nothing.
	src := &ctrlSource{
		units:   []ai.UnitSnapshot{{Owner: 1, TypeID: footman, Done: true}},
		visible: func(int, float32, float32) bool { return true },
	}
	v := ai.NewView(src, 1)
	t.Logf("FSV defeated player 7: count=%d (want 0)", v.PlayerUnitCount(7, footman))
	if v.PlayerUnitCount(7, footman) != 0 {
		t.Fatalf("defeated-player count=%d want 0", v.PlayerUnitCount(7, footman))
	}
	// Unknown type id for an existing player → 0.
	if v.OwnUnitCount(99999) != 0 {
		t.Fatalf("unknown-type count=%d want 0", v.OwnUnitCount(99999))
	}
}

// TestViewDeterminismFSV — the same source queried twice yields a byte-identical
// count vector (the #274 "determinism hash across 2 runs" requirement). The
// View is pure over its snapshot, so determinism is structural; this guards
// against a future map-iteration or ordering regression sneaking in.
func TestViewDeterminismFSV(t *testing.T) {
	mkSrc := func() *ctrlSource {
		return &ctrlSource{
			tick: 3,
			units: []ai.UnitSnapshot{
				{Owner: 1, TypeID: footman, Done: true, X: 1, Y: 1},
				{Owner: 1, TypeID: knight, Done: false, X: 2, Y: 2},
				{Owner: 2, TypeID: grunt, Done: true, X: 5, Y: 5},
				{Owner: 2, TypeID: grunt, Done: true, X: 60, Y: 5},
			},
			visible: func(player int, x, y float32) bool { return x < 50 },
		}
	}
	hash := func() string {
		v := ai.NewView(mkSrc(), 1)
		return fmt.Sprintf("own_f=%d own_f_done=%d own_k=%d enemy_g=%d enemy_g_full=%d",
			v.OwnUnitCount(footman), v.OwnUnitCountDone(footman), v.OwnUnitCount(knight),
			v.PlayerUnitCount(2, grunt),
			ai.NewView(mkSrc(), 1, ai.WithFullVision()).PlayerUnitCount(2, grunt))
	}
	h1, h2 := hash(), hash()
	t.Logf("FSV run1=%q\nFSV run2=%q", h1, h2)
	if h1 != h2 {
		t.Fatalf("count vector not deterministic:\n %q\n %q", h1, h2)
	}
	// Pin the golden values so the determinism check can't pass on a wrong-but-stable bug.
	want := "own_f=1 own_f_done=1 own_k=1 enemy_g=1 enemy_g_full=2"
	if h1 != want {
		t.Fatalf("count vector=%q want %q", h1, want)
	}
}

// TestViewZeroAllocFSV — a Refresh + a batch of queries allocates nothing at
// steady state (R-GC-1): the snapshot buffers are reused across phases.
func TestViewZeroAllocFSV(t *testing.T) {
	src := &ctrlSource{
		units: []ai.UnitSnapshot{
			{Owner: 1, TypeID: footman, Done: true, X: 1},
			{Owner: 1, TypeID: footman, Done: false, X: 2},
			{Owner: 2, TypeID: grunt, Done: true, X: 5},
		},
		visible: func(player int, x, y float32) bool { return x < 50 },
	}
	v := ai.NewView(src, 1)
	allocs := testing.AllocsPerRun(500, func() {
		v.Refresh()
		_ = v.OwnUnitCount(footman)
		_ = v.OwnUnitCountDone(footman)
		_ = v.PlayerUnitCount(2, grunt)
	})
	t.Logf("FSV refresh+query allocs/op=%v", allocs)
	if allocs != 0 {
		t.Fatalf("Refresh+queries allocate %v/op at steady state, want 0", allocs)
	}
}

// ---------------------------------------------------------------------------
// Part B — Integration FSV against a REAL sim.World with REAL fog of war.
//
// SoT = the sim's own stores (Owners/UnitTypes/Transforms) and its fog grid
// (IsVisibleToPlayer). The adapter projects those into UnitSnapshots; we then
// cross-check the View's counts against ground truth computed straight from the
// sim, proving the projection is faithful (not a second bookkeeping copy).
// ---------------------------------------------------------------------------

// worldSource adapts *sim.World to ai.UnitQuerySource using only the public sim
// API. Positions are placed at cell centers (integer world units), so the
// fixed→float→fixed round-trip through Floor()/FromInt is exact.
type worldSource struct {
	w   *sim.World
	ids []sim.EntityID // reused scratch (alloc-free AppendUnits when warm)
}

func (s *worldSource) AppendUnits(dst []ai.UnitSnapshot) []ai.UnitSnapshot {
	s.ids = s.w.AppendAllUnits(s.ids[:0]) // ascending entity id — deterministic
	for _, id := range s.ids {
		or := s.w.Owners.Row(id)
		ur := s.w.UnitTypes.Row(id)
		tr := s.w.Transforms.Row(id)
		owner := -1
		if or != -1 {
			owner = int(s.w.Owners.Player[or])
		}
		typeID := 0
		if ur != -1 {
			typeID = int(s.w.UnitTypes.TypeID[ur])
		}
		var x, y float32
		if tr != -1 {
			p := s.w.Transforms.Pos[tr]
			x, y = float32(p.X.Floor()), float32(p.Y.Floor())
		}
		dst = append(dst, ai.UnitSnapshot{
			Owner:  owner,
			TypeID: typeID,
			Done:   !s.w.IsUnderConstruction(id), // SoT for done/pending
			X:      x, Y: y,
		})
	}
	return dst
}

func (s *worldSource) VisibleTo(player int, x, y float32) bool {
	return s.w.IsVisibleToPlayer(uint8(player), fixed.Vec2{X: fixed.FromInt(int32(x)), Y: fixed.FromInt(int32(y))})
}

func (s *worldSource) Tick() uint32 { return s.w.Tick() }

// fogWorld builds a fully-walkable world with one unit def "scout" that has
// daytime sight, time frozen at noon. Mirrors litd/sim's own fog test recipe,
// using only exported API.
func fogWorld(t *testing.T) *sim.World {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 64})
	g := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.SetFlags(x, y, path.Walkable|path.Buildable|path.Flyable)
		}
	}
	w.SetGrid(g)
	// type id 0 = "scout" with sight; one entry is enough — all units share it
	// (we vary OWNER, and place enemies near/far from the scout to drive fog).
	if !w.BindUnitDefs([]data.Unit{
		{ID: "scout", Life: 100, SightDay: fixed.FromInt(360), SightNight: fixed.FromInt(160), CollisionSize: 16, Pathing: data.PathingGround},
	}) {
		t.Fatal("BindUnitDefs failed")
	}
	w.SetTimeOfDay(12 * fixed.One)
	w.SuspendTimeOfDay(true)
	return w
}

func spawnAt(t *testing.T, w *sim.World, player, team uint8, typeID uint16, cellX, cellY int32) sim.EntityID {
	t.Helper()
	id, ok := w.CreateUnit(sim.CellCenter(cellY*path.GridSize+cellX), 0)
	if !ok ||
		!w.Owners.Add(w.Ents, id, player, team, player) ||
		!w.UnitTypes.Add(w.Ents, id, typeID) ||
		!w.Collisions.Add(w.Ents, id, 1, sim.PathGround) ||
		!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) {
		t.Fatalf("spawn failed id=%d", id)
	}
	return id
}

// TestViewRealFogIntegrationFSV — the headline #274 scenario. AI player 1 has a
// scout at (10,10). Enemy player 2 has one grunt right next to the scout
// (revealed) and one grunt far across the map (fogged). The fog-honest View
// must count exactly the revealed enemy; WithFullVision counts both; and every
// count is cross-checked against ground truth read straight from the sim.
func TestViewRealFogIntegrationFSV(t *testing.T) {
	w := fogWorld(t)
	const ai1, enemy2 uint8 = 1, 2
	const scoutType, gruntType uint16 = 0, 0 // share the one def; owner distinguishes

	// AI player 1: a scout near the enemy, plus a second own unit placed FAR
	// away (cell 50,10) so the scout is the SOLE vision source over the near
	// enemy — killing the scout later must dark that area.
	scout := spawnAt(t, w, ai1, ai1, scoutType, 10, 10)
	spawnAt(t, w, ai1, ai1, scoutType, 50, 10)
	// Enemy player 2: one grunt adjacent to the scout (within sight=360 ≈ 11 cells),
	// one grunt far away (cell 60,60 — well outside sight).
	near := spawnAt(t, w, enemy2, enemy2, gruntType, 12, 10)
	far := spawnAt(t, w, enemy2, enemy2, gruntType, 60, 60)
	w.RecomputeVisibility()

	// Ground truth straight from the sim (the SoT), before the View runs.
	nearPos := w.Transforms.Pos[w.Transforms.Row(near)]
	farPos := w.Transforms.Pos[w.Transforms.Row(far)]
	nearVis := w.IsVisibleToPlayer(ai1, nearPos)
	farVis := w.IsVisibleToPlayer(ai1, farPos)
	t.Logf("FSV sim SoT: scout=%d near-enemy@(%d,%d) visible=%v | far-enemy@(%d,%d) visible=%v",
		scout, nearPos.X.Floor(), nearPos.Y.Floor(), nearVis,
		farPos.X.Floor(), farPos.Y.Floor(), farVis)
	if !nearVis || farVis {
		t.Fatalf("fog precondition wrong: nearVis=%v (want true) farVis=%v (want false)", nearVis, farVis)
	}

	src := &worldSource{w: w}
	fog := ai.NewView(src, int(ai1))
	full := ai.NewView(src, int(ai1), ai.WithFullVision())

	t.Logf("FSV View counts: own=%d (want 2) | enemy fog-honest=%d (want 1) | enemy full-vision=%d (want 2)",
		fog.OwnUnitCount(int(scoutType)), fog.PlayerUnitCount(int(enemy2), int(gruntType)),
		full.PlayerUnitCount(int(enemy2), int(gruntType)))

	if got := fog.OwnUnitCount(int(scoutType)); got != 2 {
		t.Fatalf("own count=%d want 2", got)
	}
	if got := fog.PlayerUnitCount(int(enemy2), int(gruntType)); got != 1 {
		t.Fatalf("fog-honest enemy count=%d want 1 (far grunt is in fog)", got)
	}
	if got := full.PlayerUnitCount(int(enemy2), int(gruntType)); got != 2 {
		t.Fatalf("full-vision enemy count=%d want 2 (cheats through fog)", got)
	}
	if v := fog.Now(); v != w.Tick() {
		t.Fatalf("View.Now=%d != sim tick %d", v, w.Tick())
	}

	// Cross-check: count enemy-2 units the sim itself says player 1 can see —
	// must equal the fog-honest View count (the projection is faithful).
	var truthVisible int
	var allIDs []sim.EntityID
	allIDs = w.AppendAllUnits(allIDs)
	for _, id := range allIDs {
		or := w.Owners.Row(id)
		if or == -1 || w.Owners.Player[or] != enemy2 {
			continue
		}
		if w.IsVisibleToPlayer(ai1, w.Transforms.Pos[w.Transforms.Row(id)]) {
			truthVisible++
		}
	}
	t.Logf("FSV cross-check: sim says player 1 sees %d enemy-2 units; View fog-honest count=%d",
		truthVisible, fog.PlayerUnitCount(int(enemy2), int(gruntType)))
	if truthVisible != fog.PlayerUnitCount(int(enemy2), int(gruntType)) {
		t.Fatalf("View count %d disagrees with sim ground truth %d", fog.PlayerUnitCount(int(enemy2), int(gruntType)), truthVisible)
	}

	// Edge 2 realized against the real sim: destroy the scout, recompute fog,
	// the near enemy falls back into fog → fog-honest count drops to 0.
	// (DestroyUnit removes the row immediately; KillUnit defers to a tick phase.)
	w.DestroyUnit(scout)
	w.RecomputeVisibility()
	fog.Refresh()
	t.Logf("FSV after scout dies + Refresh: near-enemy visible=%v, enemy fog-honest count=%d (want 0)",
		w.IsVisibleToPlayer(ai1, nearPos), fog.PlayerUnitCount(int(enemy2), int(gruntType)))
	if got := fog.PlayerUnitCount(int(enemy2), int(gruntType)); got != 0 {
		t.Fatalf("after losing the scout, enemy count=%d want 0 (everything fogged)", got)
	}
	// Own count drops by the dead scout (snapshot refreshed).
	if got := fog.OwnUnitCount(int(scoutType)); got != 1 {
		t.Fatalf("own count after scout death=%d want 1", got)
	}
}
