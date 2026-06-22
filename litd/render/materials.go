package render

import (
	"fmt"

	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/texture"
)

type AtlasMaterialKey struct {
	Atlas  string               `json:"atlas"`
	Preset litasset.AtlasPreset `json:"preset"`
}

type AtlasMaterialSnapshot struct {
	Key           AtlasMaterialKey `json:"key"`
	SourceWidth   int              `json:"sourceWidth"`
	SourceHeight  int              `json:"sourceHeight"`
	TextureWidth  int              `json:"textureWidth"`
	TextureHeight int              `json:"textureHeight"`
	UploadSHA256  string           `json:"uploadSHA256"`
	MaterialCount int              `json:"materialCount"`
}

type AtlasMaterial struct {
	Key      AtlasMaterialKey
	Upload   litasset.AtlasUpload
	Texture  *texture.Texture2D
	Material *material.Standard
}

type AtlasMaterialCache struct {
	entries map[AtlasMaterialKey]*AtlasMaterial
}

func NewAtlasMaterialCache() *AtlasMaterialCache {
	return &AtlasMaterialCache{entries: make(map[AtlasMaterialKey]*AtlasMaterial)}
}

func (c *AtlasMaterialCache) Material(src *litasset.AtlasSource, preset litasset.AtlasPreset) (*AtlasMaterial, error) {
	if c == nil {
		return nil, fmt.Errorf("atlas material cache is nil")
	}
	if src == nil {
		return nil, fmt.Errorf("atlas source is nil")
	}
	key := AtlasMaterialKey{Atlas: src.Name, Preset: preset}
	if existing := c.entries[key]; existing != nil {
		return existing, nil
	}
	img, upload, err := litasset.BuildAtlasUpload(src, preset)
	if err != nil {
		return nil, err
	}
	tex := texture.NewTexture2DFromRGBA(img)
	tex.SetMagFilter(gls.LINEAR)
	tex.SetMinFilter(gls.LINEAR_MIPMAP_LINEAR)
	tex.SetWrapS(gls.CLAMP_TO_EDGE)
	tex.SetWrapT(gls.CLAMP_TO_EDGE)

	mat := material.NewStandard(&math32.Color{R: 1, G: 1, B: 1})
	mat.SetSpecularColor(&math32.Color{R: 0, G: 0, B: 0})
	mat.SetShininess(1)
	mat.AddTexture(tex)

	entry := &AtlasMaterial{Key: key, Upload: upload, Texture: tex, Material: mat}
	c.entries[key] = entry
	return entry, nil
}

func (c *AtlasMaterialCache) Count() int {
	if c == nil {
		return 0
	}
	return len(c.entries)
}

func (c *AtlasMaterialCache) Snapshot(m *AtlasMaterial) AtlasMaterialSnapshot {
	if m == nil {
		return AtlasMaterialSnapshot{MaterialCount: c.Count()}
	}
	return AtlasMaterialSnapshot{
		Key:           m.Key,
		SourceWidth:   m.Upload.SourceWidth,
		SourceHeight:  m.Upload.SourceHeight,
		TextureWidth:  m.Texture.Width(),
		TextureHeight: m.Texture.Height(),
		UploadSHA256:  m.Upload.SHA256,
		MaterialCount: c.Count(),
	}
}

const (
	TeamColorSlots  = 13
	NeutralTeamSlot = 12
)

type TeamColorZone struct {
	MinU float32 `json:"minU"`
	MinV float32 `json:"minV"`
	MaxU float32 `json:"maxU"`
	MaxV float32 `json:"maxV"`
}

type TeamColorState struct {
	Slot      int           `json:"slot"`
	Color     [3]float32    `json:"color"`
	Zone      TeamColorZone `json:"zone"`
	HitFlash  float32       `json:"hitFlash"`
	FadeAlpha float32       `json:"fadeAlpha"`
	FogDim    float32       `json:"fogDim"`
	Enabled   bool          `json:"enabled"`
}

type TeamColorMesh struct {
	graphic.Graphic

	uniMm        gls.Uniform
	uniMVm       gls.Uniform
	uniMVPm      gls.Uniform
	uniNm        gls.Uniform
	uniTeamColor gls.Uniform
	uniTeamZone  gls.Uniform
	uniFxScalars gls.Uniform

	slot      int
	color     math32.Color
	zone      TeamColorZone
	hitFlash  float32
	fadeAlpha float32
	fogDim    float32
	enabled   bool
}

func NewTeamColorMesh(igeom geometry.IGeometry, imat material.IMaterial, slot int) (*TeamColorMesh, error) {
	color, err := TeamColor(slot)
	if err != nil {
		return nil, err
	}
	m := new(TeamColorMesh)
	m.Init(igeom, imat, slot, color)
	return m, nil
}

func (m *TeamColorMesh) Init(igeom geometry.IGeometry, imat material.IMaterial, slot int, color math32.Color) {
	m.Graphic.Init(m, igeom, gls.TRIANGLES)
	m.ShaderDefines.Set("LITD_TEAMCOLOR", "1")
	m.initTeamUniforms()
	m.slot = slot
	m.color = color
	m.zone = DefaultTeamColorZone()
	m.hitFlash = 0
	m.fadeAlpha = 1
	m.fogDim = 1
	m.enabled = true
	if imat != nil {
		m.AddMaterial(imat, 0, 0)
	}
}

func (m *TeamColorMesh) SetMaterial(imat material.IMaterial) {
	m.Graphic.ClearMaterials()
	m.Graphic.AddMaterial(m, imat, 0, 0)
}

func (m *TeamColorMesh) AddMaterial(imat material.IMaterial, start, count int) {
	m.Graphic.AddMaterial(m, imat, start, count)
}

func (m *TeamColorMesh) AddGroupMaterial(imat material.IMaterial, gindex int) {
	m.Graphic.AddGroupMaterial(m, imat, gindex)
}

func (m *TeamColorMesh) Clone() core.INode {
	clone := new(TeamColorMesh)
	clone.Graphic = *m.Graphic.Clone().(*graphic.Graphic)
	clone.SetIGraphic(clone)
	clone.initTeamUniforms()
	clone.slot = m.slot
	clone.color = m.color
	clone.zone = m.zone
	clone.hitFlash = m.hitFlash
	clone.fadeAlpha = m.fadeAlpha
	clone.fogDim = m.fogDim
	clone.enabled = m.enabled
	return clone
}

func (m *TeamColorMesh) RenderSetup(gs *gls.GLS, rinfo *core.RenderInfo) {
	mm := m.ModelMatrix()
	location := m.uniMm.Location(gs)
	gs.UniformMatrix4fv(location, 1, false, &mm[0])

	mvm := m.ModelViewMatrix()
	location = m.uniMVm.Location(gs)
	gs.UniformMatrix4fv(location, 1, false, &mvm[0])

	mvpm := m.ModelViewProjectionMatrix()
	location = m.uniMVPm.Location(gs)
	gs.UniformMatrix4fv(location, 1, false, &mvpm[0])

	var nm math32.Matrix3
	nm.GetNormalMatrix(mvm)
	location = m.uniNm.Location(gs)
	gs.UniformMatrix3fv(location, 1, false, &nm[0])

	location = m.uniTeamColor.Location(gs)
	gs.Uniform3f(location, m.color.R, m.color.G, m.color.B)

	location = m.uniTeamZone.Location(gs)
	gs.Uniform4f(location, m.zone.MinU, m.zone.MinV, m.zone.MaxU, m.zone.MaxV)

	enabled := float32(0)
	if m.enabled {
		enabled = 1
	}
	location = m.uniFxScalars.Location(gs)
	gs.Uniform4f(location, m.hitFlash, m.fadeAlpha, m.fogDim, enabled)
}

func (m *TeamColorMesh) SetTeamSlot(slot int) error {
	color, err := TeamColor(slot)
	if err != nil {
		return err
	}
	m.slot = slot
	m.color = color
	return nil
}

func (m *TeamColorMesh) SetTeamColor(color math32.Color) {
	m.slot = -1
	m.color = color
}

func (m *TeamColorMesh) SetTeamColorZone(zone TeamColorZone) error {
	if zone.MinU < 0 || zone.MinV < 0 || zone.MaxU > 1 || zone.MaxV > 1 || zone.MinU >= zone.MaxU || zone.MinV >= zone.MaxV {
		return fmt.Errorf("invalid team-color zone %+v", zone)
	}
	m.zone = zone
	return nil
}

func (m *TeamColorMesh) SetPresentationScalars(hitFlash, fadeAlpha, fogDim float32) {
	m.hitFlash = clamp01(hitFlash)
	m.fadeAlpha = clamp01(fadeAlpha)
	m.fogDim = clamp01(fogDim)
}

func (m *TeamColorMesh) SetTeamColorEnabled(enabled bool) {
	m.enabled = enabled
}

func (m *TeamColorMesh) TeamColorState() TeamColorState {
	return TeamColorState{
		Slot:      m.slot,
		Color:     [3]float32{m.color.R, m.color.G, m.color.B},
		Zone:      m.zone,
		HitFlash:  m.hitFlash,
		FadeAlpha: m.fadeAlpha,
		FogDim:    m.fogDim,
		Enabled:   m.enabled,
	}
}

func (m *TeamColorMesh) initTeamUniforms() {
	m.uniMm.Init("ModelMatrix")
	m.uniMVm.Init("ModelViewMatrix")
	m.uniMVPm.Init("MVP")
	m.uniNm.Init("NormalMatrix")
	m.uniTeamColor.Init("LitdTeamColor")
	m.uniTeamZone.Init("LitdTeamColorZone")
	m.uniFxScalars.Init("LitdFxScalars")
}

func TeamColor(slot int) (math32.Color, error) {
	if slot < 0 || slot >= TeamColorSlots {
		return math32.Color{}, fmt.Errorf("team color slot %d out of range [0,%d]", slot, TeamColorSlots-1)
	}
	return defaultTeamColors[slot], nil
}

func DefaultTeamColorZone() TeamColorZone {
	return TeamColorZone{MinU: 0, MinV: 0, MaxU: 0.5, MaxV: 1}
}

func clamp01(v float32) float32 {
	if v < 0 {
		return 0
	}
	if v > 1 {
		return 1
	}
	return v
}

var defaultTeamColors = [TeamColorSlots]math32.Color{
	{R: 0.80, G: 0.08, B: 0.08},
	{R: 0.10, G: 0.22, B: 0.82},
	{R: 0.00, G: 0.62, B: 0.64},
	{R: 0.45, G: 0.16, B: 0.72},
	{R: 0.94, G: 0.80, B: 0.16},
	{R: 0.93, G: 0.42, B: 0.08},
	{R: 0.12, G: 0.62, B: 0.18},
	{R: 0.88, G: 0.34, B: 0.68},
	{R: 0.50, G: 0.50, B: 0.50},
	{R: 0.42, G: 0.72, B: 0.96},
	{R: 0.05, G: 0.34, B: 0.14},
	{R: 0.46, G: 0.28, B: 0.12},
	{R: 0.72, G: 0.72, B: 0.66},
}
