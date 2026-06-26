package render

import "github.com/g3n/engine/math32"

// Edge-pan + middle-drag camera panning (#36; camera-and-culling.md §2.3,
// input.md §6 / R-INP-1.5).
//
// WC3 muscle-memory panning, as pure value math the input layer drives:
//   - Edge-pan: cursor inside an N-px screen-edge band scrolls the camera at
//     pan_speed = k*zoom; corners pan diagonally at the SAME speed as a straight
//     edge (the direction is normalized). Arrow keys reuse the same path.
//   - Middle-drag: a single terrain pick on button-down records the grabbed
//     world point; thereafter the anchor is corrected so that point stays
//     exactly under the cursor (1:1 grab, not velocity-based).
//   - All panning is clamped to a margin-inflated map bounding rect, so the
//     camera never scrolls into the void.
//
// This file owns no GL state and no camera object: it returns anchor deltas and
// clamped anchors. The caller applies them to the RTSCamera and supplies the
// per-frame terrain pick for the drag. Pure float32, deterministic.

// DefaultEdgeBandPx is the default edge-pan band width in pixels.
const DefaultEdgeBandPx = 8

// PanConfig configures the pan controller.
type PanConfig struct {
	EdgeBandPx int     // edge band width in pixels (default 8)
	PanSpeed   float32 // k in pan_speed = k*zoom, world units per second per zoom unit
	Margin     float32 // map-bounds inflation, world units (how far past the edge the anchor may sit)
}

// DefaultPanConfig returns sensible defaults.
func DefaultPanConfig() PanConfig {
	return PanConfig{EdgeBandPx: DefaultEdgeBandPx, PanSpeed: 1, Margin: 0}
}

// PanController holds pan configuration, the clamp rect, and middle-drag state.
type PanController struct {
	cfg                    PanConfig
	minX, maxX, minZ, maxZ float32
	haveBounds             bool
	dragging               bool
	grab                   math32.Vector3
}

// NewPanController builds a controller, applying defaults for non-positive
// fields.
func NewPanController(cfg PanConfig) *PanController {
	if cfg.EdgeBandPx <= 0 {
		cfg.EdgeBandPx = DefaultEdgeBandPx
	}
	if cfg.PanSpeed <= 0 {
		cfg.PanSpeed = 1
	}
	return &PanController{cfg: cfg}
}

// SetMapBounds installs the map bounding rect (render-world XZ); the clamp rect
// is this inflated by the configured margin on every side.
func (p *PanController) SetMapBounds(minX, minZ, maxX, maxZ float32) {
	m := p.cfg.Margin
	p.minX, p.maxX = minX-m, maxX+m
	p.minZ, p.maxZ = minZ-m, maxZ+m
	p.haveBounds = true
}

// EdgePanDelta returns the world-space (dx,dz) anchor delta for a cursor in the
// edge band over dt seconds at the given zoom. suppressed (active drag-select,
// focus loss, modal dialog) forces a zero delta. The magnitude is
// PanSpeed*zoom*dt for both straight-edge and corner (diagonal) pans — the
// direction is normalized so a corner is not √2 faster.
func (p *PanController) EdgePanDelta(cursorX, cursorY, viewW, viewH int, zoom, dt float32, suppressed bool) (float32, float32) {
	if suppressed || viewW <= 0 || viewH <= 0 {
		return 0, 0
	}
	b := p.cfg.EdgeBandPx
	var ux, uz float32
	if cursorX < b {
		ux = -1
	} else if cursorX >= viewW-b {
		ux = 1
	}
	if cursorY < b {
		uz = -1 // top of screen → north (−Z)
	} else if cursorY >= viewH-b {
		uz = 1
	}
	return p.scaledDir(ux, uz, zoom, dt)
}

// ArrowPanDelta returns the pan delta for held arrow keys, reusing the edge-pan
// path (same normalization and speed).
func (p *PanController) ArrowPanDelta(left, right, up, down bool, zoom, dt float32) (float32, float32) {
	var ux, uz float32
	if left {
		ux -= 1
	}
	if right {
		ux += 1
	}
	if up {
		uz -= 1
	}
	if down {
		uz += 1
	}
	return p.scaledDir(ux, uz, zoom, dt)
}

// scaledDir normalizes (ux,uz) and scales it to the pan speed.
func (p *PanController) scaledDir(ux, uz, zoom, dt float32) (float32, float32) {
	if ux == 0 && uz == 0 {
		return 0, 0
	}
	l := math32.Sqrt(ux*ux + uz*uz)
	speed := p.cfg.PanSpeed * zoom * dt
	return ux / l * speed, uz / l * speed
}

// BeginDrag starts a middle-drag, recording the world point grabbed under the
// cursor at button-down (one terrain pick, supplied by the caller).
func (p *PanController) BeginDrag(grabWorld math32.Vector3) {
	p.dragging = true
	p.grab = grabWorld
}

// Dragging reports whether a middle-drag is active.
func (p *PanController) Dragging() bool { return p.dragging }

// EndDrag ends the middle-drag.
func (p *PanController) EndDrag() { p.dragging = false }

// DragAnchorDelta returns the anchor delta that pulls the grabbed world point
// back under the cursor, given the world point currently under the cursor (a
// fresh terrain pick taken at the current anchor this frame). For a locked
// camera the screen→ground map is affine, so this is an exact 1:1 correction.
// Returns zero when not dragging.
func (p *PanController) DragAnchorDelta(worldUnderCursorNow math32.Vector3) (float32, float32) {
	if !p.dragging {
		return 0, 0
	}
	return p.grab.X - worldUnderCursorNow.X, p.grab.Z - worldUnderCursorNow.Z
}

// ClampAnchor clamps an anchor to the margin-inflated map rect. A no-op until
// SetMapBounds is called.
func (p *PanController) ClampAnchor(a math32.Vector3) math32.Vector3 {
	if !p.haveBounds {
		return a
	}
	a.X = clampF(a.X, p.minX, p.maxX)
	a.Z = clampF(a.Z, p.minZ, p.maxZ)
	return a
}

func clampF(v, lo, hi float32) float32 {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}
