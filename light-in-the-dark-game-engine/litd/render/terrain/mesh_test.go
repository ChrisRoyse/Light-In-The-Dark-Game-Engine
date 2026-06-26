package terrain

import (
	"os"
	"testing"
	"testing/fstest"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
)

func TestMeshTest64HeightFSV(t *testing.T) {
	m, err := litmapdata.Load(os.DirFS("../../.."), "data/maps/test64")
	if err != nil {
		t.Fatal(err)
	}
	mesh, err := Build(m)
	if err != nil {
		t.Fatal(err)
	}
	samples, maxDiff, err := CompareHeights(mesh, m, HundredVertexSamples(m.Width, m.Height))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV terrain mesh test64 vertices=%d triangles=%d inverted=%d maxDiff=%d first=%+v last=%+v",
		mesh.VertexCount, mesh.TriangleCount, mesh.InvertedTriangles(), maxDiff, samples[0], samples[len(samples)-1])
	if mesh.VertexCount != 65*65 || mesh.TriangleCount != 64*64*2 {
		t.Fatalf("counts wrong: vertices=%d triangles=%d", mesh.VertexCount, mesh.TriangleCount)
	}
	if maxDiff != 0 {
		t.Fatalf("mesh height differs from sim height: maxDiff=%d", maxDiff)
	}
	if inv := mesh.InvertedTriangles(); inv != 0 {
		t.Fatalf("terrain mesh has inverted triangles: %d", inv)
	}
}

func TestMeshBorderVerticesFSV(t *testing.T) {
	m, err := litmapdata.Load(os.DirFS("../../.."), "data/maps/test64")
	if err != nil {
		t.Fatal(err)
	}
	mesh, err := Build(m)
	if err != nil {
		t.Fatal(err)
	}
	corners := [][2]int{{0, 0}, {64, 0}, {0, 64}, {64, 64}}
	for _, c := range corners {
		pos, ok := mesh.PositionAtVertex(c[0], c[1])
		h, _ := mesh.HeightAtVertex(c[0], c[1])
		t.Logf("FSV terrain border vertex (%d,%d) pos=(%.0f,%.0f,%.0f) height=%d", c[0], c[1], pos.X, pos.Y, pos.Z, h)
		if !ok {
			t.Fatalf("missing border vertex %+v", c)
		}
	}
	min, _ := mesh.PositionAtVertex(0, 0)
	max, _ := mesh.PositionAtVertex(64, 64)
	if min.X != -4096 || min.Z != -4096 || max.X != 4096 || max.Z != 4096 {
		t.Fatalf("border extents wrong: min=%+v max=%+v", min, max)
	}
}

func TestMeshFlatMapFSV(t *testing.T) {
	m, err := litmapdata.Load(flatMapFS(), "data/maps/flat")
	if err != nil {
		t.Fatal(err)
	}
	mesh, err := Build(m)
	if err != nil {
		t.Fatal(err)
	}
	samples, maxDiff, err := CompareHeights(mesh, m, HundredVertexSamples(m.Width, m.Height))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("FSV terrain flat map vertices=%d triangles=%d maxDiff=%d samples=%+v", mesh.VertexCount, mesh.TriangleCount, maxDiff, samples[:4])
	if maxDiff != 0 || mesh.InvertedTriangles() != 0 {
		t.Fatalf("flat map should have exact heights and no inverted tris: diff=%d inverted=%d", maxDiff, mesh.InvertedTriangles())
	}
	for _, s := range samples {
		if s.MeshH != 0 || s.SimH != 0 {
			t.Fatalf("flat sample not zero: %+v", s)
		}
	}
}

func flatMapFS() fstest.MapFS {
	return fstest.MapFS{
		"assets/kaykit-hexagon/tree_single_A.glb": &fstest.MapFile{Data: []byte("stub")},
		"data/maps/flat/terrain.toml": &fstest.MapFile{Data: []byte(`version = 1
width = 2
height = 2
biome = "flat"
pathing-scale = 4

[[start]]
player = 0
cell = [0, 0]
`)},
		"data/maps/flat/pathing.txt":  &fstest.MapFile{Data: []byte("@repeat 8 3*8\n")},
		"data/maps/flat/cliff.txt":    &fstest.MapFile{Data: []byte("@repeat 8 0*8\n")},
		"data/maps/flat/height.txt":   &fstest.MapFile{Data: []byte("@repeat 3 0*3\n")},
		"data/maps/flat/splat.txt":    &fstest.MapFile{Data: []byte("@repeat 2 255,0,0,0*2\n")},
		"data/maps/flat/doodads.toml": &fstest.MapFile{Data: []byte("")},
	}
}
