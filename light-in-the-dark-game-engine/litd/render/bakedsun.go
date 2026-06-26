package render

import (
	"fmt"

	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/math32"
)

const VertexColorShaderDefine = "LITD_VERTEX_COLOR"

type BakedSunVertexColorSnapshot struct {
	VertexCount       int                     `json:"vertexCount"`
	ExistingColors    bool                    `json:"existingColors"`
	ShaderDefine      bool                    `json:"shaderDefine"`
	Config            litasset.BakedSunConfig `json:"config"`
	MinScalar         float32                 `json:"minScalar"`
	MaxScalar         float32                 `json:"maxScalar"`
	VertexColorBuffer bool                    `json:"vertexColorBuffer"`
}

func ApplyBakedSunVertexColors(igeom geometry.IGeometry, cfg litasset.BakedSunConfig) (BakedSunVertexColorSnapshot, error) {
	if igeom == nil {
		return BakedSunVertexColorSnapshot{}, fmt.Errorf("baked sun: geometry is nil")
	}
	geom := igeom.GetGeometry()
	if geom == nil {
		return BakedSunVertexColorSnapshot{}, fmt.Errorf("baked sun: geometry is nil")
	}
	normalVBO := geom.VBO(gls.VertexNormal)
	if normalVBO == nil {
		return BakedSunVertexColorSnapshot{}, fmt.Errorf("baked sun: geometry has no vertex normals")
	}
	vertexCount := geom.Items()
	if vertexCount <= 0 {
		return BakedSunVertexColorSnapshot{}, fmt.Errorf("baked sun: geometry has no vertices")
	}

	scalars := make([]float32, 0, vertexCount)
	minScalar, maxScalar := float32(1), float32(0)
	var firstErr error
	normalVBO.ReadVectors3(gls.VertexNormal, func(n math32.Vector3) bool {
		s, err := litasset.BakedSunScalar([3]float32{n.X, n.Y, n.Z}, cfg)
		if err != nil {
			firstErr = err
			return true
		}
		if len(scalars) == 0 || s < minScalar {
			minScalar = s
		}
		if len(scalars) == 0 || s > maxScalar {
			maxScalar = s
		}
		scalars = append(scalars, s)
		return false
	})
	if firstErr != nil {
		return BakedSunVertexColorSnapshot{}, firstErr
	}
	if len(scalars) != vertexCount {
		return BakedSunVertexColorSnapshot{}, fmt.Errorf("baked sun: normal count %d != vertex count %d", len(scalars), vertexCount)
	}

	colorVBO := geom.VBO(gls.VertexColor)
	existingColors := colorVBO != nil
	if colorVBO == nil {
		colors := make(math32.ArrayF32, 0, vertexCount*3)
		for _, s := range scalars {
			colors.Append(s, s, s)
		}
		geom.AddVBO(gls.NewVBO(colors).AddAttrib(gls.VertexColor))
	} else {
		idx := 0
		colorVBO.OperateOnVectors3(gls.VertexColor, func(c *math32.Vector3) bool {
			if idx >= len(scalars) {
				firstErr = fmt.Errorf("baked sun: color count exceeds vertex count %d", vertexCount)
				return true
			}
			c.MultiplyScalar(scalars[idx])
			idx++
			return false
		})
		if firstErr != nil {
			return BakedSunVertexColorSnapshot{}, firstErr
		}
		if idx != vertexCount {
			return BakedSunVertexColorSnapshot{}, fmt.Errorf("baked sun: color count %d != vertex count %d", idx, vertexCount)
		}
	}

	EnableVertexColor(geom)
	return BakedSunVertexColorSnapshot{
		VertexCount:       vertexCount,
		ExistingColors:    existingColors,
		ShaderDefine:      geom.ShaderDefines[VertexColorShaderDefine] == "1",
		Config:            cfg,
		MinScalar:         minScalar,
		MaxScalar:         maxScalar,
		VertexColorBuffer: geom.VBO(gls.VertexColor) != nil,
	}, nil
}

func EnableVertexColor(igeom geometry.IGeometry) {
	if igeom == nil || igeom.GetGeometry() == nil {
		return
	}
	igeom.GetGeometry().ShaderDefines.Set(VertexColorShaderDefine, "1")
}
