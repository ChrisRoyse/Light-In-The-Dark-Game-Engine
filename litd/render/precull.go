package render

import "github.com/g3n/engine/math32"

// 2D ground-footprint pre-cull (camera-and-culling.md §4.2.2–4.2.3, §7).
//
// Before any G3N scene-graph work, entities are culled by their map-space XZ
// against the camera's ground footprint — the trapezoid the view frustum cuts
// out of the ground plane (a rectangle in orthographic). Everything outside is
// never synced, never animated, and its scene node stays detached/invisible, so
// a 128×128 map with thousands of entities only touches the ~300 cells actually
// on screen at maximum zoom.
//
// The footprint is inflated by a vertical margin (max terrain height + flying
// altitude) so a tall or airborne entity whose base sits just past the edge is
// still synced — no pop-in at cliffs. The cull is intentionally conservative:
// borderline entities are kept, never wrongly dropped.

// GroundFootprintCuller tests entity XZ positions against an inflated footprint
// polygon. Visibility lists are pooled and reused — Cull allocates nothing once
// warmed (R-GC-2).
type GroundFootprintCuller struct {
	corners [4]math32.Vector2 // footprint corners on the XZ plane, CCW
	margin  float32
	have    bool
	visible []int
	culled  []int
}

// SetFootprint installs the current camera footprint and the vertical margin,
// recomputed only when the footprint changes (per zoom change), not per frame.
// Corners are normalized to counter-clockwise winding so the edge test has a
// consistent inside sign. margin (world units) inflates the polygon outward.
func (c *GroundFootprintCuller) SetFootprint(fp RTSCameraFootprint, margin float32) {
	for i := 0; i < 4; i++ {
		c.corners[i] = math32.Vector2{X: fp.Corners[i].X, Y: fp.Corners[i].Z}
	}
	if signedArea(c.corners) < 0 {
		// Clockwise → reverse to CCW.
		c.corners[1], c.corners[3] = c.corners[3], c.corners[1]
	}
	if margin < 0 {
		margin = 0
	}
	c.margin = margin
	c.have = true
}

// Contains reports whether world point (x,z) is inside the footprint inflated
// by the margin. A point is inside when its inward signed distance to every
// edge is at least -margin (i.e. no further than margin outside any edge).
func (c *GroundFootprintCuller) Contains(x, z float32) bool {
	if !c.have {
		return true // no footprint set yet → cull nothing (fail-open is safe here: shows all)
	}
	p := math32.Vector2{X: x, Y: z}
	for i := 0; i < 4; i++ {
		a := c.corners[i]
		b := c.corners[(i+1)%4]
		ex, ez := b.X-a.X, b.Y-a.Y
		// Signed area of (a,b,p)*2 = cross; positive = left = inside for CCW.
		cross := ex*(p.Y-a.Y) - ez*(p.X-a.X)
		edgeLen := math32.Sqrt(ex*ex + ez*ez)
		if edgeLen == 0 {
			continue
		}
		dist := cross / edgeLen // signed perpendicular distance, inward positive
		if dist < -c.margin {
			return false
		}
	}
	return true
}

// Cull partitions entity slots by footprint containment, writing index lists
// into the pooled visible/culled slices (reset and refilled — zero allocations
// once warmed). xs and zs are parallel world-coordinate columns. Returns the
// two index slices (valid until the next Cull).
func (c *GroundFootprintCuller) Cull(xs, zs []float32) (visible, culled []int) {
	c.visible = c.visible[:0]
	c.culled = c.culled[:0]
	n := len(xs)
	if len(zs) < n {
		n = len(zs)
	}
	for i := 0; i < n; i++ {
		if c.Contains(xs[i], zs[i]) {
			c.visible = append(c.visible, i)
		} else {
			c.culled = append(c.culled, i)
		}
	}
	return c.visible, c.culled
}

// Reserve presizes the pooled lists so the first Cull of up to n entities does
// not allocate. Call once at startup with the peak entity count.
func (c *GroundFootprintCuller) Reserve(n int) {
	if cap(c.visible) < n {
		c.visible = make([]int, 0, n)
	}
	if cap(c.culled) < n {
		c.culled = make([]int, 0, n)
	}
}

// signedArea returns twice the signed area of the quad (positive = CCW).
func signedArea(p [4]math32.Vector2) float32 {
	a := float32(0)
	for i := 0; i < 4; i++ {
		j := (i + 1) % 4
		a += p[i].X*p[j].Y - p[j].X*p[i].Y
	}
	return a
}
