package litd

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// liveUnit creates a unit with a health row (spawns at full life) and
// an owner row, returning the public handle and the raw entity id (for
// reading the sim store directly as the Source of Truth).
func liveUnit(t *testing.T, w *sim.World, g *Game, player uint8, maxLife float64) (Unit, sim.EntityID) {
	t.Helper()
	var face fixed.Angle
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, face)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	if !w.Healths.Add(w.Ents, id, fromFloat(maxLife), 0, 0, 0) {
		t.Fatal("Healths.Add failed")
	}
	if !w.Owners.Add(w.Ents, id, player, 0, 0) {
		t.Fatal("Owners.Add failed")
	}
	return Unit{id: id, g: g}, id
}

// rawLife reads the unit's life straight out of the sim Health store —
// the Source of Truth behind the Life() getter.
func rawLife(w *sim.World, id sim.EntityID) (float64, bool) {
	r := w.Healths.Row(id)
	if r < 0 {
		return 0, false
	}
	return toFloat(w.Healths.Life[r]), true
}

// TestZeroValueSemantics is the R-API-5 contract: gameplay verbs on an
// invalid handle are silent no-ops, getters return zero values, debug
// mode reports the call site, and handle chains degrade to "" rather
// than crash. Each case prints SoT before/after.
func TestZeroValueSemantics(t *testing.T) {
	// ---- case 1: verb on a killed+removed unit ----
	t.Run("removed-unit-noop", func(t *testing.T) {
		w := sim.NewWorld(sim.Caps{Units: 16})
		g := newGame(w)
		u, id := liveUnit(t, w, g, 0, 100)
		t.Logf("BEFORE remove: Life()=%v rawStore=%v", u.Life(), mustRaw(t, w, id))
		if !w.DestroyUnit(id) {
			t.Fatal("DestroyUnit failed")
		}
		beforeSet := u.Life()
		u.SetLife(50) // must not panic, must not write
		afterSet := u.Life()
		_, present := rawLife(w, id)
		t.Logf("AFTER remove+SetLife(50): Valid=%v Life()=%v→%v storeRowPresent=%v",
			u.Valid(), beforeSet, afterSet, present)
		if u.Valid() || beforeSet != 0 || afterSet != 0 {
			t.Fatalf("removed-unit verb leaked: Valid=%v Life %v/%v want false,0,0", u.Valid(), beforeSet, afterSet)
		}
	})

	// ---- case 2: stale handle to a recycled slot ----
	t.Run("recycled-slot", func(t *testing.T) {
		w := sim.NewWorld(sim.Caps{Units: 16})
		g := newGame(w)
		a, aID := liveUnit(t, w, g, 0, 100) // slot s, gen G
		if !w.DestroyUnit(aID) {
			t.Fatal("DestroyUnit(A) failed")
		}
		b, bID := liveUnit(t, w, g, 1, 80) // reuses slot s, gen G+1
		if aID.Index() != bID.Index() {
			t.Fatalf("test setup: B did not reuse A's slot (A idx=%d B idx=%d)", aID.Index(), bID.Index())
		}
		bBefore, _ := rawLife(w, bID)
		t.Logf("BEFORE stale write: A.Valid=%v B.Valid=%v B store life=%v", a.Valid(), b.Valid(), bBefore)
		a.SetLife(50) // stale handle: must not touch B in the recycled slot
		bAfter, _ := rawLife(w, bID)
		t.Logf("AFTER  A.SetLife(50): A.Valid=%v B store life=%v B.Life()=%v", a.Valid(), bAfter, b.Life())
		if a.Valid() {
			t.Fatal("stale handle A reported Valid()=true after slot recycle")
		}
		if bBefore != 80 || bAfter != 80 || b.Life() != 80 {
			t.Fatalf("stale write corrupted recycled slot: B life %v→%v (getter %v), want 80", bBefore, bAfter, b.Life())
		}
	})

	// ---- case 3: debug mode reports the call site ----
	t.Run("debug-assert", func(t *testing.T) {
		w := sim.NewWorld(sim.Caps{Units: 16})
		g := newGame(w)
		var reports []string
		g.OnInvalidHandle(func(r string) { reports = append(reports, r) })

		// A stale-but-bound handle (was a real unit, now destroyed) is
		// the bug class debug mode catches: it still carries the game
		// pointer, so the assert can fire. A never-bound zero-value
		// handle has no game and is silent by construction.
		stale, sID := liveUnit(t, w, g, 0, 100)
		if !w.DestroyUnit(sID) {
			t.Fatal("DestroyUnit failed")
		}
		g.SetDebug(true)
		stale.SetLife(50)

		t.Logf("debug reports: %v", reports)
		if len(reports) != 1 {
			t.Fatalf("want exactly 1 invalid-handle report, got %d: %v", len(reports), reports)
		}
		if !strings.Contains(reports[0], "Unit.SetLife") || !strings.Contains(reports[0], "handle_validity_test.go:") {
			t.Fatalf("report missing verb or call site: %q", reports[0])
		}

		// off again: no report
		g.SetDebug(false)
		stale.SetLife(50)
		if len(reports) != 1 {
			t.Fatalf("report fired with debug off: %v", reports)
		}
	})

	// ---- case 4: zero-value chain degrades to "" ----
	t.Run("zero-chain", func(t *testing.T) {
		name := Unit{}.Owner().Name()
		t.Logf("Unit{}.Owner().Name() = %q (Owner zero=%v)", name, Unit{}.Owner().IsZero())
		if name != "" {
			t.Fatalf("zero-value chain returned %q, want \"\"", name)
		}
		if !(Unit{}).Owner().IsZero() {
			t.Fatal("Unit{}.Owner() should be the zero-value Player")
		}
	})
}

// TestValidityZeroAlloc — the validity guard and the production
// (debug-off) no-op path allocate nothing (R-GC-3).
func TestValidityZeroAlloc(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 16})
	g := newGame(w)
	u, _ := liveUnit(t, w, g, 0, 100)
	var stale Unit
	var sink float64
	var bsink bool

	if n := testing.AllocsPerRun(1000, func() { bsink = u.Valid() }); n != 0 {
		t.Errorf("Unit.Valid() allocates %.1f/run, want 0", n)
	}
	if n := testing.AllocsPerRun(1000, func() { sink = u.Life() }); n != 0 {
		t.Errorf("Unit.Life() allocates %.1f/run, want 0", n)
	}
	// invalid handle, debug OFF: the no-op verb path must not allocate
	if n := testing.AllocsPerRun(1000, func() { stale.SetLife(7) }); n != 0 {
		t.Errorf("invalid-handle SetLife() allocates %.1f/run, want 0", n)
	}
	t.Logf("zero-alloc verified: Valid/Life/no-op SetLife; sink=%v %v", bsink, sink)
}

func mustRaw(t *testing.T, w *sim.World, id sim.EntityID) float64 {
	t.Helper()
	v, ok := rawLife(w, id)
	if !ok {
		t.Fatalf("no health row for id %#x", uint32(id))
	}
	return v
}
