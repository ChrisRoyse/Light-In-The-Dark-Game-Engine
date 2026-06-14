package sim

// Terrain heightfield (#371 item 3; regions-rects-locations.md hazard 1).
// GetLocationZ in WC3 samples the render-coupled terrain mesh; that value
// is unavailable to a headless deterministic sim and would diverge by
// platform. Instead the sim owns a regular grid of fixed-point height
// samples that map data populates, and TerrainHeight bilinearly
// interpolates it — a pure, deterministic read.
//
// Unbound (no map heightfield loaded) the world is genuinely flat at
// height 0: TerrainHeight returns 0 because the *stored* terrain is flat,
// not because the accessor ignores its input. Once BindHeightfield loads
// real samples, the same accessor reads them — no fake SoT.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// heightfield is a row-major grid of height samples on a regular lattice.
// samples[r*cols + c] is the height at (originX + c*cellSize,
// originY + r*cellSize). cols==0 means unbound (flat world).
type heightfield struct {
	cols, rows         int32
	originX, originY    fixed.F64
	cellSize           fixed.F64
	samples            []fixed.F64
}

// BindHeightfield installs the terrain height grid. Fail-closed: a
// non-positive cell size, a degenerate grid (< 1×1), or a sample count
// that does not equal cols*rows is refused and leaves the world flat.
// Rebinding is allowed (a new map replaces the old field).
func (w *World) BindHeightfield(cols, rows int32, originX, originY, cellSize fixed.F64, samples []fixed.F64) bool {
	if cols < 1 || rows < 1 || cellSize <= 0 || int(cols)*int(rows) != len(samples) {
		return false
	}
	w.height = heightfield{
		cols: cols, rows: rows,
		originX: originX, originY: originY, cellSize: cellSize,
		samples: append([]fixed.F64(nil), samples...),
	}
	return true
}

// clampIdx pins a lattice index into [-1, n] (one past each border is
// enough — sampleAt clamps the c/c+1 pair the rest of the way) so the
// subsequent int32 cast cannot overflow on a pathological coordinate.
func clampIdx(v int64, n int32) int64 {
	if v < -1 {
		return -1
	}
	if v > int64(n) {
		return int64(n)
	}
	return v
}

// sampleAt reads the grid sample at clamped integer lattice coords.
func (h *heightfield) sampleAt(c, r int32) fixed.F64 {
	if c < 0 {
		c = 0
	} else if c >= h.cols {
		c = h.cols - 1
	}
	if r < 0 {
		r = 0
	} else if r >= h.rows {
		r = h.rows - 1
	}
	return h.samples[r*h.cols+c]
}

// TerrainHeight returns the bilinearly interpolated terrain height at a
// world point. 0 when no heightfield is bound (flat world). Positions
// outside the lattice clamp to the border samples (no extrapolation).
// GetLocationZ.
func (w *World) TerrainHeight(x, y fixed.F64) fixed.F64 {
	h := &w.height
	if h.cols == 0 {
		return 0
	}
	// lattice coordinates: lx = (x - originX) / cellSize.
	lx := x.Sub(h.originX).Div(h.cellSize)
	ly := y.Sub(h.originY).Div(h.cellSize)
	cx := lx.Floor()
	cy := ly.Floor()
	tx := lx.Sub(fixed.F64(cx) << 32) // fractional part in [0,1)
	ty := ly.Sub(fixed.F64(cy) << 32)
	if tx < 0 {
		tx = 0
	}
	if ty < 0 {
		ty = 0
	}
	// clamp the lattice index into a range sampleAt resolves to the border
	// before the int32 cast, so a far off-grid coordinate cannot overflow.
	cx = clampIdx(cx, h.cols)
	cy = clampIdx(cy, h.rows)
	c, r := int32(cx), int32(cy)
	h00 := h.sampleAt(c, r)
	h10 := h.sampleAt(c+1, r)
	h01 := h.sampleAt(c, r+1)
	h11 := h.sampleAt(c+1, r+1)
	// bilerp: interpolate along x on both rows, then along y.
	top := h00.Add(h10.Sub(h00).Mul(tx))
	bot := h01.Add(h11.Sub(h01).Mul(tx))
	return top.Add(bot.Sub(top).Mul(ty))
}
