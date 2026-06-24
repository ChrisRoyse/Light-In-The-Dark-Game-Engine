package ability_test

// #622 — attach_mover now carries the full MoverSpec param set. SoT = the live
// mover columns after a cast: a spec's angvel/turn_rate/height/decay/done and
// spline waypoints must appear on the instantiated mover, not be dropped.

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ability"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// castTemplate loads + registers a template and casts it, returning the world.
func castTemplate(t *testing.T, file string) *sim.World {
	t.Helper()
	registerDamageExec()
	blob, err := os.ReadFile(filepath.Join(specsDir, file))
	if err != nil {
		t.Fatal(err)
	}
	tpl, err := ability.LoadTOML(blob)
	if err != nil {
		t.Fatal(err)
	}
	w := sim.NewWorld(sim.Caps{Units: 16, Movers: 8, MoverWaypoints: 64})
	if err := w.BindEffects([]data.CompiledEffect{{Prim: data.EPDamage}}); err != nil {
		t.Fatal(err)
	}
	w.BindDamageMatrix([][]int32{{1000}})
	for _, n := range tpl.EffectLists {
		w.RegisterEffectListName(n, sim.EffectListSpan(0, 1))
	}
	if _, err := registerLowered(w, tpl.Source); err != nil {
		t.Fatal(err)
	}
	idx, _ := w.AbilityDefs.Lookup(tpl.Source.ID)
	caster := mkCaster(w)
	enemy := mkEnemy(w, 100)
	if !w.CastAbility(idx, caster, enemy, fixed.Vec2{X: 100 * fixed.One}) {
		t.Fatal("cast failed")
	}
	return w
}

// TestOrbitalGuardianParamsReachMover: angvel + done=loop plumb to the mover.
func TestOrbitalGuardianParamsReachMover(t *testing.T) {
	w := castTemplate(t, "orbital-guardian.toml")
	if w.Movers.Count() == 0 {
		t.Fatal("no mover instantiated")
	}
	r := int32(1)
	wantBAM := fixed.Angle(uint16(1638)) // round(9 * 65536 / 360) = 1638
	t.Logf("orbital-guardian mover: AngVel=%d (want %d) DoneMode=%d (want loop=1)",
		w.Movers.AngVel[r], wantBAM, w.Movers.DoneMode[r])
	if w.Movers.AngVel[r] != wantBAM {
		t.Fatalf("AngVel=%d, want %d (9 deg/tick)", w.Movers.AngVel[r], wantBAM)
	}
	if sim.MoverDoneMode(w.Movers.DoneMode[r]) != sim.MoverDoneLoop {
		t.Fatalf("DoneMode=%d, want loop", w.Movers.DoneMode[r])
	}
}

// TestBoomerangWaypointsReachMover: the spline control points reach the mover's
// waypoint span.
func TestBoomerangWaypointsReachMover(t *testing.T) {
	w := castTemplate(t, "boomerang.toml")
	if w.Movers.Count() == 0 {
		t.Fatal("no mover instantiated")
	}
	r := int32(1)
	t.Logf("boomerang mover: kind=%d WpLen=%d", w.Movers.Kind[r], w.Movers.WpLen[r])
	if sim.MoverKind(w.Movers.Kind[r]) != sim.MoverSpline {
		t.Fatalf("kind=%d, want spline", w.Movers.Kind[r])
	}
	if w.Movers.WpLen[r] != 5 {
		t.Fatalf("WpLen=%d, want 5 (out-and-back control points)", w.Movers.WpLen[r])
	}
}
