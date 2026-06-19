package assetcatalog

// #411 FSV: CheckGLB is the shared load-time glTF catalog. SoT = the finding
// strings it returns for a synthetic GLB whose contents we control (X+X=Y: a
// Draco-flagged doc must yield GLTF-COMPRESS, a clean doc must yield nothing).

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"strings"
	"testing"
)

// makeGLB wraps a glTF JSON document in a minimal GLB container (header + JSON
// chunk), 4-byte aligned per spec.
func makeGLB(t *testing.T, doc map[string]any) []byte {
	t.Helper()
	j, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	for len(j)%4 != 0 {
		j = append(j, ' ')
	}
	var b bytes.Buffer
	w := func(v uint32) { binary.Write(&b, binary.LittleEndian, v) }
	w(0x46546C67) // glTF
	w(2)
	w(uint32(12 + 8 + len(j)))
	w(uint32(len(j)))
	w(0x4E4F534A) // JSON
	b.Write(j)
	return b.Bytes()
}

func v20() map[string]any { return map[string]any{"version": "2.0"} }

func TestCheckGLBCatalogFSV(t *testing.T) {
	// Triangle bomb: a non-indexed primitive whose POSITION accessor has
	// 12,003 vertices → 4,001 triangles, one over the 4,000 ceiling.
	overBudget := map[string]any{
		"asset":     v20(),
		"meshes":    []any{map[string]any{"primitives": []any{map[string]any{"attributes": map[string]any{"POSITION": 0}}}}},
		"accessors": []any{map[string]any{"count": 12003}},
	}
	cases := []struct {
		name    string
		data    []byte
		wantHit string // "" == must be clean
	}{
		{"clean core-profile", makeGLB(t, map[string]any{"asset": v20()}), ""},
		{"allowlisted unlit ext", makeGLB(t, map[string]any{"asset": v20(), "extensionsUsed": []string{"KHR_materials_unlit"}}), ""},
		{"draco compression", makeGLB(t, map[string]any{"asset": v20(), "extensionsUsed": []string{"KHR_draco_mesh_compression"}}), "GLTF-COMPRESS"},
		{"meshopt compression", makeGLB(t, map[string]any{"asset": v20(), "extensionsRequired": []string{"EXT_meshopt_compression"}}), "GLTF-COMPRESS"},
		{"forbidden extension", makeGLB(t, map[string]any{"asset": v20(), "extensionsUsed": []string{"KHR_lights_punctual"}}), "GLTF-EXT"},
		{"external buffer URI", makeGLB(t, map[string]any{"asset": v20(), "buffers": []any{map[string]any{"uri": "geo.bin"}}}), "GLTF-URI"},
		{"over-ceiling geometry", makeGLB(t, overBudget), "GEO-MAX"},
		{"malformed (bad magic)", []byte("NOTAGLBNOTAGLBNOTAGLB"), "GLTF-CORE"},
		{"too short", []byte{1, 2, 3}, "GLTF-CORE"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			hits := CheckGLB(c.data)
			if c.wantHit == "" {
				if len(hits) != 0 {
					t.Fatalf("%s: want clean, got findings %v", c.name, hits)
				}
				t.Logf("FSV #411 CLEAN  %-24s -> no findings", c.name)
				return
			}
			if len(hits) == 0 {
				t.Fatalf("%s: want a %s finding, got none (catalog bypass)", c.name, c.wantHit)
			}
			joined := strings.Join(hits, " | ")
			if !strings.Contains(joined, c.wantHit) {
				t.Fatalf("%s: want %s, got %q", c.name, c.wantHit, joined)
			}
			t.Logf("FSV #411 FINDING %-24s -> %s", c.name, joined)
		})
	}
}

// TestCheckGLBBoundary — the ceiling is inclusive: exactly 4,000 triangles
// passes, 4,001 fails (off-by-one guard on the geometry bomb gate).
func TestCheckGLBBoundary(t *testing.T) {
	atLimit := makeGLB(t, map[string]any{
		"asset":     v20(),
		"meshes":    []any{map[string]any{"primitives": []any{map[string]any{"attributes": map[string]any{"POSITION": 0}}}}},
		"accessors": []any{map[string]any{"count": 12000}}, // 12000/3 = 4000 == ceiling
	})
	if hits := CheckGLB(atLimit); len(hits) != 0 {
		t.Fatalf("4000 triangles (== ceiling) must pass, got %v", hits)
	}
	t.Logf("FSV #411 boundary: 4000 tris (== %d ceiling) passes; 4001 fails (see catalog test)", MaxArchiveTriangles)
}
