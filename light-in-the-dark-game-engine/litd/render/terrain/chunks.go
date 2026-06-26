package terrain

import (
	"fmt"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/math32"
)

// ChunkCellSpan is the default chunk edge in terrain cells/tiles (terrain.md
// §3): 16×16-cell chunks, so a 128×128-tile map bakes into 64 chunks.
const ChunkCellSpan = 16

// MaxChunkTriangles is the per-chunk triangle ceiling (batching-and-draw-calls.md
// §3). A bare 16×16 terrain chunk is 16·16·2 = 512 triangles; the headroom up to
// this cap absorbs the static doodads merged into the chunk (Stage B).
const MaxChunkTriangles = 8000

// Chunk is one baked terrain tile-block: a single Graphic / single draw call,
// one shared biome-atlas material, and a precomputed static world-space AABB
// that feeds G3N frustum culling. Vertices on a chunk's shared edge sit at the
// exact same world position as the neighbouring chunk's edge vertices (both are
// computed from the map-global vertex formula), so adjacent chunks meet with no
// seam crack.
type Chunk struct {
	Col, Row       int // chunk-grid coordinates
	CellX0, CellY0 int // inclusive tile origin
	CellX1, CellY1 int // exclusive tile bound
	VertexCount    int
	TriangleCount  int
	AABB           math32.Box3 // static world-space bounds (terrain only)

	// DoodadIDs are the static doodads whose anchor tile falls in this chunk.
	// Assignment is by the doodad's anchor cell alone, so a doodad whose
	// footprint straddles a chunk border is still owned by exactly one chunk.
	DoodadIDs []uint32

	Geometry  *geometry.Geometry
	positions []math32.Vector3
	normals   []math32.Vector3
	uvs       []float32
	indices   []uint32
}

// ChunkSet is the full chunked terrain for a map.
type ChunkSet struct {
	ChunkCells  int
	Cols, Rows  int
	WidthCells  int
	HeightCells int
	Chunks      []Chunk
}

// BuildChunks bakes a map into chunkCells×chunkCells-tile chunks. chunkCells<=0
// uses ChunkCellSpan. The map is partitioned edge-to-edge; the last row/column
// of chunks is clipped to the map bounds when the dimensions are not a multiple
// of chunkCells (no padding, no out-of-bounds geometry).
func BuildChunks(m *litmapdata.Map, chunkCells int) (*ChunkSet, error) {
	if m == nil {
		return nil, fmt.Errorf("terrain: nil map")
	}
	if chunkCells <= 0 {
		chunkCells = ChunkCellSpan
	}
	cols := ceilDiv(m.Width, chunkCells)
	rows := ceilDiv(m.Height, chunkCells)
	cs := &ChunkSet{
		ChunkCells:  chunkCells,
		Cols:        cols,
		Rows:        rows,
		WidthCells:  m.Width,
		HeightCells: m.Height,
		Chunks:      make([]Chunk, 0, cols*rows),
	}
	for row := 0; row < rows; row++ {
		for col := 0; col < cols; col++ {
			c, err := buildChunk(m, chunkCells, col, row)
			if err != nil {
				return nil, err
			}
			cs.Chunks = append(cs.Chunks, c)
		}
	}
	if err := assignDoodads(m, cs); err != nil {
		return nil, err
	}
	return cs, nil
}

func buildChunk(m *litmapdata.Map, chunkCells, col, row int) (Chunk, error) {
	x0, y0 := col*chunkCells, row*chunkCells
	x1, y1 := min(x0+chunkCells, m.Width), min(y0+chunkCells, m.Height)
	cw, ch := x1-x0, y1-y0 // tiles in this chunk
	vw, vh := cw+1, ch+1   // vertices span one past tiles on each axis

	// Map-global origin shift, identical to terrain.Build, so a vertex at global
	// (gx,gy) has the same world position in every chunk that includes it.
	halfW := float32(m.Width*CellSize) * 0.5
	halfH := float32(m.Height*CellSize) * 0.5

	vertexCount := vw * vh
	positions := math32.NewArrayF32(0, vertexCount*3)
	normals := math32.NewArrayF32(0, vertexCount*3)
	uvs := math32.NewArrayF32(0, vertexCount*2)
	indices := math32.NewArrayU32(0, cw*ch*6)

	c := Chunk{
		Col: col, Row: row,
		CellX0: x0, CellY0: y0, CellX1: x1, CellY1: y1,
		VertexCount:   vertexCount,
		TriangleCount: cw * ch * 2,
		positions:     make([]math32.Vector3, vertexCount),
		normals:       make([]math32.Vector3, vertexCount),
		uvs:           make([]float32, 0, vertexCount*2),
		indices:       make([]uint32, 0, cw*ch*6),
	}
	box := math32.NewBox3(nil, nil)
	box.MakeEmpty()
	for ly := 0; ly < vh; ly++ {
		for lx := 0; lx < vw; lx++ {
			gx, gy := x0+lx, y0+ly
			h, ok := m.HeightAtVertex(gx, gy)
			if !ok {
				return Chunk{}, fmt.Errorf("terrain: chunk (%d,%d) missing height vertex (%d,%d)", col, row, gx, gy)
			}
			pos := math32.Vector3{
				X: float32(gx*CellSize) - halfW,
				Y: float32(h),
				Z: float32(gy*CellSize) - halfH,
			}
			i := ly*vw + lx
			c.positions[i] = pos
			positions.AppendVector3(&pos)
			box.ExpandByPoint(&pos)
			n := normalAt(m, gx, gy)
			c.normals[i] = n
			normals.AppendVector3(&n)
			u, v := float32(gx)/float32(m.Width), float32(gy)/float32(m.Height)
			c.uvs = append(c.uvs, u, v)
			uvs.Append(u, v)
		}
	}
	for ly := 0; ly < ch; ly++ {
		for lx := 0; lx < cw; lx++ {
			a := uint32(ly*vw + lx)
			b := uint32((ly+1)*vw + lx)
			cc := uint32((ly+1)*vw + lx + 1)
			d := uint32(ly*vw + lx + 1)
			indices.Append(a, b, d, b, cc, d)
			c.indices = append(c.indices, a, b, d, b, cc, d)
		}
	}
	c.AABB = *box

	geom := geometry.NewGeometry()
	geom.SetIndices(indices)
	geom.AddVBO(gls.NewVBO(positions).AddAttrib(gls.VertexPosition))
	geom.AddVBO(gls.NewVBO(normals).AddAttrib(gls.VertexNormal))
	geom.AddVBO(gls.NewVBO(uvs).AddAttrib(gls.VertexTexcoord))
	c.Geometry = geom
	return c, nil
}

// assignDoodads bins every static doodad into the single chunk containing its
// anchor cell. Doodad cells are in pathing-grid units; chunks are tile-based,
// so the tile is the pathing cell divided by the pathing scale.
func assignDoodads(m *litmapdata.Map, cs *ChunkSet) error {
	for _, d := range m.Doodads() {
		tileX := d.X / litmapdata.PathingScale
		tileY := d.Y / litmapdata.PathingScale
		idx := cs.chunkIndexForTile(tileX, tileY)
		if idx < 0 {
			return fmt.Errorf("terrain: doodad %d tile (%d,%d) maps to no chunk", d.ID, tileX, tileY)
		}
		cs.Chunks[idx].DoodadIDs = append(cs.Chunks[idx].DoodadIDs, d.ID)
	}
	return nil
}

func (cs *ChunkSet) chunkIndexForTile(tileX, tileY int) int {
	if tileX < 0 || tileY < 0 || tileX >= cs.WidthCells || tileY >= cs.HeightCells {
		return -1
	}
	col := tileX / cs.ChunkCells
	row := tileY / cs.ChunkCells
	return row*cs.Cols + col
}

// VisibleChunks classifies chunks against a frustum by their static AABB,
// returning the indices of visible and culled chunks. This is the same test the
// renderer applies per Graphic; exposing it lets the FSV dump report the
// visible/culled split without reaching into renderer internals.
func (cs *ChunkSet) VisibleChunks(f *math32.Frustum) (visible, culled []int) {
	for i := range cs.Chunks {
		box := cs.Chunks[i].AABB
		if f.IntersectsBox(&box) {
			visible = append(visible, i)
		} else {
			culled = append(culled, i)
		}
	}
	return visible, culled
}

// WorldPosAt returns the world-space position of the global vertex (gx,gy) as
// stored in this chunk, or false if the vertex is outside the chunk's span.
// Used to prove seam continuity: a shared edge vertex resolves to the identical
// position in both adjacent chunks.
func (c *Chunk) WorldPosAt(gx, gy int) (math32.Vector3, bool) {
	if gx < c.CellX0 || gx > c.CellX1 || gy < c.CellY0 || gy > c.CellY1 {
		return math32.Vector3{}, false
	}
	vw := c.CellX1 - c.CellX0 + 1
	return c.positions[(gy-c.CellY0)*vw+(gx-c.CellX0)], true
}

// InvertedTriangles counts triangles whose geometric normal points downward
// (winding error), mirroring Mesh.InvertedTriangles for the chunk's buffers.
func (c *Chunk) InvertedTriangles() int {
	inverted := 0
	for i := 0; i+2 < len(c.indices); i += 3 {
		a := c.positions[c.indices[i]]
		b := c.positions[c.indices[i+1]]
		cc := c.positions[c.indices[i+2]]
		ab := b.Clone().Sub(&a)
		ac := cc.Clone().Sub(&a)
		if ab.Cross(ac).Y <= 0 {
			inverted++
		}
	}
	return inverted
}

// IndexOfVertexOwner returns the index of a chunk whose span includes the global
// vertex (gx,gy), or -1 if none. A vertex on a shared edge belongs to several
// chunks; the first is returned (they all store the identical world position).
func (cs *ChunkSet) IndexOfVertexOwner(gx, gy int) int {
	for i := range cs.Chunks {
		c := &cs.Chunks[i]
		if gx >= c.CellX0 && gx <= c.CellX1 && gy >= c.CellY0 && gy <= c.CellY1 {
			return i
		}
	}
	return -1
}

func ceilDiv(a, b int) int { return (a + b - 1) / b }
