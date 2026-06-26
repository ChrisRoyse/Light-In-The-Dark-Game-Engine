package render

import (
	"testing"

	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
)

// #161 regression — FogTerrainMesh's fail-closed enable logic. The fog term must
// only apply when it has a real fog texture AND a non-degenerate world rectangle
// to map onto; a nil fog or a zero-extent rectangle must leave the surface
// undimmed (visible) rather than sample a 1/0 affine or a nil texture. SoT = the
// mesh's FogEnabled() flag, which gates the LitdFogControl uniform the shader
// reads. Constructed headless (CPU only — no GL context needed).
func TestFogTerrainMeshFailClosedFSV(t *testing.T) {
	geom := geometry.NewPlane(10, 10)
	mat := material.NewStandard(&math32.Color{R: 0.3, G: 0.5, B: 0.2})
	fog := NewFogTexture(1)
	fog.Update(synthFogAllVisible{size: int32(fog.Size())}, 1)
	origin := math32.Vector2{X: -4096, Y: -4096}
	size := math32.Vector2{X: 8192, Y: 8192}

	cases := []struct {
		name   string
		fog    *FogTexture
		size   math32.Vector2
		wantOn bool
	}{
		{"valid fog + extent -> enabled", fog, size, true},
		{"nil fog -> disabled (no nil sample)", nil, size, false},
		{"zero X extent -> disabled (no 1/0 affine)", fog, math32.Vector2{X: 0, Y: 8192}, false},
		{"zero Y extent -> disabled (no 1/0 affine)", fog, math32.Vector2{X: 8192, Y: 0}, false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			m := NewFogTerrainMesh(geom, mat, tc.fog, origin, tc.size)
			got := m.FogEnabled()
			t.Logf("FSV fog=%v size=%v -> FogEnabled=%v (want %v)", tc.fog != nil, tc.size, got, tc.wantOn)
			if got != tc.wantOn {
				t.Fatalf("FogEnabled=%v, want %v", got, tc.wantOn)
			}
		})
	}

	// SetFogEnabled toggles, but can never enable a mesh that has no fog texture
	// (fail-closed): turning a nil-fog mesh "on" must stay off.
	on := NewFogTerrainMesh(geom, mat, fog, origin, size)
	on.SetFogEnabled(false)
	if on.FogEnabled() {
		t.Fatal("SetFogEnabled(false) did not disable")
	}
	on.SetFogEnabled(true)
	if !on.FogEnabled() {
		t.Fatal("SetFogEnabled(true) did not re-enable a fog-backed mesh")
	}
	off := NewFogTerrainMesh(geom, mat, nil, origin, size)
	off.SetFogEnabled(true)
	if off.FogEnabled() {
		t.Fatal("SetFogEnabled(true) enabled a mesh with no fog texture (must fail closed)")
	}
	t.Log("FSV SetFogEnabled fail-closed: nil-fog mesh stays disabled even when toggled on")
}

// #536 regression — the translated-mesh fog path. Per-fragment fog on a unit is
// taken from each fragment's WORLD position, which the shader computes as
// ModelMatrix * localPosition. The ModelMatrix uniform is fed from the mesh's
// MatrixWorld(); if that does not reflect SetPosition (e.g. stays identity or a
// zero matrix), every fragment collapses to one world point and the whole mesh
// fogs to a single flat value instead of a gradient. SoT = the mesh's
// MatrixWorld translation column (what the shader's ModelMatrix uniform will be).
func TestFogTerrainMeshTranslatedModelMatrixFSV(t *testing.T) {
	geom := geometry.NewBox(100, 10, 100)
	mat := material.NewStandard(&math32.Color{R: 0.8, G: 0.8, B: 0.8})
	fog := NewFogTexture(1)
	fog.Update(synthFogAllVisible{size: int32(fog.Size())}, 1)
	origin := math32.Vector2{X: -4096, Y: -4096}
	size := math32.Vector2{X: 8192, Y: 8192}

	const wx, wy, wz = 2730.0, 260.0, -1000.0
	m := NewFogTerrainMesh(geom, mat, fog, origin, size)
	m.SetPosition(wx, wy, wz)
	m.UpdateMatrixWorld()
	mw := m.MatrixWorld() // column-major mat4; translation at [12],[13],[14]

	t.Logf("FSV translated mesh: MatrixWorld translation=(%.1f,%.1f,%.1f) scaleX=%.3f",
		mw[12], mw[13], mw[14], mw[0])
	if mw[12] != wx || mw[13] != wy || mw[14] != wz {
		t.Fatalf("MatrixWorld translation=(%.1f,%.1f,%.1f), want (%.1f,%.1f,%.1f) — "+
			"shader ModelMatrix would not place fragments at the unit's world position",
			mw[12], mw[13], mw[14], float32(wx), float32(wy), float32(wz))
	}
	// A zero/degenerate upper-3x3 would map every local position to the same world
	// point (the bug class: whole mesh fogs to one flat value). Identity scale = 1.
	if mw[0] == 0 || mw[5] == 0 || mw[10] == 0 {
		t.Fatalf("MatrixWorld upper-3x3 has a zero diagonal (%.3f,%.3f,%.3f) — "+
			"local positions collapse and per-fragment fog degenerates to one sample",
			mw[0], mw[5], mw[10])
	}
}

// synthFogAllVisible is a trivial all-visible fog source for the enable-logic
// test (the zone content is exercised by fog_test.go; here only enable matters).
type synthFogAllVisible struct{ size int32 }

func (s synthFogAllVisible) FogStateAt(_ uint8, x, y int32) uint8 {
	if x < 0 || y < 0 || x >= s.size || y >= s.size {
		return 0
	}
	return 2
}
