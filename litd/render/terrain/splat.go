package terrain

// Terrain splat texturing via load-time bake (#78, terrain.md §2.1/§4). The default
// path (terrain.md §4) is a BAKE: the per-cell 4-layer splat weights from the map
// (mapdata.SplatWeight{A,B,C,D}, summing to 255) are folded — at map load — into one
// RGB color per terrain mesh vertex by blending four representative ground-layer
// colors sampled from the biome atlas. Baking into vertex colors (rather than a
// runtime splat shader) keeps the low/unlit preset trivial: no per-fragment splat
// ALU at runtime, identical appearance across presets, and one material per chunk.
//
// This file is the pure bake (no GL): map + layer colors -> per-vertex RGB, matching
// the terrain mesh vertex order (i = y*(Width+1)+x). The GL side adds the resulting
// VBO as a VertexColor attribute.

import (
	"fmt"

	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/math32"
)

// vertexColorShaderDefine mirrors litd/render.VertexColorShaderDefine (the shader's
// LITD_VERTEX_COLOR toggle). It is duplicated rather than imported because terrain is
// a SUBpackage of litd/render, and importing the parent creates a cycle the moment
// render's own internal tests import terrain (#78 regression). Both must name the
// same shader define; this string is the contract.
const vertexColorShaderDefine = "LITD_VERTEX_COLOR"

// SplatLayers are the four representative ground colors the splat weights blend
// between — layer A/B/C/D, each the mean color of that layer's biome-atlas region.
// (R-RND-2: the layers are regions of the ONE biome atlas; the bake uses their
// representative color so the baked vertex-color path needs no atlas texels at
// runtime.)
type SplatLayers [4]math32.Color

// BakeVertexColors returns one RGB triple per terrain mesh vertex, blending the four
// layer colors by the splat weights of the cells surrounding each vertex.
//
// The mesh has (Width+1)*(Height+1) vertices; splat weights are per CELL
// (Width*Height). A vertex at grid (x,y) is shared by up to four cells —
// (x-1,y-1),(x,y-1),(x-1,y),(x,y) — so it averages their weights (clamped at the
// borders). Averaging at shared vertices is what makes blends smooth across cell
// boundaries: neighbouring cells contribute to the same vertex, so there is no hard
// seam at the cell edge. Output is a flat R,G,B array in mesh vertex order
// (i = y*(Width+1)+x), values in [0,1]; never NaN/negative (weights are normalized
// by their actual sum, and the map loader guarantees per-cell sums of 255).
func BakeVertexColors(m *litmapdata.Map, layers SplatLayers) []float32 {
	if m == nil || m.Width <= 0 || m.Height <= 0 {
		return nil
	}
	wv, hv := m.Width+1, m.Height+1
	out := make([]float32, wv*hv*3)
	for y := 0; y < hv; y++ {
		for x := 0; x < wv; x++ {
			var wa, wb, wc, wd, n float64
			// The up-to-4 cells that touch this vertex.
			for _, d := range [4][2]int{{x - 1, y - 1}, {x, y - 1}, {x - 1, y}, {x, y}} {
				cx, cy := d[0], d[1]
				if cx < 0 || cy < 0 || cx >= m.Width || cy >= m.Height {
					continue
				}
				sw, ok := m.SplatAt(cx, cy)
				if !ok {
					continue
				}
				wa += float64(sw.A)
				wb += float64(sw.B)
				wc += float64(sw.C)
				wd += float64(sw.D)
				n++
			}
			i := (y*wv + x) * 3
			if n == 0 {
				continue // leaves black; should not happen for an in-bounds vertex
			}
			// Normalize by the actual summed weight so the result is a convex blend
			// (sum of coefficients == 1) regardless of how many cells contributed.
			total := wa + wb + wc + wd
			if total == 0 {
				continue
			}
			r := (wa*float64(layers[0].R) + wb*float64(layers[1].R) + wc*float64(layers[2].R) + wd*float64(layers[3].R)) / total
			g := (wa*float64(layers[0].G) + wb*float64(layers[1].G) + wc*float64(layers[2].G) + wd*float64(layers[3].G)) / total
			b := (wa*float64(layers[0].B) + wb*float64(layers[1].B) + wc*float64(layers[2].B) + wd*float64(layers[3].B)) / total
			out[i] = float32(r)
			out[i+1] = float32(g)
			out[i+2] = float32(b)
		}
	}
	return out
}

// SplatSnapshot is the FSV/dump record for a splat bake applied to a geometry.
type SplatSnapshot struct {
	VertexCount    int  `json:"vertexCount"`
	ExistingColors bool `json:"existingColors"` // a VertexColor VBO already existed (composited)
	ShaderDefine   bool `json:"shaderDefine"`   // LITD_VERTEX_COLOR enabled on the geometry
	VertexColorVBO bool `json:"vertexColorVBO"` // a VertexColor VBO is present after apply
}

// ApplySplatVertexColors bakes the map's splat weights into per-vertex colors and
// installs them as the geometry's VertexColor attribute, enabling the
// LITD_VERTEX_COLOR shader path (the same buffer baked sun shading uses). If a
// VertexColor VBO already exists it is overwritten with the splat base (splat is the
// ground albedo; a later baked-sun pass multiplies shade into it). This is the
// load-time bake — once applied there is no per-fragment splat work, so the low/unlit
// preset renders the identical baked colors with no splat shader bound.
func ApplySplatVertexColors(igeom geometry.IGeometry, m *litmapdata.Map, layers SplatLayers) (SplatSnapshot, error) {
	if igeom == nil || igeom.GetGeometry() == nil {
		return SplatSnapshot{}, fmt.Errorf("splat: geometry is nil")
	}
	geom := igeom.GetGeometry()
	vertexCount := geom.Items()
	if vertexCount <= 0 {
		return SplatSnapshot{}, fmt.Errorf("splat: geometry has no vertices")
	}
	rgb := BakeVertexColors(m, layers)
	if len(rgb) != vertexCount*3 {
		return SplatSnapshot{}, fmt.Errorf("splat: baked %d colors != %d geometry vertices", len(rgb)/3, vertexCount)
	}
	colorVBO := geom.VBO(gls.VertexColor)
	existing := colorVBO != nil
	if colorVBO == nil {
		colors := make(math32.ArrayF32, len(rgb))
		copy(colors, rgb)
		geom.AddVBO(gls.NewVBO(colors).AddAttrib(gls.VertexColor))
	} else {
		idx := 0
		var over error
		colorVBO.OperateOnVectors3(gls.VertexColor, func(c *math32.Vector3) bool {
			if idx >= vertexCount {
				over = fmt.Errorf("splat: color count exceeds vertex count %d", vertexCount)
				return true
			}
			c.X, c.Y, c.Z = rgb[idx*3], rgb[idx*3+1], rgb[idx*3+2]
			idx++
			return false
		})
		if over != nil {
			return SplatSnapshot{}, over
		}
	}
	geom.ShaderDefines.Set(vertexColorShaderDefine, "1") // same effect as render.EnableVertexColor
	return SplatSnapshot{
		VertexCount:    vertexCount,
		ExistingColors: existing,
		ShaderDefine:   geom.ShaderDefines[vertexColorShaderDefine] == "1",
		VertexColorVBO: geom.VBO(gls.VertexColor) != nil,
	}, nil
}

// DefaultBiomeLayers is a representative grass→dirt→rock→sand layer set used by the
// splat FSV and as a fallback when a biome atlas exposes no layer swatches. Colors
// are plausible ground means (not magenta/black), so a baked map is never a debug
// color by accident.
func DefaultBiomeLayers() SplatLayers {
	return SplatLayers{
		{R: 0.36, G: 0.52, B: 0.24}, // A: grass
		{R: 0.52, G: 0.40, B: 0.26}, // B: dirt
		{R: 0.46, G: 0.46, B: 0.47}, // C: rock
		{R: 0.78, G: 0.72, B: 0.50}, // D: sand
	}
}
