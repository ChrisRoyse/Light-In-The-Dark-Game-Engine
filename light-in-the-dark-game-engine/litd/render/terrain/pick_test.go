package terrain

import (
	"os"
	"testing"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/math32"
)

// Synthetic samplers — test INPUT with a known closed-form surface; the verdict
// is always the picked world point checked to lie on that surface (residual≈0).

type flatSampler struct {
	h   float32
	ext float32 // half-extent of the on-map region on each axis
}

func (s flatSampler) SampleHeight(x, z float32) (float32, bool) {
	if x < -s.ext || x > s.ext || z < -s.ext || z > s.ext {
		return 0, false
	}
	return s.h, true
}

// slopeSampler: surface height = k*z (a ramp), on-map within ext.
type slopeSampler struct {
	k, ext float32
}

func (s slopeSampler) SampleHeight(x, z float32) (float32, bool) {
	if x < -s.ext || x > s.ext || z < -s.ext || z > s.ext {
		return 0, false
	}
	return s.k * z, true
}

// stepSampler: a hard cliff — high terrace for z<edge, low for z>=edge.
type stepSampler struct {
	hi, lo, edge, ext float32
}

func (s stepSampler) SampleHeight(x, z float32) (float32, bool) {
	if x < -s.ext || x > s.ext || z < -s.ext || z > s.ext {
		return 0, false
	}
	if z < s.edge {
		return s.hi, true
	}
	return s.lo, true
}

const pickTol = 0.05 // world-unit tolerance after 24 bisection steps

func TestPickFlatPlaneFSV(t *testing.T) {
	var p Picker
	for _, h := range []float32{0, 500, -200} {
		s := flatSampler{h: h, ext: 5000}
		origin := math32.Vector3{X: 10, Y: 1000, Z: 20}
		dir := math32.Vector3{X: 0, Y: -1, Z: 0}
		r := p.Pick(origin, dir, s)
		t.Logf("FSV flat h=%.0f -> hit=%v world=(%.3f,%.3f,%.3f)", h, r.Hit, r.World.X, r.World.Y, r.World.Z)
		if !r.Hit {
			t.Fatalf("flat h=%.0f: no hit", h)
		}
		if math32.Abs(r.World.Y-h) > pickTol || r.World.X != 10 || r.World.Z != 20 {
			t.Fatalf("flat h=%.0f: world=(%.3f,%.3f,%.3f) want (10,%.0f,20)", h, r.World.X, r.World.Y, r.World.Z, h)
		}
	}
}

func TestPickSlopeOnSurfaceFSV(t *testing.T) {
	var p Picker
	s := slopeSampler{k: 0.5, ext: 100000}
	origin := math32.Vector3{X: 0, Y: 1000, Z: 0}
	dir := math32.Vector3{X: 0, Y: -1, Z: 1}
	r := p.Pick(origin, dir, s)
	residual := r.World.Y - s.k*r.World.Z
	t.Logf("FSV slope hit world=(%.3f,%.3f,%.3f) surfaceH(z)=%.3f residual=%.4f", r.World.X, r.World.Y, r.World.Z, s.k*r.World.Z, residual)
	if !r.Hit {
		t.Fatal("slope: no hit")
	}
	if math32.Abs(residual) > pickTol {
		t.Fatalf("slope pick off surface: residual=%.4f", residual)
	}
}

// TestPickCliffNearTerraceFSV — the first crossing is the near/upper terrace,
// so the marker sits on terrain, never mid-wall. (Edge case 1.)
func TestPickCliffNearTerraceFSV(t *testing.T) {
	var p Picker
	// High terrace (500) on the near side (z<0), low (0) beyond.
	s := stepSampler{hi: 500, lo: 0, edge: 0, ext: 100000}
	origin := math32.Vector3{X: 0, Y: 1000, Z: -800}
	dir := math32.Vector3{X: 0, Y: -1, Z: 1}
	r := p.Pick(origin, dir, s)
	t.Logf("FSV cliff hit world=(%.3f,%.3f,%.3f) hit=%v", r.World.X, r.World.Y, r.World.Z, r.Hit)
	if !r.Hit {
		t.Fatal("cliff: no hit")
	}
	// Must land exactly on a terrace (500 here), not floating between 0 and 500.
	if math32.Abs(r.World.Y-500) > pickTol {
		t.Fatalf("cliff pick mid-wall: Y=%.3f want 500 (upper terrace)", r.World.Y)
	}
	if r.World.Z >= 0 {
		t.Fatalf("cliff: expected near-side (z<0) hit, got z=%.3f", r.World.Z)
	}
}

// TestPickOffMapFSV — a ray over the void never crosses the surface. (Edge 2.)
func TestPickOffMapFSV(t *testing.T) {
	var p Picker
	s := flatSampler{h: 0, ext: 100} // tiny on-map region
	origin := math32.Vector3{X: 1000, Y: 1000, Z: 1000}
	dir := math32.Vector3{X: 0, Y: -1, Z: 0}
	r := p.Pick(origin, dir, s)
	t.Logf("FSV off-map hit=%v onMap=%v", r.Hit, r.OnMap)
	if r.Hit {
		t.Fatalf("ray over the void should not hit, got world=(%.1f,%.1f,%.1f)", r.World.X, r.World.Y, r.World.Z)
	}
}

// TestPickDeterminismFSV — pure function: 25 rays picked twice are identical
// (the headless-vs-rendered byte-identity guarantee at the function level).
func TestPickDeterminismFSV(t *testing.T) {
	var p Picker
	s := slopeSampler{k: 0.3, ext: 100000}
	mismatch := 0
	for i := 0; i < 25; i++ {
		fi := float32(i)
		origin := math32.Vector3{X: fi*7 - 80, Y: 2000, Z: fi*5 - 60}
		dir := math32.Vector3{X: 0.2, Y: -1, Z: 0.4}
		a := p.Pick(origin, dir, s)
		b := p.Pick(origin, dir, s)
		if a != b {
			mismatch++
			if mismatch <= 3 {
				t.Errorf("ray %d nondeterministic: %+v != %+v", i, a, b)
			}
		}
	}
	t.Logf("FSV determinism 25 rays, mismatch=%d (want 0)", mismatch)
	if mismatch != 0 {
		t.Fatalf("pick nondeterministic in %d/25 rays", mismatch)
	}
}

// TestPickRealMapFSV — a real loaded map: straight-down picks land on the
// bilinear surface and resolve the correct pathing cell. SoT = the map's own
// HeightAtVertex via the sampler. (Edge: real data + pathing-cell mapping.)
func TestPickRealMapFSV(t *testing.T) {
	m, err := litmapdata.Load(os.DirFS("../../.."), "data/maps/test64")
	if err != nil {
		t.Fatalf("load test64: %v", err)
	}
	s := NewMapHeightSampler(m)
	var p Picker

	// Sample several world XZ points; a straight-down ray must land at exactly
	// the sampler's surface height, and the pathing cell must match the
	// transform (world → terrain coord × PathingScale).
	type pt struct{ x, z float32 }
	for _, q := range []pt{{0, 0}, {-2000, 1500}, {1000, -1000}, {3000, 3000}} {
		want, on := s.SampleHeight(q.x, q.z)
		if !on {
			t.Fatalf("test point (%.0f,%.0f) off map", q.x, q.z)
		}
		origin := math32.Vector3{X: q.x, Y: 1e6, Z: q.z}
		r := p.Pick(origin, math32.Vector3{Y: -1}, s)
		halfW := float32(m.Width*CellSize) * 0.5
		halfH := float32(m.Height*CellSize) * 0.5
		wantCX := int32(math32.Floor((q.x + halfW) / CellSize * float32(litmapdata.PathingScale)))
		wantCY := int32(math32.Floor((q.z + halfH) / CellSize * float32(litmapdata.PathingScale)))
		t.Logf("FSV realmap (%.0f,%.0f) -> hit Y=%.3f wantH=%.3f cell=(%d,%d) wantCell=(%d,%d) onMap=%v",
			q.x, q.z, r.World.Y, want, r.PathCellX, r.PathCellY, wantCX, wantCY, r.OnMap)
		if !r.Hit || !r.OnMap {
			t.Fatalf("realmap (%.0f,%.0f): hit=%v onMap=%v", q.x, q.z, r.Hit, r.OnMap)
		}
		if math32.Abs(r.World.Y-want) > pickTol {
			t.Fatalf("realmap (%.0f,%.0f): Y=%.3f want %.3f", q.x, q.z, r.World.Y, want)
		}
		if r.PathCellX != wantCX || r.PathCellY != wantCY {
			t.Fatalf("realmap (%.0f,%.0f): cell=(%d,%d) want (%d,%d)", q.x, q.z, r.PathCellX, r.PathCellY, wantCX, wantCY)
		}
	}
}
