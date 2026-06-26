package render

import (
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/texture"
)

// Fog-of-war texture consumer (fog-of-war-minimap-selection.md §1, §2.1–2.3,
// §5–6; M4 deliverable).
//
// The sim owns the truth: a per-player 2-bit fog grid (hidden / explored /
// visible) at FogTexSize×FogTexSize cells. This file is a pure read-only
// consumer — it unions the local player's grid with allied grids (shared
// vision), maps each cell to a luminance, exponentially blends a persistent
// CPU buffer toward that target so fog fades instead of popping, and uploads
// the buffer into one persistent R8 texture. Render never mutates the grid;
// the bridge is the same one-way sim→render shape as snapshot interpolation.
//
// The buffer is the source of truth this code is verified against: given a
// known grid state at a cell, the byte at that cell must reach the known
// luminance (hidden=0, explored≈40%, visible=full). 128×128 R8 ≈ 16 KB,
// allocated once; Update and Upload allocate nothing at steady state (R-GC).

const (
	// FogTexSize is the fog texture edge in cells. It mirrors the sim's
	// FogGridSize (path.GridSize / FogCellPathingSize = 512 / 4 = 128); the
	// constant is duplicated here so render does not import sim for a literal.
	FogTexSize = 128

	// Luminance levels written into the R8 buffer per fog state.
	FogHiddenLum   uint8 = 0   // unexplored — black
	FogExploredLum uint8 = 102 // seen-before — ~40% of 255, dimmed
	FogVisibleLum  uint8 = 255 // currently in sight — full bright

	// Sim fog-state values mirrored from sim.FogHidden/Explored/Visible. The
	// union takes the max, so the ordering hidden<explored<visible matters.
	fogStateHidden   uint8 = 0
	fogStateExplored uint8 = 1
	fogStateVisible  uint8 = 2

	// fogMaxPlayers bounds the union mask loop (sim.MaxPlayers).
	fogMaxPlayers = 16
)

// FogGridSource is the read-only sim contract render consumes. *sim.World
// satisfies it through its FogStateAt method, so renderdemo passes the live
// world while tests pass a synthetic grid with known cells — both feed the
// exact same code path, and the buffer bytes are the verdict.
type FogGridSource interface {
	// FogStateAt returns the fog state (0 hidden / 1 explored / 2 visible) for
	// player at fog cell (x,y); out-of-range reads return hidden.
	FogStateAt(player uint8, x, y int32) uint8
}

// FogTexture holds the persistent fog buffer and its GL texture. buf is the
// smoothed luminance actually uploaded (the SoT); target is the latest desired
// luminance from the grid union, kept so the exponential blend has a stable
// goal across frames.
type FogTexture struct {
	size    int
	buf     []byte // size*size smoothed luminance — uploaded; verified against
	target  []byte // size*size latest target luminance from the grid union
	blend   float32
	tex     *texture.Texture2D
	uploads int
}

// NewFogTexture allocates the fog buffers and fixes the exponential blend
// factor in (0,1]: 1 means instant (no smoothing), smaller fades slower. An
// out-of-range factor is clamped to 1 (fail to the visible, non-smoothed path
// rather than a silently frozen buffer).
func NewFogTexture(blend float32) *FogTexture {
	if blend <= 0 || blend > 1 {
		blend = 1
	}
	n := FogTexSize * FogTexSize
	return &FogTexture{
		size:   FogTexSize,
		buf:    make([]byte, n),
		target: make([]byte, n),
		blend:  blend,
	}
}

// Update reads the fog grid for every player whose bit is set in mask, unions
// their visibility per cell (max state), maps the result to a target
// luminance, and blends the persistent buffer toward it. mask should include
// the local player's own bit; allied bits add shared vision. Zero allocations.
func (f *FogTexture) Update(src FogGridSource, mask uint16) {
	size := int32(f.size)
	for y := int32(0); y < size; y++ {
		row := int(y) * f.size
		for x := int32(0); x < size; x++ {
			state := fogStateHidden
			for p := uint8(0); p < fogMaxPlayers; p++ {
				if mask&(1<<p) == 0 {
					continue
				}
				if s := src.FogStateAt(p, x, y); s > state {
					state = s
					if state == fogStateVisible {
						break // max reached, no ally can raise it further
					}
				}
			}
			idx := row + int(x)
			f.target[idx] = fogLum(state)
			f.buf[idx] = blendByte(f.buf[idx], f.target[idx], f.blend)
		}
	}
}

// fogLum maps a sim fog state to its render luminance.
func fogLum(state uint8) uint8 {
	switch state {
	case fogStateVisible:
		return FogVisibleLum
	case fogStateExplored:
		return FogExploredLum
	default:
		return FogHiddenLum
	}
}

// blendByte exponentially moves cur toward target by factor a, rounding to the
// nearest byte. It guarantees convergence: each step changes cur by at least 1
// in the right direction (so rounding can never stall short of target) and
// never overshoots. With a==1 it jumps straight to target.
func blendByte(cur, target uint8, a float32) uint8 {
	if cur == target {
		return target
	}
	next := float32(cur) + (float32(target)-float32(cur))*a
	r := int32(next + 0.5)
	if target > cur {
		if r <= int32(cur) {
			r = int32(cur) + 1
		}
		if r > int32(target) {
			r = int32(target)
		}
	} else {
		if r >= int32(cur) {
			r = int32(cur) - 1
		}
		if r < int32(target) {
			r = int32(target)
		}
	}
	return uint8(r)
}

// At returns the current smoothed luminance at buffer cell (x,y).
func (f *FogTexture) At(x, y int) uint8 { return f.buf[y*f.size+x] }

// Buffer returns the backing luminance slice (the uploaded SoT). The slice is
// stable across the texture's life — callers must not resize it.
func (f *FogTexture) Buffer() []byte { return f.buf }

// Size returns the texture edge length in cells.
func (f *FogTexture) Size() int { return f.size }

// EnsureTexture lazily creates the persistent R8 fog texture bound to the
// buffer slice. Created exactly once; the same pointer is returned thereafter.
// Construction is CPU-only (no GL until the renderer uploads), so it is safe
// headless.
func (f *FogTexture) EnsureTexture() *texture.Texture2D {
	if f.tex == nil {
		f.tex = texture.NewTexture2DFromData(f.size, f.size, gls.RED, gls.UNSIGNED_BYTE, gls.R8, f.buf)
	}
	return f.tex
}

// Upload marks the persistent texture for a GL re-send of the current buffer.
// It reuses the same backing slice (no allocation) and never recreates the
// texture, so steady-state fog updates do not churn GPU resources.
func (f *FogTexture) Upload() {
	f.EnsureTexture()
	f.tex.SetData(f.size, f.size, gls.RED, gls.UNSIGNED_BYTE, gls.R8, f.buf)
	f.uploads++
}

// Uploads reports how many times the buffer has been pushed to the texture.
func (f *FogTexture) Uploads() int { return f.uploads }

// VisibleToMask reports whether fog cell (x,y) is currently *visible* (not
// merely explored) to any player in mask. The sync pass uses live grid state
// — not the smoothed texture — so a fading cell still fogs gameplay correctly.
func VisibleToMask(src FogGridSource, mask uint16, x, y int32) bool {
	for p := uint8(0); p < fogMaxPlayers; p++ {
		if mask&(1<<p) == 0 {
			continue
		}
		if src.FogStateAt(p, x, y) == fogStateVisible {
			return true
		}
	}
	return false
}

// ShouldDrawEntity mirrors sim visibility into the entity sync pass. Own units
// are never fogged. An enemy or neutral entity draws only when its cell is
// currently visible to the viewer AND it is detectable — an invisible unit
// with no covering true-sight source is skipped even inside a visible cell.
func ShouldDrawEntity(isOwn, cellVisible, detectable bool) bool {
	if isOwn {
		return true
	}
	return cellVisible && detectable
}
