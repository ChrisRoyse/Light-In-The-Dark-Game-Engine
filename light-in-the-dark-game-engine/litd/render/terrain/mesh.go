// Package terrain builds render geometry from immutable map terrain data.
package terrain

import (
	"fmt"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/math32"
)

const CellSize = 128

type Mesh struct {
	Geometry      *geometry.Geometry
	WidthCells    int
	HeightCells   int
	VertexCount   int
	TriangleCount int

	heights   []int32
	positions []math32.Vector3
	indices   []uint32
}

type HeightSample struct {
	X      int   `json:"x"`
	Y      int   `json:"y"`
	MeshH  int32 `json:"meshH"`
	SimH   int32 `json:"simH"`
	Diff   int32 `json:"diff"`
	WorldX int32 `json:"worldX"`
	WorldZ int32 `json:"worldZ"`
}

func Build(m *litmapdata.Map) (*Mesh, error) {
	if m == nil {
		return nil, fmt.Errorf("terrain: nil map")
	}
	wv, hv := m.Width+1, m.Height+1
	vertexCount := wv * hv
	positions := math32.NewArrayF32(0, vertexCount*3)
	normals := math32.NewArrayF32(0, vertexCount*3)
	uvs := math32.NewArrayF32(0, vertexCount*2)
	indices := math32.NewArrayU32(0, m.Width*m.Height*6)
	out := &Mesh{
		WidthCells:    m.Width,
		HeightCells:   m.Height,
		VertexCount:   vertexCount,
		TriangleCount: m.Width * m.Height * 2,
		heights:       make([]int32, vertexCount),
		positions:     make([]math32.Vector3, vertexCount),
		indices:       make([]uint32, 0, m.Width*m.Height*6),
	}
	halfW := float32(m.Width*CellSize) * 0.5
	halfH := float32(m.Height*CellSize) * 0.5
	for y := 0; y < hv; y++ {
		for x := 0; x < wv; x++ {
			h, ok := m.HeightAtVertex(x, y)
			if !ok {
				return nil, fmt.Errorf("terrain: missing height vertex (%d,%d)", x, y)
			}
			i := y*wv + x
			out.heights[i] = h
			pos := math32.Vector3{
				X: float32(x*CellSize) - halfW,
				Y: float32(h),
				Z: float32(y*CellSize) - halfH,
			}
			out.positions[i] = pos
			positions.AppendVector3(&pos)
			n := normalAt(m, x, y)
			normals.AppendVector3(&n)
			uvs.Append(float32(x)/float32(m.Width), float32(y)/float32(m.Height))
		}
	}
	for y := 0; y < m.Height; y++ {
		for x := 0; x < m.Width; x++ {
			a := uint32(y*wv + x)
			b := uint32((y+1)*wv + x)
			c := uint32((y+1)*wv + x + 1)
			d := uint32(y*wv + x + 1)
			indices.Append(a, b, d, b, c, d)
			out.indices = append(out.indices, a, b, d, b, c, d)
		}
	}
	geom := geometry.NewGeometry()
	geom.SetIndices(indices)
	geom.AddVBO(gls.NewVBO(positions).AddAttrib(gls.VertexPosition))
	geom.AddVBO(gls.NewVBO(normals).AddAttrib(gls.VertexNormal))
	geom.AddVBO(gls.NewVBO(uvs).AddAttrib(gls.VertexTexcoord))
	out.Geometry = geom
	return out, nil
}

func (m *Mesh) HeightAtVertex(x, y int) (int32, bool) {
	if m == nil || x < 0 || y < 0 || x > m.WidthCells || y > m.HeightCells {
		return 0, false
	}
	return m.heights[y*(m.WidthCells+1)+x], true
}

func (m *Mesh) PositionAtVertex(x, y int) (math32.Vector3, bool) {
	if m == nil || x < 0 || y < 0 || x > m.WidthCells || y > m.HeightCells {
		return math32.Vector3{}, false
	}
	return m.positions[y*(m.WidthCells+1)+x], true
}

func (m *Mesh) InvertedTriangles() int {
	if m == nil {
		return 0
	}
	inverted := 0
	for i := 0; i < len(m.indices); i += 3 {
		a := m.positions[m.indices[i]]
		b := m.positions[m.indices[i+1]]
		c := m.positions[m.indices[i+2]]
		ab := b.Clone().Sub(&a)
		ac := c.Clone().Sub(&a)
		cross := ab.Cross(ac)
		if cross.Y <= 0 {
			inverted++
		}
	}
	return inverted
}

func CompareHeights(mesh *Mesh, m *litmapdata.Map, samples [][2]int) ([]HeightSample, int32, error) {
	if mesh == nil || m == nil {
		return nil, 0, fmt.Errorf("terrain: nil compare input")
	}
	out := make([]HeightSample, 0, len(samples))
	var maxAbs int32
	for _, s := range samples {
		x, y := s[0], s[1]
		mh, ok := mesh.HeightAtVertex(x, y)
		if !ok {
			return nil, 0, fmt.Errorf("terrain: mesh missing height vertex (%d,%d)", x, y)
		}
		sh, ok := m.HeightAtVertex(x, y)
		if !ok {
			return nil, 0, fmt.Errorf("terrain: map missing height vertex (%d,%d)", x, y)
		}
		diff := mh - sh
		if diff < 0 {
			if -diff > maxAbs {
				maxAbs = -diff
			}
		} else if diff > maxAbs {
			maxAbs = diff
		}
		out = append(out, HeightSample{
			X: x, Y: y, MeshH: mh, SimH: sh, Diff: diff,
			WorldX: int32(x * CellSize), WorldZ: int32(y * CellSize),
		})
	}
	return out, maxAbs, nil
}

func HundredVertexSamples(widthCells, heightCells int) [][2]int {
	out := make([][2]int, 0, 100)
	for iy := 0; iy < 10; iy++ {
		y := (iy * heightCells) / 9
		for ix := 0; ix < 10; ix++ {
			x := (ix * widthCells) / 9
			out = append(out, [2]int{x, y})
		}
	}
	return out
}

func normalAt(m *litmapdata.Map, x, y int) math32.Vector3 {
	h := func(x, y int) float32 {
		v, ok := m.HeightAtVertex(x, y)
		if !ok {
			return 0
		}
		return float32(v)
	}
	x0, x1 := x-1, x+1
	if x0 < 0 {
		x0 = x
	}
	if x1 > m.Width {
		x1 = x
	}
	y0, y1 := y-1, y+1
	if y0 < 0 {
		y0 = y
	}
	if y1 > m.Height {
		y1 = y
	}
	dx := h(x1, y) - h(x0, y)
	dz := h(x, y1) - h(x, y0)
	n := math32.Vector3{X: -dx, Y: float32(CellSize * 2), Z: -dz}
	n.Normalize()
	return n
}
