package ai_test

import (
	"fmt"
	"hash/fnv"
	"sort"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// ---------------------------------------------------------------------------
// Part A — wave STATE MACHINE FSV against a controlled source.
//
// The WaveManager is the unit under test. The source of truth is the exact
// roster + member positions I dictate and the orders the manager emits — known
// input, known transitions (X+X=Y). Real orders/combat are exercised in Part B.
// ---------------------------------------------------------------------------

// fakeWaveSource is a controlled WaveSource: a fixed eligible-unit list per
// type, member positions I move by hand, a death set, and recorders for the
// move/attack orders the manager issues. Maps are fine here — this is test
// scaffolding, not gameplay code.
type fakeWaveSource struct {
	byType map[int][]int32   // typeID → eligible ids (kept ascending)
	pos    map[int32][2]int32
	dead   map[int32]bool
	moved  map[int32][2]int32 // last OrderMoveTo target
	atk    map[int32][2]int32 // last OrderAttackTo target
}

func newFakeWaveSource() *fakeWaveSource {
	return &fakeWaveSource{
		byType: map[int][]int32{},
		pos:    map[int32][2]int32{},
		dead:   map[int32]bool{},
		moved:  map[int32][2]int32{},
		atk:    map[int32][2]int32{},
	}
}

func (s *fakeWaveSource) add(typeID int, id, x, y int32) {
	s.byType[typeID] = append(s.byType[typeID], id)
	sort.Slice(s.byType[typeID], func(i, j int) bool { return s.byType[typeID][i] < s.byType[typeID][j] })
	s.pos[id] = [2]int32{x, y}
}

func (s *fakeWaveSource) EligibleUnits(player, typeID int, dst []int32) []int32 {
	for _, id := range s.byType[typeID] { // ascending
		if !s.dead[id] {
			dst = append(dst, id)
		}
	}
	return dst
}
func (s *fakeWaveSource) UnitPos(id int32) (int32, int32, bool) {
	if s.dead[id] {
		return 0, 0, false
	}
	p, ok := s.pos[id]
	return p[0], p[1], ok
}
func (s *fakeWaveSource) OrderMoveTo(id, x, y int32)   { s.moved[id] = [2]int32{x, y} }
func (s *fakeWaveSource) OrderAttackTo(id, x, y int32) { s.atk[id] = [2]int32{x, y} }

const stSoldier = 0

// TestAttackWavesLifecycleFSV — compose → gather → launch, with the launch
// gated on FORMATION (not the deadline). Two runs produce an identical
// lifecycle trace (transition ticks + member ids).
func TestAttackWavesLifecycleFSV(t *testing.T) {
	run := func(log bool) (string, uint32) {
		src := newFakeWaveSource()
		// 3 soldiers far from the gather point (200,200); patience 10000 so the
		// launch can only come from forming up.
		src.add(stSoldier, 10, 0, 0)
		src.add(stSoldier, 11, 500, 0)
		src.add(stSoldier, 12, 0, 500)
		mgr := ai.NewWaveManager(src, 64 /*radius*/, 10000 /*patience*/)

		id := mgr.Stage(1, 200, 200, 900, 900, []ai.Quota{{TypeID: stSoldier, Count: 3}}, 0)
		if id == 0 {
			t.Fatal("Stage returned 0 with 3 eligible units")
		}
		w, _ := mgr.WaveByID(id)
		if log {
			t.Logf("FSV t0 STAGE wave=%d state=%d members=%v (move-to-gather issued: %v)", id, w.State, w.Members, src.moved)
		}
		if w.State != ai.WaveGathering || len(w.Members) != 3 {
			t.Fatalf("post-stage state=%d members=%v want gathering/3", w.State, w.Members)
		}

		var trace string
		var launchTick uint32
		for tk := uint32(1); tk <= 12; tk++ {
			// Units crawl toward the gather point: at tick 5 they all arrive.
			if tk == 5 {
				src.pos[10] = [2]int32{200, 200}
				src.pos[11] = [2]int32{210, 200}
				src.pos[12] = [2]int32{200, 215}
			}
			prev := mgr.WaveState(id)
			mgr.Tick(tk)
			now := mgr.WaveState(id)
			if prev != now {
				trace += fmt.Sprintf("t%d:%d→%d ", tk, prev, now)
				if now == ai.WaveLaunched {
					launchTick = tk
				}
			}
		}
		if log {
			ww, _ := mgr.WaveByID(id)
			t.Logf("FSV transitions: %s", trace)
			t.Logf("FSV launch tick=%d attack-move targets issued: %v", launchTick, src.atk)
			t.Logf("FSV final wave state=%d members=%v", ww.State, ww.Members)
		}
		return trace, launchTick
	}

	tr1, lt1 := run(true)
	tr2, lt2 := run(false)
	if lt1 != 5 {
		t.Fatalf("launch tick=%d want 5 (formed, not deadline)", lt1)
	}
	if tr1 != tr2 || lt1 != lt2 {
		t.Fatalf("lifecycle not deterministic:\n run1=%q (launch %d)\n run2=%q (launch %d)", tr1, lt1, tr2, lt2)
	}
	// Confirm the launch issued an attack-move to the target for every member.
	src := newFakeWaveSource()
	src.add(stSoldier, 10, 200, 200)
	mgr := ai.NewWaveManager(src, 64, 0)
	id := mgr.Stage(1, 200, 200, 900, 900, []ai.Quota{{TypeID: stSoldier, Count: 1}}, 0)
	mgr.Tick(1) // formed (already at gather) → launch
	if got := src.atk[10]; got != [2]int32{900, 900} {
		t.Fatalf("launched member attack-move target=%v want (900,900)", got)
	}
	if mgr.WaveState(id) != ai.WaveLaunched {
		t.Fatalf("state=%d want launched", mgr.WaveState(id))
	}
}

// TestAttackWavesMemberDiesDuringGatherFSV — edge 1. A member lost during
// gather is pruned; the wave launches with the survivors (partial-wave launch).
// And if the whole roster dies before launch, the wave aborts (no launch).
func TestAttackWavesMemberDiesDuringGatherFSV(t *testing.T) {
	src := newFakeWaveSource()
	src.add(stSoldier, 10, 200, 200) // already at gather
	src.add(stSoldier, 11, 200, 200)
	src.add(stSoldier, 12, 200, 200)
	mgr := ai.NewWaveManager(src, 64, 10000)
	id := mgr.Stage(1, 200, 200, 900, 900, []ai.Quota{{TypeID: stSoldier, Count: 3}}, 0)
	w0, _ := mgr.WaveByID(id)
	t.Logf("FSV staged members=%v", w0.Members)

	// Kill member 11 before the formation Tick.
	src.dead[11] = true
	mgr.Tick(1)
	w1, _ := mgr.WaveByID(id)
	t.Logf("FSV after death-of-11 + Tick: state=%d members=%v atk-issued=%v", w1.State, w1.Members, src.atk)
	if w1.State != ai.WaveLaunched {
		t.Fatalf("state=%d want launched (survivors form up)", w1.State)
	}
	if len(w1.Members) != 2 || w1.Members[0] != 10 || w1.Members[1] != 12 {
		t.Fatalf("survivors=%v want [10 12] (dead member pruned)", w1.Members)
	}
	if _, launched11 := src.atk[11]; launched11 {
		t.Fatal("dead member 11 was issued an attack-move (should be pruned)")
	}

	// Now the abort case: a fresh wave whose entire roster dies before launch.
	src2 := newFakeWaveSource()
	src2.add(stSoldier, 20, 999, 999) // far → not formed yet
	src2.add(stSoldier, 21, 999, 999)
	mgr2 := ai.NewWaveManager(src2, 8, 10000)
	id2 := mgr2.Stage(1, 0, 0, 900, 900, []ai.Quota{{TypeID: stSoldier, Count: 2}}, 0)
	src2.dead[20] = true
	src2.dead[21] = true
	mgr2.Tick(1)
	t.Logf("FSV roster-wiped wave: state=%d (want done/aborted) atk-issued=%v", mgr2.WaveState(id2), src2.atk)
	if mgr2.WaveState(id2) != ai.WaveDone {
		t.Fatalf("state=%d want WaveDone (aborted, no survivors)", mgr2.WaveState(id2))
	}
	if len(src2.atk) != 0 {
		t.Fatalf("aborted wave issued attack-moves: %v", src2.atk)
	}
}

// TestAttackWavesZeroEligibleNoOpFSV — edge 2. A stage that finds no eligible
// units creates no wave and returns 0.
func TestAttackWavesZeroEligibleNoOpFSV(t *testing.T) {
	src := newFakeWaveSource() // empty
	mgr := ai.NewWaveManager(src, 64, 100)
	id := mgr.Stage(1, 0, 0, 9, 9, []ai.Quota{{TypeID: stSoldier, Count: 5}}, 0)
	t.Logf("FSV stage with 0 eligible → id=%d waveCount=%d", id, mgr.WaveCount())
	if id != 0 || mgr.WaveCount() != 0 {
		t.Fatalf("zero-eligible stage made a wave: id=%d count=%d", id, mgr.WaveCount())
	}
}

// TestAttackWavesSaveMidGatherFSV — edge 3. A mid-gather wave round-trips
// through Save/Load byte-faithfully, so the restored manager launches on the
// identical (absolute, deadline-driven) tick with the identical roster.
func TestAttackWavesSaveMidGatherFSV(t *testing.T) {
	build := func() (*ai.WaveManager, *fakeWaveSource, uint32) {
		src := newFakeWaveSource()
		src.add(stSoldier, 10, 999, 999) // far → never forms; only the deadline launches
		src.add(stSoldier, 11, 999, 999)
		mgr := ai.NewWaveManager(src, 4 /*tiny radius*/, 8 /*deadline = stage+8*/)
		id := mgr.Stage(1, 0, 0, 700, 700, []ai.Quota{{TypeID: stSoldier, Count: 2}}, 0)
		return mgr, src, id
	}

	// Reference run: no save, launches at the deadline tick.
	refMgr, refSrc, refID := build()
	var refLaunch uint32
	for tk := uint32(1); tk <= 12; tk++ {
		before := refMgr.WaveState(refID)
		refMgr.Tick(tk)
		if before == ai.WaveGathering && refMgr.WaveState(refID) == ai.WaveLaunched {
			refLaunch = tk
		}
	}
	_ = refSrc
	t.Logf("FSV reference launch tick=%d", refLaunch)

	// Saved run: advance a few ticks mid-gather, save, restore into a fresh
	// manager, then drive to launch.
	mgr, src, id := build()
	for tk := uint32(1); tk <= 3; tk++ { // mid-gather, before the deadline
		mgr.Tick(tk)
	}
	if mgr.WaveState(id) != ai.WaveGathering {
		t.Fatalf("pre-save state=%d want still gathering", mgr.WaveState(id))
	}
	blob := mgr.Save(nil)
	wBefore, _ := mgr.WaveByID(id)
	t.Logf("FSV saved %d bytes mid-gather: wave=%d state=%d members=%v deadline=%d", len(blob), id, wBefore.State, wBefore.Members, wBefore.Deadline)

	mgr2 := ai.NewWaveManager(src, 4, 8)
	if err := mgr2.Load(blob); err != nil {
		t.Fatalf("Load: %v", err)
	}
	wAfter, ok := mgr2.WaveByID(id)
	if !ok {
		t.Fatal("restored manager lost the wave")
	}
	t.Logf("FSV restored: wave=%d state=%d members=%v deadline=%d", id, wAfter.State, wAfter.Members, wAfter.Deadline)
	if wAfter.State != wBefore.State || wAfter.Deadline != wBefore.Deadline ||
		len(wAfter.Members) != len(wBefore.Members) {
		t.Fatalf("restored wave differs: %+v vs %+v", wAfter, wBefore)
	}
	for i := range wBefore.Members {
		if wAfter.Members[i] != wBefore.Members[i] {
			t.Fatalf("restored members %v != %v", wAfter.Members, wBefore.Members)
		}
	}

	var restoredLaunch uint32
	for tk := uint32(4); tk <= 12; tk++ {
		before := mgr2.WaveState(id)
		mgr2.Tick(tk)
		if before == ai.WaveGathering && mgr2.WaveState(id) == ai.WaveLaunched {
			restoredLaunch = tk
		}
	}
	t.Logf("FSV restored launch tick=%d (want == reference %d)", restoredLaunch, refLaunch)
	if restoredLaunch != refLaunch || refLaunch != 8 {
		t.Fatalf("launch ticks differ: restored=%d reference=%d (want 8)", restoredLaunch, refLaunch)
	}
}

// TestAttackWavesSaveFailClosedFSV — a corrupt or truncated blob is rejected and
// leaves the manager untouched (fail-closed deserialization).
func TestAttackWavesSaveFailClosedFSV(t *testing.T) {
	src := newFakeWaveSource()
	src.add(stSoldier, 10, 0, 0)
	mgr := ai.NewWaveManager(src, 8, 100)
	mgr.Stage(1, 0, 0, 9, 9, []ai.Quota{{TypeID: stSoldier, Count: 1}}, 0)
	before := mgr.WaveCount()

	if err := mgr.Load([]byte("XXXX")); err == nil {
		t.Fatal("Load accepted a too-short blob")
	}
	bad := mgr.Save(nil)
	bad[0] = 'Z' // corrupt the magic
	if err := mgr.Load(bad); err == nil {
		t.Fatal("Load accepted a bad-magic blob")
	}
	good := mgr.Save(nil)
	if err := mgr.Load(good[:len(good)-3]); err == nil {
		t.Fatal("Load accepted a truncated member list")
	}
	if mgr.WaveCount() != before {
		t.Fatalf("failed Load mutated the manager: count %d != %d", mgr.WaveCount(), before)
	}
	t.Logf("FSV fail-closed: bad magic / short / truncated all rejected; manager unchanged (count=%d)", mgr.WaveCount())
}

// TestAttackWavesTwoSimultaneousDisjointFSV — edge 4. Two waves staged from the
// same unit pool draw disjoint members; no unit appears in both.
func TestAttackWavesTwoSimultaneousDisjointFSV(t *testing.T) {
	src := newFakeWaveSource()
	for _, id := range []int32{10, 11, 12, 13} {
		src.add(stSoldier, id, 0, 0)
	}
	mgr := ai.NewWaveManager(src, 64, 100)
	id1 := mgr.Stage(1, 0, 0, 9, 9, []ai.Quota{{TypeID: stSoldier, Count: 2}}, 0)
	id2 := mgr.Stage(1, 5, 5, 8, 8, []ai.Quota{{TypeID: stSoldier, Count: 2}}, 0)
	w1, _ := mgr.WaveByID(id1)
	w2, _ := mgr.WaveByID(id2)
	t.Logf("FSV wave1 members=%v wave2 members=%v", w1.Members, w2.Members)
	if fmt.Sprint(w1.Members) != "[10 11]" || fmt.Sprint(w2.Members) != "[12 13]" {
		t.Fatalf("membership not the deterministic split: w1=%v w2=%v want [10 11]/[12 13]", w1.Members, w2.Members)
	}
	seen := map[int32]bool{}
	for _, m := range append(append([]int32{}, w1.Members...), w2.Members...) {
		if seen[m] {
			t.Fatalf("unit %d is in both waves", m)
		}
		seen[m] = true
	}
	t.Logf("FSV two waves hold disjoint membership (no overlap)")
}

// ---------------------------------------------------------------------------
// Part B — integration FSV against a real sim.World: a wave launched at a
// static defense, resolved by the sim's order + combat systems.
// ---------------------------------------------------------------------------

const (
	wSoldier uint16 = 0 // armed, mobile attacker
	wDummy   uint16 = 1 // unarmed, static defender
)

func waveDefs() []data.Unit {
	return []data.Unit{
		{ID: "soldier", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 0x4000,
			CollisionSize: 16, Pathing: data.PathingGround, AcquisitionRange: 400 * fixed.One,
			SightDay: 700 * fixed.One, SightNight: 700 * fixed.One,
			Attacks: []data.Attack{{AttackType: 0, Range: 120 * fixed.One, DamageBase: 50,
				CooldownTicks: 20, DamagePointTicks: 2, BackswingTicks: 2, Delivery: data.DeliveryInstant}}},
		{ID: "dummy", Life: 100, CollisionSize: 16, Pathing: data.PathingGround,
			SightDay: 200 * fixed.One, SightNight: 200 * fixed.One},
	}
}

type waveAdapter struct{ w *sim.World }

func (a *waveAdapter) EligibleUnits(player, typeID int, dst []int32) []int32 {
	var ids []sim.EntityID
	ids = a.w.AppendAllUnits(ids) // ascending entity id
	for _, id := range ids {
		or := a.w.Owners.Row(id)
		ur := a.w.UnitTypes.Row(id)
		if or == -1 || ur == -1 {
			continue
		}
		if int(a.w.Owners.Player[or]) == player && int(a.w.UnitTypes.TypeID[ur]) == typeID {
			dst = append(dst, int32(uint32(id)))
		}
	}
	return dst
}
func (a *waveAdapter) UnitPos(id int32) (int32, int32, bool) {
	e := sim.EntityID(uint32(id))
	if !a.w.Ents.Alive(e) {
		return 0, 0, false
	}
	tr := a.w.Transforms.Row(e)
	if tr == -1 {
		return 0, 0, false
	}
	p := a.w.Transforms.Pos[tr]
	return int32(p.X.Floor()), int32(p.Y.Floor()), true
}
func (a *waveAdapter) OrderMoveTo(id, x, y int32) {
	a.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: ptWU(x, y)}, false)
}
func (a *waveAdapter) OrderAttackTo(id, x, y int32) {
	// Realize attack-move as a move toward the target: on arrival the unit's
	// idle stance acquires nearby enemies and the attack cycle engages. The sim
	// does not yet implement attack-move-point acquisition or target pursuit
	// (the acquiring stances are Stop/Hold/Patrol; pursuit is the #150 work), so
	// move-to-target + on-arrival acquisition is the faithful realization today.
	// When pursuit lands this switches to sim.OrderAttack.
	a.w.IssueOrder(sim.EntityID(uint32(id)), sim.Order{Kind: sim.OrderMove, Point: ptWU(x, y)}, false)
}

func ptWU(x, y int32) fixed.Vec2 { return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)} }

func waveSimWorld(t *testing.T) *sim.World {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 64, PathRequests: 64})
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatal(err)
	}
	g := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.SetFlags(x, y, path.Walkable|path.Flyable)
		}
	}
	w.SetGrid(g) // movement + pathing; Visibility unbound → combat is not fog-gated
	if !w.BindUnitDefs(waveDefs()) {
		t.Fatal("BindUnitDefs failed")
	}
	return w
}

func outcomeHash(w *sim.World) (h uint64, p1Alive, p2Alive int) {
	var ids []sim.EntityID
	ids = w.AppendAllUnits(ids)
	hh := fnv.New64a()
	for _, id := range ids { // AppendAllUnits is ascending → stable hash input
		or := w.Owners.Row(id)
		hr := w.Healths.Row(id)
		life := int64(0)
		if hr != -1 {
			life = int64(w.Healths.Life[hr].Floor())
		}
		owner := -1
		if or != -1 {
			owner = int(w.Owners.Player[or])
		}
		fmt.Fprintf(hh, "id%d/p%d/hp%d;", uint32(id), owner, life)
		if owner == 1 {
			p1Alive++
		} else if owner == 2 {
			p2Alive++
		}
	}
	return hh.Sum64(), p1Alive, p2Alive
}

// TestAttackWavesSkirmishFSV — a 3-soldier wave (player 1) launched at a static
// 3-dummy defense (player 2). The sim's order + acquisition + combat systems
// resolve it; the final outcome (surviving units + hp) hashes identically across
// two runs, and the defenders are wiped while the attackers survive.
func TestAttackWavesSkirmishFSV(t *testing.T) {
	run := func(log bool) (uint64, int, int) {
		w := waveSimWorld(t)
		// Attackers clustered around (1000,1000); defenders clustered around
		// (1300,1000). The wave gathers at the attackers' cluster then launches
		// toward a standoff point in weapon range of the defense.
		atk := []sim.EntityID{}
		for i, p := range [][2]int32{{1000, 1000}, {1032, 1000}, {1000, 1032}} {
			id, ok := w.SpawnFromTable(wSoldier, 1, 1, ptWU(p[0], p[1]))
			if !ok {
				t.Fatalf("attacker %d spawn failed", i)
			}
			atk = append(atk, id)
		}
		for i, p := range [][2]int32{{1300, 1000}, {1300, 1040}, {1340, 1000}} {
			if _, ok := w.SpawnFromTable(wDummy, 2, 2, ptWU(p[0], p[1])); !ok {
				t.Fatalf("defender %d spawn failed", i)
			}
		}

		mgr := ai.NewWaveManager(&waveAdapter{w}, 64, 6)
		// Gather at the attackers' cluster (formed within a few ticks); target is
		// a standoff in weapon range of the defense centroid (~1313,1013).
		id := mgr.Stage(1, 1010, 1010, 1230, 1013, []ai.Quota{{TypeID: int(wSoldier), Count: 3}}, w.Tick())
		if id == 0 {
			t.Fatal("Stage made no wave")
		}

		var launchTick uint32
		for i := 0; i < 400; i++ {
			now := w.Tick()
			before := mgr.WaveState(id)
			mgr.Tick(now)
			if before == ai.WaveGathering && mgr.WaveState(id) == ai.WaveLaunched {
				launchTick = now
			}
			w.RecomputeVisibility() // the wave reveals the defense as it advances
			w.Step()
			_, _, p2 := outcomeHash(w)
			if p2 == 0 { // defense wiped
				break
			}
		}
		h, p1, p2 := outcomeHash(w)
		if log {
			t.Logf("FSV skirmish: launch tick=%d final p1Alive=%d p2Alive=%d outcomeHash=%#x at tick %d", launchTick, p1, p2, h, w.Tick())
		}
		return h, p1, p2
	}

	h1, p1a, p2a := run(true)
	h2, _, _ := run(false)
	if p2a != 0 {
		t.Fatalf("defense not wiped: p2Alive=%d (wave failed to resolve)", p2a)
	}
	if p1a != 3 {
		t.Fatalf("attackers should all survive vs unarmed dummies: p1Alive=%d want 3", p1a)
	}
	if h1 != h2 {
		t.Fatalf("skirmish outcome not deterministic: %#x != %#x", h1, h2)
	}
	t.Logf("FSV skirmish outcome deterministic across 2 runs: hash=%#x (attackers 3 survive, defenders 0)", h1)
}
