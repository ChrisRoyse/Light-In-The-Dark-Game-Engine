package render

import "github.com/g3n/engine/math32"

// Pooled ground-decal selection circles (#167; fog-of-war-minimap-selection.md
// §4.1–4.3, §7.1.2; camera-and-culling.md §5.2 depth bias).
//
// Selection circles are quads draped on the terrain under selected units. They
// come from a preallocated pool (the shared pooled-quad infrastructure blob
// shadows and pings reuse), are tinted by the unit's relationship to the local
// player (self/ally/enemy/neutral), and have their four corners draped onto the
// ground by sampling the terrain height function at the corner XZ — so a circle
// on a slope follows the surface instead of clipping. Pure value math; the GL
// quad mesh + shared ring texture are wired at the render-graph boundary.

// Relationship is the local player's stance toward a unit, selecting the circle
// tint. The caller classifies via the sim/api (IsAlly/IsEnemy); render only
// maps it to a color.
type Relationship uint8

const (
	RelSelf Relationship = iota
	RelAlly
	RelEnemy
	RelNeutral
)

// RGBA is a render color channel tuple in [0,1].
type RGBA struct{ R, G, B, A float32 }

// RelationshipTint returns the selection-circle color for a relationship:
// self green, ally cyan-blue, enemy red, neutral gold.
func RelationshipTint(rel Relationship) RGBA {
	switch rel {
	case RelSelf:
		return RGBA{0, 1, 0, 1}
	case RelAlly:
		return RGBA{0, 0.6, 1, 1}
	case RelEnemy:
		return RGBA{1, 0, 0, 1}
	default:
		return RGBA{1, 0.85, 0, 1}
	}
}

// SelectionDecal is one pooled circle: a center on the XZ plane, a radius, a
// relationship tint, and the four ground-draped corner heights (CCW from the
// −X,−Z corner). Active marks the slot as in use.
type SelectionDecal struct {
	Center  math32.Vector2 // XZ
	Radius  float32
	Tint    RGBA
	CornerY [4]float32 // draped heights at (−,−),(+,−),(+,+),(−,+)
	Active  bool
}

// SelectionDecalPool is a fixed-size pool of selection circles.
type SelectionDecalPool struct {
	decals []SelectionDecal
	active int
}

// NewSelectionDecalPool preallocates a pool of cap circles.
func NewSelectionDecalPool(capacity int) *SelectionDecalPool {
	if capacity < 0 {
		capacity = 0
	}
	return &SelectionDecalPool{decals: make([]SelectionDecal, capacity)}
}

// Cap returns the pool capacity.
func (p *SelectionDecalPool) Cap() int { return len(p.decals) }

// ActiveCount returns how many circles are currently in use.
func (p *SelectionDecalPool) ActiveCount() int { return p.active }

// Acquire claims a free slot for a circle at center with radius and tint, and
// returns its index. ok is false when the pool is full (fail-closed: never
// grows mid-frame, never allocates).
func (p *SelectionDecalPool) Acquire(center math32.Vector2, radius float32, rel Relationship) (int, bool) {
	for i := range p.decals {
		if !p.decals[i].Active {
			p.decals[i] = SelectionDecal{
				Center: center, Radius: radius, Tint: RelationshipTint(rel), Active: true,
			}
			p.active++
			return i, true
		}
	}
	return -1, false
}

// Release frees the slot at i.
func (p *SelectionDecalPool) Release(i int) {
	if i < 0 || i >= len(p.decals) || !p.decals[i].Active {
		return
	}
	p.decals[i] = SelectionDecal{}
	p.active--
}

// ReleaseAll clears the pool (one selection-change per frame typically rebuilds
// it).
func (p *SelectionDecalPool) ReleaseAll() {
	for i := range p.decals {
		p.decals[i] = SelectionDecal{}
	}
	p.active = 0
}

// At returns a pointer to the decal at i (for inspection/test).
func (p *SelectionDecalPool) At(i int) *SelectionDecal { return &p.decals[i] }

// Drape resamples every active circle's four corner heights from the terrain
// height function (corner XZ = center ± radius). A circle on a slope thus has
// its quad corners on the surface. Zero allocations.
func (p *SelectionDecalPool) Drape(heightAt func(x, z float32) float32) {
	if heightAt == nil {
		return
	}
	for i := range p.decals {
		d := &p.decals[i]
		if !d.Active {
			continue
		}
		cx, cz, r := d.Center.X, d.Center.Y, d.Radius
		d.CornerY[0] = heightAt(cx-r, cz-r)
		d.CornerY[1] = heightAt(cx+r, cz-r)
		d.CornerY[2] = heightAt(cx+r, cz+r)
		d.CornerY[3] = heightAt(cx-r, cz+r)
	}
}
