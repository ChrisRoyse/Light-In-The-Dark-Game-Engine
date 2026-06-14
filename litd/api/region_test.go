package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// TestRegionContainmentFSV — the Region noun: create, add a rect, test
// point and unit containment. SoT: Contains/ContainsUnit against known
// positions and the unit's actual sim transform.
func TestRegionContainmentFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 8})
	g := newGame(w)

	rg := g.NewRegion()
	rg.AddRect(NewRect(Vec2{0, 0}, Vec2{300, 300}))
	t.Logf("FSV region valid=%v contains(150,150)=%v contains(500,500)=%v",
		rg.Valid(), rg.Contains(Vec2{150, 150}), rg.Contains(Vec2{500, 500}))
	if !rg.Valid() {
		t.Fatalf("fresh region invalid")
	}
	if !rg.Contains(Vec2{150, 150}) {
		t.Fatalf("point inside rect not contained")
	}
	if rg.Contains(Vec2{500, 500}) {
		t.Fatalf("point outside rect contained")
	}

	uIn, _ := unitAt(t, w, g, 0, 150, 150)
	uOut, _ := unitAt(t, w, g, 0, 800, 800)
	t.Logf("FSV ContainsUnit in=%v out=%v", rg.ContainsUnit(uIn), rg.ContainsUnit(uOut))
	if !rg.ContainsUnit(uIn) {
		t.Fatalf("unit inside region not detected")
	}
	if rg.ContainsUnit(uOut) {
		t.Fatalf("unit outside region detected")
	}

	// RemoveRect clears it; AddCell adds back a single point's cell.
	rg.RemoveRect(NewRect(Vec2{0, 0}, Vec2{300, 300}))
	if rg.Contains(Vec2{150, 150}) {
		t.Fatalf("RemoveRect left point contained")
	}
	rg.AddCell(Vec2{150, 150})
	if !rg.Contains(Vec2{150, 150}) {
		t.Fatalf("AddCell(150,150) not contained")
	}
	if rg.Contains(Vec2{250, 250}) {
		t.Fatalf("AddCell leaked into a neighbour cell")
	}
}

// TestRegionRemoveAndStaleFSV — Remove invalidates the handle; a stale
// handle's verbs are safe no-ops and never touch the recycled slot.
func TestRegionRemoveAndStaleFSV(t *testing.T) {
	_, g := func() (*sim.World, *Game) { w := sim.NewWorld(sim.Caps{}); return w, newGame(w) }()

	r1 := g.NewRegion()
	r1.AddRect(NewRect(Vec2{0, 0}, Vec2{100, 100}))
	r1.Remove()
	t.Logf("FSV after Remove: r1.valid=%v", r1.Valid())
	if r1.Valid() {
		t.Fatalf("region valid after Remove")
	}
	r1.Remove() // idempotent, no panic

	r2 := g.NewRegion() // recycles r1's slot under a new generation
	t.Logf("FSV recycle: r1{id=%d gen=%d} r2{id=%d gen=%d} r1.valid=%v r2.valid=%v",
		r1.id, r1.gen, r2.id, r2.gen, r1.Valid(), r2.Valid())
	if r1.Valid() {
		t.Fatalf("stale handle valid after slot recycle")
	}
	if !r2.Valid() {
		t.Fatalf("recycled region invalid")
	}
	// Stale verbs must not write into r2.
	r1.AddRect(NewRect(Vec2{0, 0}, Vec2{1000, 1000}))
	if r2.Contains(Vec2{500, 500}) {
		t.Fatalf("stale AddRect wrote into the live recycled region")
	}
}

// TestRegionWorldBoundsFSV — WorldBounds is the 16,384-wu grid rect.
func TestRegionWorldBoundsFSV(t *testing.T) {
	_, g := func() (*sim.World, *Game) { w := sim.NewWorld(sim.Caps{}); return w, newGame(w) }()
	wb := g.WorldBounds()
	t.Logf("FSV WorldBounds = %+v (w=%.0f h=%.0f)", wb, wb.Width(), wb.Height())
	if wb.Min() != (Vec2{0, 0}) || wb.Width() != 16384 || wb.Height() != 16384 {
		t.Fatalf("WorldBounds = %+v, want [0,0]-[16384,16384]", wb)
	}
}

// TestRegionZeroValueNoOpFSV — the zero-value Region is inert (R-API-5).
func TestRegionZeroValueNoOpFSV(t *testing.T) {
	var z Region
	z.AddRect(NewRect(Vec2{0, 0}, Vec2{1, 1}))
	z.RemoveRect(NewRect(Vec2{0, 0}, Vec2{1, 1}))
	z.AddCell(Vec2{0, 0})
	z.Remove()
	t.Logf("FSV zero-value region valid=%v contains=%v", z.Valid(), z.Contains(Vec2{0, 0}))
	if z.Valid() || z.Contains(Vec2{0, 0}) {
		t.Fatalf("zero-value region not inert")
	}
}
