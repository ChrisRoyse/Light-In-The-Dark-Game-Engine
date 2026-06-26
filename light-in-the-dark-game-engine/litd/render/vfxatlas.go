package render

import (
	"image"
	"math"

	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/texture"
)

// VFXSprite selects a cell of the procedural VFX atlas. The missile/impact/aura
// billboards in cmd/renderdemo (-scene missiles, #528) sample these sub-rects
// instead of drawing debug boxes; a real art atlas can later replace the
// generated pixels through the exact same UV sub-rect contract.
type VFXSprite int

const (
	VFXSpriteMissile VFXSprite = iota // soft blue-white radial glow
	VFXSpriteImpact                   // hot orange radial burst
	VFXSpriteAura                     // green ring (hollow centre)
	VFXSpriteSpare                    // reserved cell
	vfxSpriteCount
)

// VFX atlas layout: a 2x2 grid of square cells. Kept small and power-of-two so
// the whole sheet is one cheap upload and every cell is an exact half-rect.
const (
	vfxAtlasCells = 2  // cells per axis
	vfxAtlasCell  = 64 // pixels per cell edge
	vfxAtlasSize  = vfxAtlasCells * vfxAtlasCell
)

// vfxCellCoord maps a sprite to its (col,row) cell in the 2x2 grid.
func vfxCellCoord(s VFXSprite) (col, row int) {
	switch s {
	case VFXSpriteMissile:
		return 0, 0
	case VFXSpriteImpact:
		return 1, 0
	case VFXSpriteAura:
		return 0, 1
	default: // VFXSpriteSpare and any out-of-range value
		return 1, 1
	}
}

// VFXAtlasSubRect returns the (u0,v0,u1,v1) sub-rect of the atlas covering the
// sprite's cell, in [0,1] texture coordinates. This is the UV contract the
// billboard pools carry and the renderer samples via Texture2D offset/repeat.
func VFXAtlasSubRect(s VFXSprite) math32.Vector4 {
	col, row := vfxCellCoord(s)
	const f = 1.0 / float32(vfxAtlasCells)
	u0 := float32(col) * f
	v0 := float32(row) * f
	return math32.Vector4{X: u0, Y: v0, Z: u0 + f, W: v0 + f}
}

// NewVFXAtlasImage builds the procedural VFX sprite sheet as straight-alpha
// RGBA. CPU-only (no GL), so it is safe to construct and assert on headless.
// Each cell is a radial field centred in the cell: a falloff glow, a hotter
// burst, and a hollow ring — distinguishable both visually and by pixel SoT.
func NewVFXAtlasImage() *image.RGBA {
	img := image.NewRGBA(image.Rect(0, 0, vfxAtlasSize, vfxAtlasSize))
	for s := VFXSprite(0); s < vfxSpriteCount; s++ {
		col, row := vfxCellCoord(s)
		ox, oy := col*vfxAtlasCell, row*vfxAtlasCell
		for py := 0; py < vfxAtlasCell; py++ {
			for px := 0; px < vfxAtlasCell; px++ {
				// Normalised distance from the cell centre, 0 at centre, 1 at
				// the inscribed edge (cell half-width).
				half := float64(vfxAtlasCell) / 2
				dx := (float64(px) + 0.5 - half) / half
				dy := (float64(py) + 0.5 - half) / half
				d := math.Sqrt(dx*dx + dy*dy)
				r, g, b, a := vfxSample(s, d)
				i := img.PixOffset(ox+px, oy+py)
				img.Pix[i+0] = clamp8(r)
				img.Pix[i+1] = clamp8(g)
				img.Pix[i+2] = clamp8(b)
				img.Pix[i+3] = clamp8(a)
			}
		}
	}
	return img
}

// vfxSample returns the straight-alpha colour for a sprite at normalised radius
// d (0 centre .. 1 inscribed edge). Returns 0..1 channel intensities.
func vfxSample(s VFXSprite, d float64) (r, g, b, a float64) {
	switch s {
	case VFXSpriteMissile:
		// Soft glow: smooth falloff from a white-hot core to a blue rim.
		i := clamp01f(1 - d)
		i *= i
		return 0.6 + 0.4*i, 0.8*i + 0.2, 1.0, i
	case VFXSpriteImpact:
		// Hot burst: brighter, tighter core; orange to yellow.
		i := clamp01f(1 - d)
		a = math.Pow(i, 1.5)
		return 1.0, 0.45 + 0.5*i, 0.12 * i, a
	case VFXSpriteAura:
		// Hollow ring peaking near 0.65 radius (a gaussian shell), green.
		shell := math.Exp(-((d - 0.65) * (d - 0.65)) / (2 * 0.16 * 0.16))
		return 0.35, 0.95, 0.5, shell
	default:
		return 0, 0, 0, 0
	}
}

// NewVFXAtlasTexture wraps the procedural atlas image in a GL texture configured
// for the given sprite's sub-rect, so a sampling material reads only that cell.
// LINEAR filtering + clamp keeps the sprite soft and avoids bleeding across the
// (single-pixel) cell seam. The texture upload is deferred to first render, so
// this stays headless-safe until actually drawn.
func NewVFXAtlasTexture(s VFXSprite) *texture.Texture2D {
	tex := texture.NewTexture2DFromRGBA(NewVFXAtlasImage())
	tex.SetMagFilter(gls.LINEAR)
	tex.SetMinFilter(gls.LINEAR)
	tex.SetWrapS(gls.CLAMP_TO_EDGE)
	tex.SetWrapT(gls.CLAMP_TO_EDGE)
	sub := VFXAtlasSubRect(s)
	// Offset+repeat select the cell: sample = sub.xy + uv*(sub.zw - sub.xy).
	tex.SetOffset(sub.X, sub.Y)
	tex.SetRepeat(sub.Z-sub.X, sub.W-sub.Y)
	return tex
}

func clamp01f(v float64) float64 {
	if v < 0 {
		return 0
	}
	if v > 1 {
		return 1
	}
	return v
}

func clamp8(v float64) uint8 {
	if v <= 0 {
		return 0
	}
	if v >= 1 {
		return 255
	}
	return uint8(v*255 + 0.5)
}
