// Package minimap contains deterministic world/pixel mapping shared by the
// runtime minimap and editor preview.
package minimap

// Mapper maps a rectangular world XZ range to a rectangular minimap pixel
// buffer. Pixel (0,0) is the north-west corner; +px is east, +py is south.
type Mapper struct {
	width, height          int
	minX, minZ, maxX, maxZ float32
}

// NewMapper builds a mapper for a minimap pixel buffer. Invalid dimensions or
// degenerate world ranges are allowed but report !Valid and map to zero.
func NewMapper(width, height int, minX, minZ, maxX, maxZ float32) Mapper {
	return Mapper{width: width, height: height, minX: minX, minZ: minZ, maxX: maxX, maxZ: maxZ}
}

// Valid reports whether the mapper can perform reversible coordinate mapping.
func (m Mapper) Valid() bool {
	return m.width > 0 && m.height > 0 && m.maxX > m.minX && m.maxZ > m.minZ
}

// Width returns the minimap buffer width in pixels.
func (m Mapper) Width() int { return m.width }

// Height returns the minimap buffer height in pixels.
func (m Mapper) Height() int { return m.height }

// WorldToPixel maps a world XZ point to its minimap pixel, clamped to the map.
func (m Mapper) WorldToPixel(x, z float32) (px, py int) {
	if !m.Valid() {
		return 0, 0
	}
	u := (x - m.minX) / (m.maxX - m.minX)
	v := (z - m.minZ) / (m.maxZ - m.minZ)
	px = clampPix(int(u*float32(m.width)), m.width)
	py = clampPix(int(v*float32(m.height)), m.height)
	return px, py
}

// PixelToWorld maps a minimap pixel center back to a world XZ point.
func (m Mapper) PixelToWorld(px, py int) (x, z float32) {
	if !m.Valid() {
		return 0, 0
	}
	cpx := clampPix(px, m.width)
	cpy := clampPix(py, m.height)
	u := (float32(cpx) + 0.5) / float32(m.width)
	v := (float32(cpy) + 0.5) / float32(m.height)
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
