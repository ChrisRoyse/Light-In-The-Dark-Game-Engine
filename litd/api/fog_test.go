package litd

// #243 fog API FSV. SoT = the sim visibility grid read back through
// Game.FogStateAt / IsVisibleTo after a recompute — proving the API writes
// real applied state, not just returning its own arguments.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

func fogGame(t *testing.T) (*Game, *sim.World) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 64})
	g := path.NewGrid()
	for y := int32(0); y < path.GridSize; y++ {
		for x := int32(0); x < path.GridSize; x++ {
			g.SetFlags(x, y, path.Walkable|path.Buildable|path.Flyable)
		}
	}
	w.SetGrid(g)
	defs := []data.Unit{{ID: "scout", Life: 100, SightDay: fixed.FromInt(360), CollisionSize: 16, Pathing: data.PathingGround}}
	if !w.BindUnitDefs(defs) {
		t.Fatal("BindUnitDefs failed")
	}
	w.SetTimeOfDay(12 * fixed.One)
	w.SuspendTimeOfDay(true)
	return newGame(w), w
}

// TestFogModifierAPIFSV — NewFogModifier reveals a point through the public
// API; Start/Stop/Destroy + Valid() behave; SoT is g.FogStateAt.
func TestFogModifierAPIFSV(t *testing.T) {
	g, w := fogGame(t)
	p := g.Player(0)
	pt := Vec2{X: 1000, Y: 1000}
	area := Rect{MinX: 980, MinY: 980, MaxX: 1020, MaxY: 1020}

	w.RecomputeVisibility()
	t.Logf("FSV before: FogStateAt=%d IsVisibleTo=%v (want masked/false)", g.FogStateAt(p, pt), g.IsVisibleTo(p, pt))
	if g.FogStateAt(p, pt) != FogMasked || g.IsVisibleTo(p, pt) {
		t.Fatalf("expected masked before modifier, got %d", g.FogStateAt(p, pt))
	}

	// created stopped → no effect yet.
	f := g.NewFogModifier(p, FogVisible, area)
	if !f.Valid() {
		t.Fatal("modifier handle invalid after create")
	}
	w.RecomputeVisibility()
	t.Logf("FSV created-stopped: FogStateAt=%d (want masked)", g.FogStateAt(p, pt))
	if g.FogStateAt(p, pt) != FogMasked {
		t.Fatalf("stopped modifier affected grid: %d", g.FogStateAt(p, pt))
	}

	// start → reveals.
	f.Start()
	w.RecomputeVisibility()
	t.Logf("FSV started: FogStateAt=%d IsVisibleTo=%v (want visible/true)", g.FogStateAt(p, pt), g.IsVisibleTo(p, pt))
	if g.FogStateAt(p, pt) != FogVisible || !g.IsVisibleTo(p, pt) {
		t.Fatalf("started modifier did not reveal: %d", g.FogStateAt(p, pt))
	}

	// destroy → handle invalid, reverts (to fogged, was visible).
	f.Destroy()
	w.RecomputeVisibility()
	t.Logf("FSV destroyed: valid=%v FogStateAt=%d (want false/fogged)", f.Valid(), g.FogStateAt(p, pt))
	if f.Valid() || g.FogStateAt(p, pt) != FogFogged {
		t.Fatalf("destroy failed: valid=%v state=%d", f.Valid(), g.FogStateAt(p, pt))
	}

	// edge: zero/invalid handle methods are no-ops (no panic).
	var zero FogModifier
	zero.Start()
	zero.Stop()
	zero.Destroy()
	if zero.Valid() {
		t.Fatal("zero modifier reports valid")
	}
}

// TestFogCircleAndSharedAPIFSV — Circle area + SharedVision option through
// the API. p0 shares vision with p1; the circle reveals for both.
func TestFogCircleAndSharedAPIFSV(t *testing.T) {
	g, w := fogGame(t)
	p0, p1, p2 := g.Player(0), g.Player(1), g.Player(2)
	p0.SetAllianceFlag(p1, AllySharedVision, true)
	pt := Vec2{X: 1500, Y: 1500}
	circ := Circle{Center: pt, Radius: 200}

	g.NewFogModifier(p0, FogVisible, circ, SharedVision(true), Started())
	w.RecomputeVisibility()
	t.Logf("FSV shared circle: p0=%v p1=%v p2=%v (want true/true/false)",
		g.IsVisibleTo(p0, pt), g.IsVisibleTo(p1, pt), g.IsVisibleTo(p2, pt))
	if !g.IsVisibleTo(p0, pt) || !g.IsVisibleTo(p1, pt) {
		t.Fatal("shared circle did not reveal for ally")
	}
	if g.IsVisibleTo(p2, pt) {
		t.Fatal("shared circle leaked to non-ally")
	}

	// edge: a far point outside the circle stays masked.
	far := Vec2{X: 2200, Y: 2200}
	t.Logf("FSV outside circle: p0 far=%d (want masked)", g.FogStateAt(p0, far))
	if g.FogStateAt(p0, far) != FogMasked {
		t.Fatalf("circle leaked outside radius: %d", g.FogStateAt(p0, far))
	}
}

// TestFogTogglesAndShareVisionAPIFSV — global toggles + Unit.ShareVision via
// the API, read back at the sim SoT.
func TestFogTogglesAndShareVisionAPIFSV(t *testing.T) {
	g, w := fogGame(t)
	p0, p1 := g.Player(0), g.Player(1)

	// instant SetFogState stamps now (no recompute needed to observe).
	pt := Vec2{X: 800, Y: 800}
	g.SetFogState(p0, FogVisible, Rect{MinX: 790, MinY: 790, MaxX: 810, MaxY: 810}, false)
	t.Logf("FSV instant: FogStateAt=%d (want visible)", g.FogStateAt(p0, pt))
	if g.FogStateAt(p0, pt) != FogVisible {
		t.Fatalf("instant set did not stamp: %d", g.FogStateAt(p0, pt))
	}

	// toggles: defaults on, then flip.
	t.Logf("FSV toggle defaults: fog=%v mask=%v", g.FogEnabled(), g.FogMaskEnabled())
	if !g.FogEnabled() || !g.FogMaskEnabled() {
		t.Fatal("toggle defaults wrong")
	}
	dark := Vec2{X: 3000, Y: 3000}
	g.SetFogMaskEnabled(false)
	t.Logf("FSV mask-off: FogStateAt(dark)=%d (want fogged)", g.FogStateAt(p0, dark))
	if g.FogStateAt(p0, dark) != FogFogged {
		t.Fatalf("mask-off should report fogged: %d", g.FogStateAt(p0, dark))
	}
	g.SetFogEnabled(false)
	t.Logf("FSV fog-off: FogStateAt(dark)=%d (want visible)", g.FogStateAt(p0, dark))
	if g.FogStateAt(p0, dark) != FogVisible {
		t.Fatalf("fog-off should report visible: %d", g.FogStateAt(p0, dark))
	}
	g.SetFogEnabled(true)
	g.SetFogMaskEnabled(true)

	// Unit.ShareVision: a scout's sight reaches an ally only while shared.
	id, ok := w.CreateUnit(sim.CellCenter(50*path.GridSize+50), 0)
	if !ok || !w.Owners.Add(w.Ents, id, 0, 0, 0) || !w.UnitTypes.Add(w.Ents, id, 0) ||
		!w.Collisions.Add(w.Ents, id, 1, sim.PathGround) || !w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0) {
		t.Fatal("scout setup failed")
	}
	scout := Unit{id: id, g: g}
	scoutPos := Vec2{X: float64(w.Transforms.Pos[w.Transforms.Row(id)].X) / 4294967296.0, Y: float64(w.Transforms.Pos[w.Transforms.Row(id)].Y) / 4294967296.0}
	w.RecomputeVisibility()
	t.Logf("FSV pre-share: p1 sees scout=%v (want false)", g.IsVisibleTo(p1, scoutPos))
	if g.IsVisibleTo(p1, scoutPos) {
		t.Fatal("p1 already sees scout")
	}
	scout.ShareVision(p1, true)
	w.RecomputeVisibility()
	t.Logf("FSV shared: p1 sees scout=%v (want true)", g.IsVisibleTo(p1, scoutPos))
	if !g.IsVisibleTo(p1, scoutPos) {
		t.Fatal("ShareVision did not grant sight")
	}
}
