package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// TestGameTerrainHeightFSV — Game.TerrainHeight reads the sim heightfield
// through the public API. SoT: the float height returned at known points
// of a 2×2 grid (corners 0/10/20/40 over [0,100]²), compared to the sim
// value directly. Unbound reads flat 0.
func TestGameTerrainHeightFSV(t *testing.T) {
	w := sim.NewWorld(sim.Caps{})
	g := newGame(w)

	// unbound: flat 0 through the API.
	t.Logf("FSV unbound: TerrainHeight(50,50)=%.3f (want 0)", g.TerrainHeight(Vec2{X: 50, Y: 50}))
	if g.TerrainHeight(Vec2{X: 50, Y: 50}) != 0 {
		t.Fatal("unbound API height not 0")
	}

	fi := func(n int32) fixed.F64 { return fixed.FromInt(n) }
	if !w.BindHeightfield(2, 2, 0, 0, fi(100), []fixed.F64{fi(0), fi(10), fi(20), fi(40)}) {
		t.Fatal("bind refused")
	}

	cases := []struct {
		p    Vec2
		want float64
	}{
		{Vec2{X: 0, Y: 0}, 0},
		{Vec2{X: 100, Y: 0}, 10},
		{Vec2{X: 0, Y: 100}, 20},
		{Vec2{X: 100, Y: 100}, 40},
		{Vec2{X: 50, Y: 50}, 17.5}, // bilinear centre
	}
	for _, c := range cases {
		got := g.TerrainHeight(c.p)
		t.Logf("FSV API: TerrainHeight(%.0f,%.0f)=%.3f want=%.3f", c.p.X, c.p.Y, got, c.want)
		if got != c.want {
			t.Fatalf("TerrainHeight(%.0f,%.0f) = %.3f, want %.3f", c.p.X, c.p.Y, got, c.want)
		}
	}

	// nil-game safety.
	var ng *Game
	if ng.TerrainHeight(Vec2{X: 1, Y: 1}) != 0 {
		t.Fatal("nil game TerrainHeight should be 0")
	}
}
