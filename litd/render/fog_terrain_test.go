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

// synthFogAllVisible is a trivial all-visible fog source for the enable-logic
// test (the zone content is exercised by fog_test.go; here only enable matters).
type synthFogAllVisible struct{ size int32 }

func (s synthFogAllVisible) FogStateAt(_ uint8, x, y int32) uint8 {
	if x < 0 || y < 0 || x >= s.size || y >= s.size {
		return 0
	}
	return 2
}
