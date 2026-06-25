package terrain

import (
	"math"
	"os"
	"testing"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
)

// #78 splat bake FSV. SoT = the per-vertex RGB the bake produces from the real
// test64 splat map (mostly layer-A grass with a band of layer-C rock through the
// middle rows, cols 12-19), blended with DefaultBiomeLayers. Verifies the three
// edge cases that matter for a bake: a full-weight single-layer vertex matches that
// swatch exactly; a boundary vertex is a smooth convex blend of its neighbours (no
// hard seam); and no vertex is ever black/NaN/out-of-gamut (no magenta-debug or
// negative artefact).
func TestSplatBakeVertexColorsFSV(t *testing.T) {
	m, err := litmapdata.Load(os.DirFS("../../.."), "data/maps/test64")
	if err != nil {
		t.Fatalf("load test64: %v", err)
	}
	layers := DefaultBiomeLayers()
	cols := BakeVertexColors(m, layers)

	wv, hv := m.Width+1, m.Height+1
	if got, want := len(cols), wv*hv*3; got != want {
		t.Fatalf("bake length = %d, want %d (%dx%d vertices)", got, want, wv, hv)
	}
	at := func(x, y int) [3]float64 {
		i := (y*wv + x) * 3
		return [3]float64{float64(cols[i]), float64(cols[i+1]), float64(cols[i+2])}
	}
	approx := func(a, b float64) bool { return math.Abs(a-b) < 1e-4 }
	eqColor := func(c [3]float64, r, g, b float32) bool {
		return approx(c[0], float64(r)) && approx(c[1], float64(g)) && approx(c[2], float64(b))
	}

	// Edge 2: a vertex deep in the grass region (all 4 surrounding cells = layer A,
	// 255,0,0,0) bakes to the grass swatch exactly.
	grass := at(2, 2)
	if !eqColor(grass, layers[0].R, layers[0].G, layers[0].B) {
		t.Fatalf("grass vertex (2,2) = %v, want layer A %v", grass, layers[0])
	}
	t.Logf("FSV full-weight grass vertex = %v == layer A", grass)

	// Full-weight rock: a vertex inside the rock band (rows 30-33, cols 12-19; all 4
	// surrounding cells = layer C, 0,0,255,0) bakes to the rock swatch exactly.
	rock := at(15, 31)
	if !eqColor(rock, layers[2].R, layers[2].G, layers[2].B) {
		t.Fatalf("rock vertex (15,31) = %v, want layer C %v", rock, layers[2])
	}
	t.Logf("FSV full-weight rock vertex  = %v == layer C", rock)

	// Edge 1/smoothness: a vertex on the grass↔rock boundary (2 grass cells above,
	// 2 rock cells below) is the exact midpoint of the two swatches — a smooth blend,
	// not a hard seam and not a debug color.
	boundary := at(15, 30)
	midR := (layers[0].R + layers[2].R) / 2
	midG := (layers[0].G + layers[2].G) / 2
	midB := (layers[0].B + layers[2].B) / 2
	if !eqColor(boundary, midR, midG, midB) {
		t.Fatalf("boundary vertex (15,30) = %v, want grass/rock midpoint (%.3f,%.3f,%.3f)", boundary, midR, midG, midB)
	}
	// And it must be strictly between the two layers on the green channel (grass
	// 0.52 vs rock 0.46) — proving it actually blended, not snapped to one.
	if !(boundary[1] < float64(layers[0].G) && boundary[1] > float64(layers[2].G)) {
		t.Fatalf("boundary green %.4f not between rock %.2f and grass %.2f", boundary[1], layers[2].G, layers[0].G)
	}
	t.Logf("FSV boundary vertex          = %v (smooth grass↔rock midpoint)", boundary)

	// Edge 3: no in-bounds vertex is black/NaN/out-of-gamut.
	black := 0
	for y := 0; y < hv; y++ {
		for x := 0; x < wv; x++ {
			c := at(x, y)
			for ch, v := range c {
				if math.IsNaN(v) || v < 0 || v > 1 {
					t.Fatalf("vertex (%d,%d) channel %d = %v out of [0,1]", x, y, ch, v)
				}
			}
			if c[0] == 0 && c[1] == 0 && c[2] == 0 {
				black++
			}
		}
	}
	if black != 0 {
		t.Fatalf("%d in-bounds vertices baked to pure black (unweighted)", black)
	}
	t.Logf("FSV #78: %d vertices baked, all in [0,1], none black/NaN/magenta", wv*hv)
}

// A nil or degenerate map bakes to nil, not a panic (fail-closed).
func TestSplatBakeNilMapFSV(t *testing.T) {
	if BakeVertexColors(nil, DefaultBiomeLayers()) != nil {
		t.Fatal("nil map should bake to nil")
	}
}

// Applying the bake to a real terrain mesh geometry installs the VertexColor VBO and
// enables the LITD_VERTEX_COLOR shader path — the renderable half of #78. SoT = the
// returned SplatSnapshot + the geometry's VBO/shader-define state.
func TestSplatApplyToMeshFSV(t *testing.T) {
	m, err := litmapdata.Load(os.DirFS("../../.."), "data/maps/test64")
	if err != nil {
		t.Fatalf("load test64: %v", err)
	}
	mesh, err := Build(m)
	if err != nil {
		t.Fatalf("build terrain mesh: %v", err)
	}
	snap, err := ApplySplatVertexColors(mesh.Geometry, m, DefaultBiomeLayers())
	if err != nil {
		t.Fatalf("apply splat: %v", err)
	}
	if snap.VertexCount != (m.Width+1)*(m.Height+1) {
		t.Fatalf("snapshot VertexCount=%d, want %d", snap.VertexCount, (m.Width+1)*(m.Height+1))
	}
	if !snap.VertexColorVBO {
		t.Fatal("no VertexColor VBO after apply")
	}
	if !snap.ShaderDefine {
		t.Fatal("LITD_VERTEX_COLOR shader define not enabled")
	}
	if snap.ExistingColors {
		t.Fatal("fresh terrain mesh should not have had a prior color buffer")
	}
	t.Logf("FSV #78 apply: %+v", snap)

	// Idempotent re-apply overwrites in place (existing buffer path): same vertex
	// count, ExistingColors now true, still one VBO + define.
	snap2, err := ApplySplatVertexColors(mesh.Geometry, m, DefaultBiomeLayers())
	if err != nil {
		t.Fatalf("re-apply splat: %v", err)
	}
	if !snap2.ExistingColors || !snap2.VertexColorVBO || snap2.VertexCount != snap.VertexCount {
		t.Fatalf("re-apply snapshot wrong: %+v", snap2)
	}
	t.Logf("FSV #78 re-apply (overwrite existing buffer): %+v", snap2)
}
