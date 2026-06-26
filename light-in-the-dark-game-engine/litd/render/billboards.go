package render

import "github.com/g3n/engine/math32"

// Constant-orientation billboard health bars (#167; fog-of-war-minimap-
// selection.md §4.1–4.3, §7.1.2).
//
// Under the locked RTS camera (fixed pitch/yaw) every billboard shares one
// orientation, so a health bar is just a pooled quad at a world anchor above
// its unit with two per-graphic uniforms: a fill in [0,1] (current/max HP) and
// a color from a green→red ramp. No geometry is rewritten as HP changes — only
// the fill and color uniforms update. Bars show for the current selection, for
// everything under Alt, or for damaged units per setting; that policy is the
// caller's, this pool just holds and shades the bars.

// HealthBar is one pooled bar: a world anchor (above the unit), the fill
// fraction, and the ramped color. Active marks the slot in use.
type HealthBar struct {
	Anchor math32.Vector3
	Fill   float32 // clamped [0,1]
	Color  RGBA
	Active bool
}

// HealthColor maps a fill fraction to the green→red ramp: full green at 1,
// yellow at 0.5, red at 0. Out-of-range fills clamp.
func HealthColor(fill float32) RGBA {
	if fill < 0 {
		fill = 0
	} else if fill > 1 {
		fill = 1
	}
	r := 2 * (1 - fill) // 0 at full, 1 at half-or-below
	g := 2 * fill       // 1 at half-or-above, 0 at empty
	if r > 1 {
		r = 1
	}
	if g > 1 {
		g = 1
	}
	return RGBA{R: r, G: g, B: 0, A: 1}
}

// HealthBarPool is a fixed-size pool of health bars.
type HealthBarPool struct {
	bars   []HealthBar
	active int
}

// NewHealthBarPool preallocates a pool of cap bars.
func NewHealthBarPool(capacity int) *HealthBarPool {
	if capacity < 0 {
		capacity = 0
	}
	return &HealthBarPool{bars: make([]HealthBar, capacity)}
}

func (p *HealthBarPool) Cap() int            { return len(p.bars) }
func (p *HealthBarPool) ActiveCount() int    { return p.active }
func (p *HealthBarPool) At(i int) *HealthBar { return &p.bars[i] }

// Acquire claims a free slot for a bar at anchor with the given current/max HP.
// fill = current/max, clamped; color ramped. ok is false when the pool is full.
// maxHP <= 0 is treated as an empty (0 fill) bar rather than dividing by zero.
func (p *HealthBarPool) Acquire(anchor math32.Vector3, hp, maxHP float32) (int, bool) {
	fill := float32(0)
	if maxHP > 0 {
		fill = hp / maxHP
	}
	if fill < 0 {
		fill = 0
	} else if fill > 1 {
		fill = 1
	}
	for i := range p.bars {
		if !p.bars[i].Active {
			p.bars[i] = HealthBar{Anchor: anchor, Fill: fill, Color: HealthColor(fill), Active: true}
			p.active++
			return i, true
		}
	}
	return -1, false
}

// SetFill updates only the fill + color uniforms of an active bar (no geometry
// rewrite), e.g. when its unit takes damage.
func (p *HealthBarPool) SetFill(i int, hp, maxHP float32) {
	if i < 0 || i >= len(p.bars) || !p.bars[i].Active {
		return
	}
	fill := float32(0)
	if maxHP > 0 {
		fill = hp / maxHP
	}
	if fill < 0 {
		fill = 0
	} else if fill > 1 {
		fill = 1
	}
	p.bars[i].Fill = fill
	p.bars[i].Color = HealthColor(fill)
}

// Release frees the slot at i.
func (p *HealthBarPool) Release(i int) {
	if i < 0 || i >= len(p.bars) || !p.bars[i].Active {
		return
	}
	p.bars[i] = HealthBar{}
	p.active--
}

// ReleaseAll clears the pool.
func (p *HealthBarPool) ReleaseAll() {
	for i := range p.bars {
		p.bars[i] = HealthBar{}
	}
	p.active = 0
}
