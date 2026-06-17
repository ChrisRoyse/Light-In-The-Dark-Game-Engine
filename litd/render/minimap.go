package render

import (
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/texture"
)

// Minimap blip layer + viewport rect + click↔world mapping (#165;
// fog-of-war-minimap-selection.md §3, §4.3, §7.1.4).
//
// The minimap composites a cached top-down terrain RTT, the fog texture, a
// CPU-written blip texture, and a viewport line-loop. This file owns the
// deterministic, headless parts: the world↔minimap-pixel transform (and its
// exact inverse for click-to-world), the CPU blip raster into a preallocated
// 256×256 RGBA buffer (team-colored, visibility-respecting), and the camera
// footprint reduced to a 5-vertex line loop updated in place. The RTT base and
// the composite shader (the ≤6-draw-call budget) are wired at the GL boundary;
// the blip buffer here is the texture source and the verified source of truth.
//
// Pixel convention: minimap pixel (0,0) is the −X,−Z (north-west) map corner;
// +px is east (+X), +py is south (+Z). The transform is linear over the map
// rect, so the inverse is exact at pixel centers — clicks map back to world
// with sub-cell error by construction.

// MinimapSize is the blip/texture edge in pixels (matches the 1080p HUD).
const MinimapSize = 256

// Minimap holds the world rect, the CPU blip buffer, the viewport line loop,
// and the persistent blip texture.
type Minimap struct {
	size                   int
	minX, minZ, maxX, maxZ float32
	buf                    []byte // size*size*4 RGBA, the blip texture source
	tex                    *texture.Texture2D
	frustum                [5]math32.Vector2 // viewport line loop (last == first)
	uploads                int
}

// NewMinimap builds a minimap over the given world rect (render-world XZ).
func NewMinimap(minX, minZ, maxX, maxZ float32) *Minimap {
	return &Minimap{
		size: MinimapSize,
		minX: minX, minZ: minZ, maxX: maxX, maxZ: maxZ,
		buf: make([]byte, MinimapSize*MinimapSize*4),
	}
}

// Size returns the minimap edge in pixels.
func (m *Minimap) Size() int { return m.size }

// WorldToPixel maps a world XZ point to its minimap pixel, clamped to the map.
func (m *Minimap) WorldToPixel(x, z float32) (px, py int) {
	u := (x - m.minX) / (m.maxX - m.minX)
	v := (z - m.minZ) / (m.maxZ - m.minZ)
	px = clampPix(int(u*float32(m.size)), m.size)
	py = clampPix(int(v*float32(m.size)), m.size)
	return px, py
}

// PixelToWorld maps a minimap pixel (its center) back to a world XZ point — the
// exact inverse of WorldToPixel at pixel centers, used for click-to-world.
func (m *Minimap) PixelToWorld(px, py int) (x, z float32) {
	u := (float32(px) + 0.5) / float32(m.size)
	v := (float32(py) + 0.5) / float32(m.size)
	x = m.minX + u*(m.maxX-m.minX)
	z = m.minZ + v*(m.maxZ-m.minZ)
	return x, z
}

func clampPix(v, size int) int {
	if v < 0 {
		return 0
	}
	if v >= size {
		return size - 1
	}
	return v
}

// Clear resets the blip buffer to transparent black (zero), allocation-free.
func (m *Minimap) Clear() {
	for i := range m.buf {
		m.buf[i] = 0
	}
}

// PlotBlip stamps a sizePx×sizePx blip centered on the world point's pixel, in
// the given color. A non-visible blip (fogged enemy/neutral) is skipped — fog
// is respected on the minimap exactly as in the world. sizePx is typically 2
// for units, 3–4 for buildings. The block is clamped to the buffer edges.
func (m *Minimap) PlotBlip(x, z float32, sizePx int, c RGBA, visible bool) {
	if !visible || sizePx <= 0 {
		return
	}
	cx, cy := m.WorldToPixel(x, z)
	half := sizePx / 2
	r, g, b, a := toByte(c.R), toByte(c.G), toByte(c.B), toByte(c.A)
	for dy := 0; dy < sizePx; dy++ {
		py := cy - half + dy
		if py < 0 || py >= m.size {
			continue
		}
		for dx := 0; dx < sizePx; dx++ {
			px := cx - half + dx
			if px < 0 || px >= m.size {
				continue
			}
			o := (py*m.size + px) * 4
			m.buf[o], m.buf[o+1], m.buf[o+2], m.buf[o+3] = r, g, b, a
		}
	}
}

func toByte(v float32) byte {
	if v <= 0 {
		return 0
	}
	if v >= 1 {
		return 255
	}
	return byte(v*255 + 0.5)
}

// Frustum reduces a camera ground footprint to a 5-vertex minimap line loop
// (4 corners + closing vertex), updated in place. Returns the loop.
func (m *Minimap) Frustum(fp RTSCameraFootprint) [5]math32.Vector2 {
	for i := 0; i < 4; i++ {
		px, py := m.WorldToPixel(fp.Corners[i].X, fp.Corners[i].Z)
		m.frustum[i] = math32.Vector2{X: float32(px), Y: float32(py)}
	}
	m.frustum[4] = m.frustum[0] // close the loop
	return m.frustum
}

// At returns the RGBA at a blip-buffer pixel (for inspection/test).
func (m *Minimap) At(px, py int) RGBA {
	o := (py*m.size + px) * 4
	return RGBA{
		R: float32(m.buf[o]) / 255,
		G: float32(m.buf[o+1]) / 255,
		B: float32(m.buf[o+2]) / 255,
		A: float32(m.buf[o+3]) / 255,
	}
}

// Buffer returns the backing RGBA slice (the persistent texture source).
func (m *Minimap) Buffer() []byte { return m.buf }

// EnsureTexture lazily creates the persistent RGBA blip texture bound to the
// buffer (CPU-only until the renderer uploads).
func (m *Minimap) EnsureTexture() *texture.Texture2D {
	if m.tex == nil {
		m.tex = texture.NewTexture2DFromData(m.size, m.size, gls.RGBA, gls.UNSIGNED_BYTE, gls.RGBA8, m.buf)
	}
	return m.tex
}

// Upload marks the persistent texture for a GL re-send of the current buffer
// (same backing slice — no allocation, no texture recreation).
func (m *Minimap) Upload() {
	m.EnsureTexture()
	m.tex.SetData(m.size, m.size, gls.RGBA, gls.UNSIGNED_BYTE, gls.RGBA8, m.buf)
	m.uploads++
}

// Uploads reports how many times the buffer has been pushed to the texture.
func (m *Minimap) Uploads() int { return m.uploads }
