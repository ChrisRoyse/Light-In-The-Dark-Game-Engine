package luabind

// First Flame high-ground terrain FSV (#174). Source of truth = the cliff/ramp
// grid parsed from data/maps/firstflame/cliff.txt by mapdata.Load, and the sim
// pathing grid GridFromMap bakes from it. Asserts the authored terrain realises
// "high-ground beacons, cliffs + ramps used meaningfully" (#174 spec):
//   - the three beacons sit on level-1 plateaus, both starts on level-0 ground;
//   - the cliff grid is exactly point-symmetric about the map centre (fairness);
//   - on the REAL sim grid (path.StepLegal cliff rule), the two starts reach each
//     other with equal path length both directions;
//   - each beacon plateau is reachable ONLY across a ramp — neutralise the ramps
//     and the high ground becomes unreachable from the low-ground starts.
// Manual inspection: the t.Logf lines below print the concrete evidence
// (symmetry sample, path lengths, reachable/unreachable) cross-checked by hand
// against the authored cliff.txt run-lengths.

import (
	"os"
	"testing"

	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// bfsDist returns the 4-neighbour shortest-step distance from (sx,sy) to (gx,gy)
// over g using the full ground-movement rule (StepLegal: destination walkable +
// cliff adjacency legal), or -1 if unreachable. Bounded to the map area w×h.
func bfsDist(g *path.Grid, w, h, sx, sy, gx, gy int) int {
	if sx < 0 || sy < 0 || gx < 0 || gy < 0 {
		return -1
	}
	dist := make([]int, w*h)
	for i := range dist {
		dist[i] = -1
	}
	dist[sy*w+sx] = 0
	queue := []int{sy*w + sx}
	for len(queue) > 0 {
		cur := queue[0]
		queue = queue[1:]
		cx, cy := cur%w, cur/w
		if cx == gx && cy == gy {
			return dist[cur]
		}
		for _, d := range [][2]int{{1, 0}, {-1, 0}, {0, 1}, {0, -1}} {
			nx, ny := cx+d[0], cy+d[1]
			if nx < 0 || ny < 0 || nx >= w || ny >= h {
				continue
			}
			if dist[ny*w+nx] != -1 {
				continue
			}
			if !g.StepLegal(int32(cx), int32(cy), int32(nx), int32(ny)) {
				continue
			}
			dist[ny*w+nx] = dist[cur] + 1
			queue = append(queue, ny*w+nx)
		}
	}
	return -1
}

func TestFirstFlameHighGroundTerrainFSV(t *testing.T) {
	root := os.DirFS("../..")
	m, err := mapdata.Load(root, "data/maps/firstflame")
	if err != nil {
		t.Fatalf("Load(firstflame): %v", err)
	}
	pw, ph := m.PathingWidth, m.PathingHeight
	if pw != 256 || ph != 256 {
		t.Fatalf("pathing dims = %dx%d, want 256x256", pw, ph)
	}

	cliff := func(x, y int) mapdata.Cliff {
		c, ok := m.CliffAt(x, y)
		if !ok {
			t.Fatalf("CliffAt(%d,%d) out of bounds", x, y)
		}
		return c
	}

	// (a) Spot-checks against the authored cliff.txt run-lengths (SoT bytes).
	type spot struct {
		x, y  int
		level uint8
		ramp  bool
		what  string
	}
	for _, s := range []spot{
		{128, 128, 1, false, "central beacon: plateau top"},
		{118, 128, 0, true, "central west ramp (y in [124,132])"},
		{138, 128, 0, true, "central east ramp (mirror)"},
		{118, 120, 1, false, "central plateau west edge, non-ramp row → cliff face"},
		{117, 128, 0, false, "low ground just west of central plateau"},
		{88, 88, 1, false, "NW beacon: plateau top"},
		{88, 98, 0, true, "NW south-edge ramp (x in [84,92])"},
		{168, 168, 1, false, "SE beacon: plateau top (mirror of NW)"},
		{168, 158, 0, true, "SE north-edge ramp (mirror of NW ramp)"},
		{40, 128, 0, false, "P0 start: low ground"},
		{216, 128, 0, false, "P1 start: low ground"},
	} {
		c := cliff(s.x, s.y)
		if c.Level != s.level || c.Ramp != s.ramp {
			t.Errorf("cliff(%d,%d) = {lvl %d ramp %v}, want {lvl %d ramp %v} — %s",
				s.x, s.y, c.Level, c.Ramp, s.level, s.ramp, s.what)
		}
	}

	// (b) Exact point symmetry about the centre: cell (x,y) mirrors (256-x,256-y).
	// Interior cells [1,255] map within range; the impassable outer ring is exempt.
	mirrors := 0
	for y := 1; y < ph; y++ {
		for x := 1; x < pw; x++ {
			a := cliff(x, y)
			b := cliff(pw-x, ph-y)
			if a.Level != b.Level || a.Ramp != b.Ramp {
				t.Fatalf("asymmetry: cliff(%d,%d)={%d,%v} != cliff(%d,%d)={%d,%v}",
					x, y, a.Level, a.Ramp, pw-x, ph-y, b.Level, b.Ramp)
			}
			mirrors++
		}
	}

	// (c) Beacons on level 1 (high ground), starts on level 0 (low ground).
	for _, b := range m.Beacons() {
		if c := cliff(b.X, b.Y); c.Level != 1 {
			t.Errorf("beacon %d at (%d,%d) on level %d, want high ground (1)", b.ID, b.X, b.Y, c.Level)
		}
	}
	for _, s := range m.Starts() {
		if c := cliff(s.X, s.Y); c.Level != 0 {
			t.Errorf("start P%d at (%d,%d) on level %d, want low ground (0)", s.Player, s.X, s.Y, c.Level)
		}
	}

	// (d) Build the REAL sim pathing grid and walk it.
	g, err := GridFromMap(m)
	if err != nil {
		t.Fatalf("GridFromMap: %v", err)
	}
	var p0, p1 mapdata.StartLocation
	for _, s := range m.Starts() {
		if s.Player == 0 {
			p0 = s
		} else if s.Player == 1 {
			p1 = s
		}
	}
	fwd := bfsDist(g, pw, ph, p0.X, p0.Y, p1.X, p1.Y)
	rev := bfsDist(g, pw, ph, p1.X, p1.Y, p0.X, p0.Y)
	if fwd < 0 {
		t.Fatalf("P0 start (%d,%d) cannot reach P1 start (%d,%d) on the sim grid", p0.X, p0.Y, p1.X, p1.Y)
	}
	if fwd != rev {
		t.Fatalf("path asymmetry: P0→P1 = %d steps, P1→P0 = %d steps", fwd, rev)
	}

	// Central beacon reachable from low-ground P0 start (across a ramp).
	bx, by := 128, 128
	upDist := bfsDist(g, pw, ph, p0.X, p0.Y, bx, by)
	if upDist < 0 {
		t.Fatalf("central beacon (%d,%d) unreachable from P0 start — high ground must be ramp-accessible", bx, by)
	}

	// (e) Ramp-gating teeth: neutralise every ramp (make it plain terrain at its
	// base level) and the high-ground beacon must become unreachable — proving the
	// ramp was the ONLY way up, i.e. the cliffs genuinely gate movement.
	g2, err := GridFromMap(m)
	if err != nil {
		t.Fatalf("GridFromMap #2: %v", err)
	}
	ramps := 0
	for y := 0; y < ph; y++ {
		for x := 0; x < pw; x++ {
			if c := cliff(x, y); c.Ramp {
				g2.SetCliffLevel(int32(x), int32(y), c.Level) // drop the ramp join
				ramps++
			}
		}
	}
	sealed := bfsDist(g2, pw, ph, p0.X, p0.Y, bx, by)
	if sealed >= 0 {
		t.Fatalf("central beacon still reachable (%d steps) after sealing all %d ramps — cliffs do not gate movement", sealed, ramps)
	}

	t.Logf("FSV #174 high ground: %d symmetric interior cells (exact mirror about centre); "+
		"3 beacons on level-1 plateaus, 2 starts on level-0 ground; "+
		"P0↔P1 path = %d steps both directions; central beacon %d steps up via ramp; "+
		"sealing all %d ramp cells makes it UNREACHABLE (cliff gating proven).",
		mirrors, fwd, upDist, ramps)
}
