package render

import (
	"fmt"

	litasset "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset"
	"github.com/g3n/engine/gls"
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
