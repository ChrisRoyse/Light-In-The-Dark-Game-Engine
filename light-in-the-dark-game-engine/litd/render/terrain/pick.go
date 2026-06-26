package terrain

import (
	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/math32"
)

// Terrain picking by ray-marching the height function (#81; terrain.md §1.5,
// §5.1–5.4).
//
// All terrain picking routes through the canonical height *function*, never the
// tessellated render mesh. The height function here is the continuous bilinear
// interpolation of the map's per-vertex heights — the same surface the mesh is
// built from, but evaluated analytically so the result is identical headless
// and rendered and never depends on triangle subdivision. A screen click is
// turned into a world-space ray by the caller (camera Unproject); this code
// marches that ray against the surface, refines the crossing by bisection, and
// reports the world hit point and the pathing cell under it.
//
// Determinism: pure float32 math with fixed step and iteration counts; no map
// iteration, no RNG. Same ray + same map → bit-identical PickResult, with or
// without a window. This is the deterministic, headless-testable picker the
// click-order and camera-pan paths consume.

// HeightSampler returns the terrain surface height (render-world Y) at render
// world ground coords (x, z) and whether that ground point lies on the map.
// Off-map points return ok=false so the marcher can detect leaving the world.
type HeightSampler interface {
	SampleHeight(x, z float32) (h float32, onMap bool)
}

// MapHeightSampler samples a loaded map's vertex heights as a continuous
// bilinear surface in render-world space, using the exact world transform the
// mesh builder uses (centered on the map, CellSize units per terrain cell).
type MapHeightSampler struct {
	m            *litmapdata.Map
	halfW, halfH float32
}

// NewMapHeightSampler builds a sampler over m. Returns nil for a nil map.
func NewMapHeightSampler(m *litmapdata.Map) *MapHeightSampler {
	if m == nil {
		return nil
	}
	return &MapHeightSampler{
		m:     m,
		halfW: float32(m.Width*CellSize) * 0.5,
		halfH: float32(m.Height*CellSize) * 0.5,
	}
}

// SampleHeight bilinearly interpolates the four surrounding vertex heights.
func (s *MapHeightSampler) SampleHeight(x, z float32) (float32, bool) {
	u := (x + s.halfW) / CellSize
	v := (z + s.halfH) / CellSize
	if u < 0 || v < 0 || u > float32(s.m.Width) || v > float32(s.m.Height) {
		return 0, false
	}
	x0 := int(math32.Floor(u))
	z0 := int(math32.Floor(v))
	if x0 >= s.m.Width {
		x0 = s.m.Width - 1
	}
	if z0 >= s.m.Height {
		z0 = s.m.Height - 1
	}
	fx := u - float32(x0)
	fz := v - float32(z0)
	h00, _ := s.m.HeightAtVertex(x0, z0)
	h10, _ := s.m.HeightAtVertex(x0+1, z0)
	h01, _ := s.m.HeightAtVertex(x0, z0+1)
	h11, _ := s.m.HeightAtVertex(x0+1, z0+1)
	top := lerp(float32(h00), float32(h10), fx)
	bot := lerp(float32(h01), float32(h11), fx)
	return lerp(top, bot, fz), true
}

func lerp(a, b, t float32) float32 { return a + (b-a)*t }

// PickResult is the outcome of a terrain pick.
type PickResult struct {
	World     math32.Vector3 // surface hit point in render-world space
	PathCellX int32          // pathing cell under the hit (valid iff OnMap)
	PathCellY int32
	Hit       bool // the ray crossed the surface within MaxDist
	OnMap     bool // the hit point lies within the map's pathing bounds
}

// Picker ray-marches a HeightSampler. Zero value is usable (defaults applied).
type Picker struct {
	Step    float32 // march increment in world units (default CellSize/4)
	MaxDist float32 // maximum ray length to march (default 1<<20)
	Refine  int     // bisection iterations after a crossing (default 24)
}

func (p Picker) withDefaults() Picker {
	if p.Step <= 0 {
		p.Step = CellSize / 4
	}
	if p.MaxDist <= 0 {
		p.MaxDist = 1 << 20
	}
	if p.Refine <= 0 {
		p.Refine = 24
	}
	return p
}

// residual returns pointY - surfaceHeight at the ray sample for parameter t,
// and whether the sample is on the map. A positive residual means the ray is
// above the surface, negative means below.
func sampleResidual(origin, dir math32.Vector3, t float32, s HeightSampler) (res float32, onMap bool) {
	px := origin.X + dir.X*t
	py := origin.Y + dir.Y*t
	pz := origin.Z + dir.Z*t
	h, ok := s.SampleHeight(px, pz)
	if !ok {
		return 0, false
	}
	return py - h, true
}

// Pick marches the ray (origin, dir) against the surface and returns the first
// (nearest-to-origin) crossing. dir need not be normalized. For a top-down RTS
// camera the nearest crossing is the upper/near cell at a cliff edge, so picks
// land on the terrain surface, never floating mid-wall. If the ray never
// crosses the surface on the map within MaxDist, Hit is false.
func (p Picker) Pick(origin, dir math32.Vector3, s HeightSampler) PickResult {
	p = p.withDefaults()
	if s == nil {
		return PickResult{}
	}
	d := dir
	if l := d.Length(); l > 0 {
		d.MultiplyScalar(1 / l)
	} else {
		return PickResult{}
	}

	prevT := float32(0)
	prevRes, prevOn := sampleResidual(origin, d, 0, s)
	for t := p.Step; t <= p.MaxDist; t += p.Step {
		res, on := sampleResidual(origin, d, t, s)
		if on && prevOn && prevRes > 0 && res <= 0 {
			// Crossing bracketed in (prevT, t]; bisect for the surface point.
			lo, hi := prevT, t
			for i := 0; i < p.Refine; i++ {
				mid := (lo + hi) * 0.5
				mres, mon := sampleResidual(origin, d, mid, s)
				if !mon {
					// Stepped off the map inside the bracket; stop refining.
					hi = mid
					continue
				}
				if mres > 0 {
					lo = mid
				} else {
					hi = mid
				}
			}
			tHit := (lo + hi) * 0.5
			world := math32.Vector3{
				X: origin.X + d.X*tHit,
				Y: origin.Y + d.Y*tHit,
				Z: origin.Z + d.Z*tHit,
			}
			r := PickResult{World: world, Hit: true}
			if cx, cy, ok := PathCell(s, world.X, world.Z); ok {
				r.PathCellX, r.PathCellY, r.OnMap = cx, cy, true
			}
			return r
		}
		prevT, prevRes, prevOn = t, res, on
	}
	return PickResult{}
}

// PathCell maps a render-world ground point to its pathing cell, when the
// sampler is a map-backed sampler (others have no pathing grid). PathingScale
// sub-cells per terrain cell, matching the sim's pathing layout.
func PathCell(s HeightSampler, x, z float32) (cx, cy int32, ok bool) {
	ms, isMap := s.(*MapHeightSampler)
	if !isMap {
		return 0, 0, false
	}
	m := ms.m
	u := (x + ms.halfW) / CellSize // terrain-cell coordinate
	v := (z + ms.halfH) / CellSize
	cx = int32(math32.Floor(u * float32(litmapdata.PathingScale)))
	cy = int32(math32.Floor(v * float32(litmapdata.PathingScale)))
	if cx < 0 || cy < 0 || int(cx) >= m.PathingWidth || int(cy) >= m.PathingHeight {
		return cx, cy, false
	}
	return cx, cy, true
}
