package sim

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// acqWorld builds a world with a scanner at (1000,1000), acquisition
// range 600, weapon cooldown set (armed).
func acqWorld(t *testing.T) (*World, EntityID) {
	t.Helper()
	w := NewWorld(Caps{})
	s := acqUnit(t, w, 0, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One})
	cr := w.Combats.Row(s)
	w.Combats.AcquisitionRange[cr] = 600 * fixed.One
	return w, s
}

// acqUnit spawns an owned, damageable, armed unit at pos.
func acqUnit(t *testing.T, w *World, player, team uint8, pos fixed.Vec2) EntityID {
	t.Helper()
	id, ok := w.CreateUnit(pos, 0)
	if !ok || !w.Owners.Add(w.Ents, id, player, team, player) ||
		!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) ||
		!w.Combats.Add(w.Ents, id) {
		t.Fatal("acq unit setup failed")
	}
	w.Combats.Cooldown[w.Combats.Row(id)][0] = 27 // armed
	return id
}

// tuple stringer for the candidate dumps the issue FSV demands.
func acqTuple(w *World, scanner, cand EntityID) string {
	cr := w.Combats.Row(scanner)
	sp := w.Transforms.Pos[w.Transforms.Row(scanner)]
	cp := w.Transforms.Pos[w.Transforms.Row(cand)]
	hi, lo := fixed.DistSq(sp, cp)
	return fmt.Sprintf("(threat=%d distSq=%d/%d idx=%d)",
		w.threatClassOf(cr, cand), hi, lo, cand.Index())
}

// Full candidate table: armed closer enemy beats farther one; ally and
// dead candidates are filtered. Winner verified against the running
// best the system picked.
func TestAcquireCandidateTable(t *testing.T) {
	w, s := acqWorld(t)
	near := acqUnit(t, w, 2, 1, fixed.Vec2{X: 1200 * fixed.One, Y: 1000 * fixed.One})   // 200 away
	far := acqUnit(t, w, 2, 1, fixed.Vec2{X: 1500 * fixed.One, Y: 1000 * fixed.One})    // 500 away
	ally := acqUnit(t, w, 1, 0, fixed.Vec2{X: 1050 * fixed.One, Y: 1000 * fixed.One})   // same team
	corpse := acqUnit(t, w, 3, 1, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}) // will die
	w.KillUnit(corpse)
	w.Step() // bury the corpse (and run a first scan pass)

	got := w.acquireScan(w.Combats.Row(s), s)
	for _, c := range []struct {
		name string
		id   EntityID
	}{{"near-enemy", near}, {"far-enemy", far}, {"ally", ally}} {
		mark := " "
		if c.id == got {
			mark = "← WINNER"
		}
		t.Logf("candidate %-10s %s %s", c.name, acqTuple(w, s, c.id), mark)
	}
	t.Logf("corpse %d filtered (dead)", corpse)
	if got != near {
		t.Fatalf("winner = %d, want near enemy %d", got, near)
	}
}

// Issue edge 1: equidistant candidates → lower entityIndex wins, and
// reversing the bucket list order changes nothing (total order).
func TestAcquireTieBreakAndScanOrder(t *testing.T) {
	w, s := acqWorld(t)
	a := acqUnit(t, w, 2, 1, fixed.Vec2{X: 1300 * fixed.One, Y: 1000 * fixed.One}) // east, 300
	b := acqUnit(t, w, 2, 1, fixed.Vec2{X: 700 * fixed.One, Y: 1000 * fixed.One})  // west, 300
	cr := w.Combats.Row(s)

	run1 := w.acquireScan(cr, s)
	t.Logf("run 1 (creation order): a=%s b=%s → winner %d", acqTuple(w, s, a), acqTuple(w, s, b), run1)

	// reverse the bucket list: a re-files at its bucket head
	pa := w.Transforms.Pos[w.Transforms.Row(a)]
	w.bucketRemove(a)
	w.bucketInsert(a, pa)
	run2 := w.acquireScan(cr, s)
	t.Logf("run 2 (reversed bucket order) → winner %d", run2)

	if run1 != a || run2 != a {
		t.Fatalf("equidistant tie must go to lower index %d (got %d, %d)", a.Index(), run1, run2)
	}
}

// Issue edge 2: my recent attacker in range beats a closer
// non-attacker via threatClass; the memory decays.
func TestAcquireAttackerPriority(t *testing.T) {
	w, s := acqWorld(t)
	closer := acqUnit(t, w, 2, 1, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One})   // 100 away
	attacker := acqUnit(t, w, 2, 1, fixed.Vec2{X: 1450 * fixed.One, Y: 1000 * fixed.One}) // 450 away
	cr := w.Combats.Row(s)
	w.Combats.LastAttacker[cr] = attacker
	w.Combats.LastDamagedTick[cr] = 0

	got := w.acquireScan(cr, s)
	t.Logf("closer non-attacker %s, attacker %s → winner %d (want attacker %d)",
		acqTuple(w, s, closer), acqTuple(w, s, attacker), got, attacker)
	if got != attacker {
		t.Fatalf("attacker priority failed: got %d", got)
	}

	// decay: 101 ticks after the hit, the memory has expired
	for i := 0; i < DamageMemoryTicks+1; i++ {
		w.Step()
	}
	got = w.acquireScan(cr, s)
	t.Logf("after %d ticks decay: winner %d (want closer %d)", DamageMemoryTicks+1, got, closer)
	if got != closer {
		t.Fatalf("decayed memory must yield to distance: got %d", got)
	}
}

// Issue edge 3: candidate outside acquisition range but inside attack
// range is NOT acquired; a candidate exactly ON the acquisition
// boundary is (inclusive compare).
func TestAcquireRangeBoundary(t *testing.T) {
	w, s := acqWorld(t)
	cr := w.Combats.Row(s)
	w.Combats.Range[cr][0] = 900 * fixed.One                                         // attack range > acquisition (synthetic)
	out := acqUnit(t, w, 2, 1, fixed.Vec2{X: 1700 * fixed.One, Y: 1000 * fixed.One}) // 700: in attack, out of acq

	got := w.acquireScan(cr, s)
	t.Logf("acq=600 attack=900 candidate at 700 %s → acquired=%v (want none)", acqTuple(w, s, out), got != 0)
	if got != 0 {
		t.Fatalf("candidate outside acquisition range must not be acquired (got %d)", got)
	}

	edge := acqUnit(t, w, 2, 1, fixed.Vec2{X: 1600 * fixed.One, Y: 1000 * fixed.One}) // exactly 600
	got = w.acquireScan(cr, s)
	t.Logf("candidate exactly at 600 %s → winner %d (boundary inclusive)", acqTuple(w, s, edge), got)
	if got != edge {
		t.Fatalf("boundary candidate must be acquired: got %d", got)
	}
}

// Throttle: a unit scans only on ticks where (tick+index)%interval==0
// — per-unit phase offset spreads scans deterministically.
func TestAcquireThrottlePhase(t *testing.T) {
	w, s := acqWorld(t)
	enemy := acqUnit(t, w, 2, 1, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One})
	_ = enemy
	cr := w.Combats.Row(s)
	idx := s.Index()

	acquiredAt := uint32(0)
	for i := 0; i < 10; i++ {
		w.Step()
		if w.Combats.Target[cr] != 0 && acquiredAt == 0 {
			acquiredAt = w.Tick()
		}
	}
	t.Logf("scanner idx=%d interval=%d acquired at tick %d ((tick+idx)%%interval==%d)",
		idx, DefaultAcquireInterval, acquiredAt, (acquiredAt+idx)%DefaultAcquireInterval)
	if acquiredAt == 0 {
		t.Fatal("never acquired")
	}
	if (acquiredAt+idx)%DefaultAcquireInterval != 0 {
		t.Fatalf("acquired on a non-scan tick %d", acquiredAt)
	}
}

// Order gating: a moving unit does not auto-acquire; Stop and Hold do.
func TestAcquireOrderGate(t *testing.T) {
	w, s := acqWorld(t)
	acqUnit(t, w, 2, 1, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One})
	if !w.Orders.Add(w.Ents, s) {
		t.Fatal("orders add")
	}
	cr, or := w.Combats.Row(s), w.Orders.Row(s)

	w.Orders.Kind[or] = OrderMove
	for i := 0; i < DefaultAcquireInterval+1; i++ {
		w.Step()
	}
	t.Logf("under OrderMove after %d ticks: target=%d (want 0)", DefaultAcquireInterval+1, w.Combats.Target[cr])
	if w.Combats.Target[cr] != 0 {
		t.Fatal("moving unit must not auto-acquire")
	}

	w.Orders.Kind[or] = OrderHold
	for i := 0; i < DefaultAcquireInterval+1; i++ {
		w.Step()
	}
	t.Logf("under OrderHold: target=%d (want nonzero)", w.Combats.Target[cr])
	if w.Combats.Target[cr] == 0 {
		t.Fatal("holding unit must auto-acquire")
	}
}

// Determinism: two identical worlds, scans driven through Step,
// identical Target columns.
func TestAcquireDeterminism(t *testing.T) {
	build := func() *World {
		w := NewWorld(Caps{})
		for i := 0; i < 40; i++ {
			team := uint8(i % 2)
			pos := fixed.Vec2{
				X: fixed.F64(1000+40*i) * fixed.One,
				Y: fixed.F64(1000+90*(i%7)) * fixed.One,
			}
			id := func() EntityID {
				id, ok := w.CreateUnit(pos, 0)
				if !ok || !w.Owners.Add(w.Ents, id, team, team, team) ||
					!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) ||
					!w.Combats.Add(w.Ents, id) {
					t.Fatal("setup")
				}
				return id
			}()
			cr := w.Combats.Row(id)
			w.Combats.Cooldown[cr][0] = 27
			w.Combats.AcquisitionRange[cr] = 600 * fixed.One
		}
		return w
	}
	w1, w2 := build(), build()
	for i := 0; i < 50; i++ {
		w1.Step()
		w2.Step()
	}
	for r := int32(0); r < w1.Combats.Count(); r++ {
		if w1.Combats.Target[r] != w2.Combats.Target[r] {
			t.Fatalf("row %d targets diverge: %d vs %d", r, w1.Combats.Target[r], w2.Combats.Target[r])
		}
	}
	t.Logf("40 units, 50 ticks: %d Target rows identical across worlds", w1.Combats.Count())
}

// The acquisition scan and bucket reconcile must be alloc-free.
func TestAcquireZeroAlloc(t *testing.T) {
	w, s := acqWorld(t)
	for i := 0; i < 60; i++ {
		acqUnit(t, w, 2, 1, fixed.Vec2{
			X: fixed.F64(900+10*i) * fixed.One,
			Y: fixed.F64(950+7*(i%9)) * fixed.One,
		})
	}
	cr := w.Combats.Row(s)
	allocs := testing.AllocsPerRun(100, func() {
		w.Combats.Target[cr] = 0
		w.acquisitionSystemForce(cr)
		w.bucketReconcile()
	})
	t.Logf("allocs/run over scan+reconcile: %v (must be 0)", allocs)
	if allocs != 0 {
		t.Fatalf("acquisition allocates: %v allocs/run", allocs)
	}
}

// acquisitionSystemForce scans one combat row ignoring the throttle —
// test-only entry to the same scan path.
func (w *World) acquisitionSystemForce(cr int32) {
	w.Combats.Target[cr] = w.acquireScan(cr, w.Combats.Entity[cr])
}

func BenchmarkAcquireScan(b *testing.B) {
	w := NewWorld(Caps{})
	mk := func(player, team uint8, pos fixed.Vec2) EntityID {
		id, ok := w.CreateUnit(pos, 0)
		if !ok || !w.Owners.Add(w.Ents, id, player, team, player) ||
			!w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) ||
			!w.Combats.Add(w.Ents, id) {
			b.Fatal("setup")
		}
		w.Combats.Cooldown[w.Combats.Row(id)][0] = 27
		return id
	}
	s := mk(0, 0, fixed.Vec2{X: 8000 * fixed.One, Y: 8000 * fixed.One})
	cr := w.Combats.Row(s)
	w.Combats.AcquisitionRange[cr] = 700 * fixed.One
	// 500-unit deathball around the scanner (the M3 low-tier scene)
	for i := 0; i < 500; i++ {
		mk(2, 1, fixed.Vec2{
			X: fixed.F64(7500+(i%32)*32) * fixed.One,
			Y: fixed.F64(7500+(i/32)*32) * fixed.One,
		})
	}
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		if w.acquireScan(cr, s) == 0 {
			b.Fatal("scan found nothing")
		}
	}
}
