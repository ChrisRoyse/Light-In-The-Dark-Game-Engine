package terrain

import (
	"os"
	"testing"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/math32"
)

func loadTest64(t *testing.T) *litmapdata.Map {
	t.Helper()
	m, err := litmapdata.Load(os.DirFS("../../.."), "data/maps/test64")
	if err != nil {
		t.Fatal(err)
	}
	return m
}

// frustumFromBox builds a frustum equal to an axis-aligned world box: six
// inward-facing planes. Inside a half-space is DistanceToPoint>=0 and
// DistanceToPoint = dot(normal,p)+constant (frustum.go IntersectsBox).
func frustumFromBox(b *math32.Box3) *math32.Frustum {
	return math32.NewFrustum(
		math32.NewPlane(math32.NewVector3(1, 0, 0), -b.Min.X), // x >= min.X
		math32.NewPlane(math32.NewVector3(-1, 0, 0), b.Max.X), // x <= max.X
		math32.NewPlane(math32.NewVector3(0, 1, 0), -b.Min.Y), // y >= min.Y
		math32.NewPlane(math32.NewVector3(0, -1, 0), b.Max.Y), // y <= max.Y
		math32.NewPlane(math32.NewVector3(0, 0, 1), -b.Min.Z), // z >= min.Z
		math32.NewPlane(math32.NewVector3(0, 0, -1), b.Max.Z), // z <= max.Z
	)
}

func TestBuildChunksCountAndCoverageFSV(t *testing.T) {
	m := loadTest64(t)
	cs, err := BuildChunks(m, ChunkCellSpan)
	if err != nil {
		t.Fatal(err)
	}
	// 64x64 tiles / 16 = 4x4 = 16 chunks.
	if cs.Cols != 4 || cs.Rows != 4 || len(cs.Chunks) != 16 {
		t.Fatalf("chunk grid wrong: cols=%d rows=%d n=%d", cs.Cols, cs.Rows, len(cs.Chunks))
	}
	totalTris := 0
	union := math32.NewBox3(nil, nil)
	union.MakeEmpty()
	for i := range cs.Chunks {
		c := &cs.Chunks[i]
		cw, ch := c.CellX1-c.CellX0, c.CellY1-c.CellY0
		wantTris := cw * ch * 2
		if c.TriangleCount != wantTris {
			t.Fatalf("chunk %d tris=%d want %d", i, c.TriangleCount, wantTris)
		}
		if c.TriangleCount > MaxChunkTriangles {
			t.Fatalf("chunk %d tris=%d exceeds cap %d", i, c.TriangleCount, MaxChunkTriangles)
		}
		if inv := c.InvertedTriangles(); inv != 0 {
			t.Fatalf("chunk %d has %d inverted triangles", i, inv)
		}
		totalTris += c.TriangleCount
		union.Union(&c.AABB)
	}
	t.Logf("FSV chunks n=%d totalTris=%d unionMin=%v unionMax=%v chunk0=%+v..%+v",
		len(cs.Chunks), totalTris, union.Min, union.Max,
		cs.Chunks[0].AABB.Min, cs.Chunks[0].AABB.Max)
	// Per-chunk tris sum to the whole-map mesh tri count (no gaps, no overlap of tiles).
	if totalTris != m.Width*m.Height*2 {
		t.Fatalf("chunk tris sum=%d, want whole-map %d", totalTris, m.Width*m.Height*2)
	}
	// AABB union must equal the whole-map world extent: X,Z in [-half,half], Y in [0,512].
	half := float32(m.Width*CellSize) * 0.5
	if union.Min.X != -half || union.Max.X != half || union.Min.Z != -half || union.Max.Z != half {
		t.Fatalf("AABB union XZ wrong: min=%v max=%v half=%v", union.Min, union.Max, half)
	}
	if union.Min.Y != 0 || union.Max.Y != 512 {
		t.Fatalf("AABB union Y wrong: min=%v max=%v want [0,512]", union.Min.Y, union.Max.Y)
	}
}

func TestChunkSeamContinuityFSV(t *testing.T) {
	m := loadTest64(t)
	cs, err := BuildChunks(m, ChunkCellSpan)
	if err != nil {
		t.Fatal(err)
	}
	left := &cs.Chunks[0]  // (col0,row0): tiles [0,16)
	right := &cs.Chunks[1] // (col1,row0): tiles [16,32)
	if left.CellX1 != 16 || right.CellX0 != 16 {
		t.Fatalf("adjacency wrong: left.x1=%d right.x0=%d", left.CellX1, right.CellX0)
	}
	half := float32(m.Width*CellSize) * 0.5
	mism := 0
	for gy := 0; gy <= 16; gy++ {
		lp, lok := left.WorldPosAt(16, gy)
		rp, rok := right.WorldPosAt(16, gy)
		if !lok || !rok || lp != rp {
			mism++
			continue
		}
		// And both must equal the map-global formula => crack-free against the
		// single-mesh Build too.
		wantX := float32(16*CellSize) - half
		if lp.X != wantX {
			t.Fatalf("seam vertex (16,%d) X=%v want %v", gy, lp.X, wantX)
		}
	}
	t.Logf("FSV seam shared-edge gx=16 rows=0..16 mismatches=%d sample=%v", mism, mustPos(t, left, 16, 8))
	if mism != 0 {
		t.Fatalf("%d seam vertices differ between adjacent chunks (cracks)", mism)
	}
}

func mustPos(t *testing.T, c *Chunk, gx, gy int) math32.Vector3 {
	t.Helper()
	p, ok := c.WorldPosAt(gx, gy)
	if !ok {
		t.Fatalf("no vertex (%d,%d) in chunk", gx, gy)
	}
	return p
}

func TestChunkClippedDimsFSV(t *testing.T) {
	m := loadTest64(t)
	// 64 / 24 => ceil = 3 cols/rows; last chunk spans tiles [48,64) = 16 tiles.
	cs, err := BuildChunks(m, 24)
	if err != nil {
		t.Fatal(err)
	}
	if cs.Cols != 3 || cs.Rows != 3 || len(cs.Chunks) != 9 {
		t.Fatalf("clipped grid wrong: cols=%d rows=%d n=%d", cs.Cols, cs.Rows, len(cs.Chunks))
	}
	last := &cs.Chunks[len(cs.Chunks)-1] // (col2,row2)
	t.Logf("FSV clipped last chunk bounds x[%d,%d) y[%d,%d) tris=%d",
		last.CellX0, last.CellX1, last.CellY0, last.CellY1, last.TriangleCount)
	if last.CellX0 != 48 || last.CellX1 != 64 || last.CellY0 != 48 || last.CellY1 != 64 {
		t.Fatalf("clipped chunk bounds wrong: %+v", last)
	}
	if last.CellX1 > m.Width || last.CellY1 > m.Height {
		t.Fatalf("clipped chunk leaves map: %+v", last)
	}
}

func TestChunkDoodadAssignmentFSV(t *testing.T) {
	m := loadTest64(t)
	cs, err := BuildChunks(m, ChunkCellSpan)
	if err != nil {
		t.Fatal(err)
	}
	// test64 doodads: pathing cells (40,42),(96,88),(176,170) => tiles /4 =>
	// (10,10),(24,22),(44,42) => chunks /16 => (0,0)=0, (1,1)=5, (2,2)=10.
	seen := map[uint32]int{}
	total := 0
	for i := range cs.Chunks {
		for _, id := range cs.Chunks[i].DoodadIDs {
			seen[id]++
			total++
			t.Logf("FSV doodad %d -> chunk idx %d (col%d,row%d)", id, i, cs.Chunks[i].Col, cs.Chunks[i].Row)
		}
	}
	if total != 3 {
		t.Fatalf("assigned %d doodads, want 3", total)
	}
	for id, n := range seen {
		if n != 1 {
			t.Fatalf("doodad %d assigned %d times, want exactly once", id, n)
		}
	}
	if got := cs.Chunks[0].DoodadIDs; len(got) != 1 || got[0] != 1 {
		t.Fatalf("doodad 1 should be in chunk 0: %v", got)
	}
	if got := cs.Chunks[5].DoodadIDs; len(got) != 1 || got[0] != 2 {
		t.Fatalf("doodad 2 should be in chunk 5: %v", got)
	}
	if got := cs.Chunks[10].DoodadIDs; len(got) != 1 || got[0] != 3 {
		t.Fatalf("doodad 3 should be in chunk 10: %v", got)
	}
}

func TestChunkFrustumCullingFSV(t *testing.T) {
	m := loadTest64(t)
	cs, err := BuildChunks(m, ChunkCellSpan)
	if err != nil {
		t.Fatal(err)
	}
	half := float32(m.Width*CellSize) * 0.5

	// Whole-map frustum: every chunk visible, none culled.
	all := frustumFromBox(math32.NewBox3(
		math32.NewVector3(-half, -1, -half),
		math32.NewVector3(half, 1000, half)))
	vis, cull := cs.VisibleChunks(all)
	t.Logf("FSV culling whole-map visible=%d culled=%d", len(vis), len(cull))
	if len(vis) != 16 || len(cull) != 0 {
		t.Fatalf("whole-map frustum: visible=%d culled=%d, want 16/0", len(vis), len(cull))
	}

	// Tight frustum over the (0,0) chunk's quadrant only: that chunk visible,
	// the far (3,3) chunk culled.
	near := frustumFromBox(math32.NewBox3(
		math32.NewVector3(-half, -1, -half),
		math32.NewVector3(-half+float32(ChunkCellSpan*CellSize), 1000, -half+float32(ChunkCellSpan*CellSize))))
	vis2, cull2 := cs.VisibleChunks(near)
	visSet := map[int]bool{}
	for _, i := range vis2 {
		visSet[i] = true
	}
	cullSet := map[int]bool{}
	for _, i := range cull2 {
		cullSet[i] = true
	}
	t.Logf("FSV culling near-quadrant visible=%v culled-count=%d", vis2, len(cull2))
	if !visSet[0] {
		t.Fatalf("chunk 0 (near) must be visible: vis=%v", vis2)
	}
	if !cullSet[15] {
		t.Fatalf("chunk 15 (far corner) must be culled: vis=%v", vis2)
	}
}
