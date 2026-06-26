package render

import (
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
)

// fogTextureSlot is the GL texture unit the fog sampler binds to. It is kept
// clear of the terrain material's own texture slots (a flat-color terrain
// material uses none; even an atlas-textured one stays well below this), so the
// fog sampler never collides with MatTexture[].
const fogTextureSlot = 6

// FogTerrainMesh draws a world-space terrain surface with the per-fragment
// fog-of-war texture term (#161): the bound shader samples the fog texture by
// the fragment's world XZ and dims the surface in three zones (hidden ->
// near-black, explored -> ~40% dim + desaturated, visible -> full). The term
// lives inside the terrain's own draw via the LITD_FOG shader define, so it
// adds zero draw calls — exactly like TeamColorMesh layers the team-color term
// onto a unit's draw.
//
// The fog UV is taken from the fragment's true world XZ: the vertex shader maps
// the local position through this mesh's ModelMatrix before the affine UV, so a
// world-baked terrain chunk (identity model matrix) and a translated unit box
// (#536) both fog by their world position. The affine UV is
// uv = (worldXZ - origin) * invSize.
type FogTerrainMesh struct {
	graphic.Graphic

	uniMm    gls.Uniform
	uniMVm   gls.Uniform
	uniMVPm  gls.Uniform
	uniNm    gls.Uniform
	uniXform gls.Uniform
	uniCtrl  gls.Uniform

	fog     *FogTexture
	origin  math32.Vector2 // world-space XZ min corner the fog texture covers
	invSize math32.Vector2 // 1 / world XZ extent
	enabled bool
}

// NewFogTerrainMesh builds a terrain mesh that fogs itself from fog, mapping the
// world rectangle [origin, origin+worldSize] (XZ) onto the fog texture's UV
// space. A nil fog or zero worldSize disables the term (the surface renders
// undimmed) rather than sampling garbage — fail to the safe, visible path.
func NewFogTerrainMesh(igeom geometry.IGeometry, imat material.IMaterial, fog *FogTexture, origin, worldSize math32.Vector2) *FogTerrainMesh {
	m := new(FogTerrainMesh)
	m.Graphic.Init(m, igeom, gls.TRIANGLES)
	m.ShaderDefines.Set("LITD_FOG", "1")
	m.initFogUniforms()
	m.fog = fog
	m.origin = origin
	if worldSize.X != 0 {
		m.invSize.X = 1.0 / worldSize.X
	}
	if worldSize.Y != 0 {
		m.invSize.Y = 1.0 / worldSize.Y
	}
	m.enabled = fog != nil && worldSize.X != 0 && worldSize.Y != 0
	if fog != nil {
		// LINEAR filtering on the R8 fog texture gives the soft (bilinear) zone
		// edges the spec calls for; the sampler name routes it to LitdFogTex.
		tex := fog.EnsureTexture()
		tex.SetUniformNames("LitdFogTex", "LitdFogTexInfo")
		tex.SetMagFilter(gls.LINEAR)
		tex.SetMinFilter(gls.LINEAR)
	}
	if imat != nil {
		m.AddMaterial(imat, 0, 0)
	}
	return m
}

// AddMaterial adds a material to the mesh's draw, mirroring TeamColorMesh so the
// base terrain material (color + lighting) renders while this mesh supplies the
// fog uniforms and sampler.
func (m *FogTerrainMesh) AddMaterial(imat material.IMaterial, start, count int) {
	m.Graphic.AddMaterial(m, imat, start, count)
}

func (m *FogTerrainMesh) initFogUniforms() {
	m.uniMm.Init("ModelMatrix")
	m.uniMVm.Init("ModelViewMatrix")
	m.uniMVPm.Init("MVP")
	m.uniNm.Init("NormalMatrix")
	m.uniXform.Init("LitdFogXform")
	m.uniCtrl.Init("LitdFogControl")
}

// SetFogEnabled toggles the term at runtime without changing draw count — used
// by the FSV to prove the fog-on vs fog-off draw-call delta is zero.
func (m *FogTerrainMesh) SetFogEnabled(on bool) { m.enabled = on && m.fog != nil }

// FogEnabled reports whether the term is currently applied.
func (m *FogTerrainMesh) FogEnabled() bool { return m.enabled }

// RenderSetup sets the standard transform uniforms plus the fog affine + control
// uniforms and binds the fog texture to its dedicated unit. The renderer calls
// this before the attached material's RenderSetup, so the fog sampler is in
// place for the draw and is not disturbed by the (separate-slot) material.
func (m *FogTerrainMesh) RenderSetup(gs *gls.GLS, rinfo *core.RenderInfo) {
	mm := m.ModelMatrix()
	gs.UniformMatrix4fv(m.uniMm.Location(gs), 1, false, &mm[0])

	mvm := m.ModelViewMatrix()
	gs.UniformMatrix4fv(m.uniMVm.Location(gs), 1, false, &mvm[0])

	mvpm := m.ModelViewProjectionMatrix()
	gs.UniformMatrix4fv(m.uniMVPm.Location(gs), 1, false, &mvpm[0])

	var nm math32.Matrix3
	nm.GetNormalMatrix(mvm)
	gs.UniformMatrix3fv(m.uniNm.Location(gs), 1, false, &nm[0])

	gs.Uniform4f(m.uniXform.Location(gs), m.origin.X, m.origin.Y, m.invSize.X, m.invSize.Y)

	enabled := float32(0)
	if m.enabled && m.fog != nil {
		enabled = 1
	}
	gs.Uniform4f(m.uniCtrl.Location(gs), enabled, 0, 0, 0)

	if m.fog != nil {
		m.fog.EnsureTexture().RenderSetup(gs, fogTextureSlot, 0)
	}
}
