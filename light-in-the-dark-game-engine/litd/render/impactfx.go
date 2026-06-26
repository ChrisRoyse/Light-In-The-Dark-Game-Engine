package render

import "github.com/g3n/engine/math32"

// Impact one-shot billboard pool (#309, R-GC-2).
//
// Missile impacts, spell bursts, and other transient hit VFX want a brief
// sprite flash at a world point that fades and vanishes on its own. Like the
// VFX light pool (vfxlights.go), these live in a fixed pool, preallocated once
// and recycled — never created at runtime — so steady state holds zero
// allocations. Every impact carries a mandatory finite lifetime and is released
// back to the pool when it expires; the render frame draws the active slots from
// the pool snapshot (data-only, no g3n nodes — the same posture as the health-bar
// billboard pool).
//
// The pool never denies a new impact (unlike the priority-gated light pool):
// impacts are ephemeral and equal, so when every slot is busy the new burst
// reuses the slot with the least remaining lifetime — the one about to vanish
// anyway — keeping the freshest bursts on screen. The choice is deterministic
// (lowest index wins a tie).

// MaxImpactFX is the impact-billboard pool capacity. Impacts are far more
// numerous than dynamic lights, so the pool is correspondingly larger.
const MaxImpactFX = 32

// ImpactRequest spawns one impact billboard. Lifetime (ticks) and Size must be
// positive — a permanent or zero-size impact is refused (an impact is by
// definition a transient, visible flash).
type ImpactRequest struct {
	Pos      math32.Vector3 // world impact point
	Size     float32        // quad half-extent (world units); must be > 0
	Color    math32.Color   // tint; the render frame fades alpha by LifeFrac
	UV       math32.Vector4 // atlas sub-rect (u0,v0,u1,v1) for the impact sprite
	Lifetime int32          // ticks until auto-release; must be > 0
}

// ImpactSlotInfo is a read-only snapshot of one pool slot for the render frame
// and the FSV dump. LifeFrac is the remaining fraction in [0,1] (1 at spawn,
// approaching 0 at expiry) — the fade weight the renderer multiplies into alpha.
type ImpactSlotInfo struct {
	Active    bool           `json:"active"`
	Pos       math32.Vector3 `json:"-"`
	Size      float32        `json:"size"`
	Color     math32.Color   `json:"-"`
	UV        math32.Vector4 `json:"-"`
	Remaining int32          `json:"remaining"`
	MaxLife   int32          `json:"maxLife"`
	LifeFrac  float32        `json:"lifeFrac"`
	Handle    uint64         `json:"handle"`
}

// ImpactDecision records the outcome of an Acquire, for the FSV event log.
type ImpactDecision struct {
	Granted bool   `json:"granted"`
	Slot    int    `json:"slot"`   // -1 when refused
	Victim  int    `json:"victim"` // reused slot, -1 when a free slot was used
	Reason  string `json:"reason"`
}

type impactSlot struct {
	active    bool
	pos       math32.Vector3
	size      float32
	color     math32.Color
	uv        math32.Vector4
	remaining int32
	maxLife   int32
	handle    uint64
}

// ImpactFXPool is the fixed pool. The zero value is not usable — call
// NewImpactFXPool.
type ImpactFXPool struct {
	slots  [MaxImpactFX]impactSlot
	nextID uint64
}

// NewImpactFXPool returns an empty impact-billboard pool. No per-impact
// allocation ever happens after this.
func NewImpactFXPool() *ImpactFXPool { return &ImpactFXPool{} }

// Acquire spawns an impact billboard. Returns the handle (0 when refused) and
// the decision. A free slot is used first; when the pool is full the slot with
// the least remaining lifetime is reused. Zero allocations.
func (p *ImpactFXPool) Acquire(req ImpactRequest) (uint64, ImpactDecision) {
	if req.Lifetime <= 0 {
		return 0, ImpactDecision{Slot: -1, Victim: -1, Reason: "refused:invalid-lifetime"}
	}
	if req.Size <= 0 {
		return 0, ImpactDecision{Slot: -1, Victim: -1, Reason: "refused:invalid-size"}
	}
	if i := p.freeSlot(); i >= 0 {
		p.bind(i, req)
		return p.slots[i].handle, ImpactDecision{Granted: true, Slot: i, Victim: -1, Reason: "free-slot"}
	}
	v := p.shortestRemaining()
	p.bind(v, req)
	return p.slots[v].handle, ImpactDecision{Granted: true, Slot: v, Victim: v, Reason: "reuse:shortest-remaining"}
}

func (p *ImpactFXPool) freeSlot() int {
	for i := range p.slots {
		if !p.slots[i].active {
			return i
		}
	}
	return -1
}

// shortestRemaining returns the active slot with the least remaining lifetime
// (lowest index wins a tie). The pool is full when this is called, so there is
// always an active slot.
func (p *ImpactFXPool) shortestRemaining() int {
	best := -1
	for i := range p.slots {
		if !p.slots[i].active {
			continue
		}
		if best < 0 || p.slots[i].remaining < p.slots[best].remaining {
			best = i
		}
	}
	return best
}

func (p *ImpactFXPool) bind(i int, req ImpactRequest) {
	p.nextID++
	s := &p.slots[i]
	s.active = true
	s.pos = req.Pos
	s.size = req.Size
	s.color = req.Color
	s.uv = req.UV
	s.remaining = req.Lifetime
	s.maxLife = req.Lifetime
	s.handle = p.nextID
}

// Release frees the slot holding handle. Returns false if no active slot holds
// it (stale/already-expired handle).
func (p *ImpactFXPool) Release(handle uint64) bool {
	for i := range p.slots {
		if p.slots[i].active && p.slots[i].handle == handle {
			p.slots[i].active = false
			p.slots[i].handle = 0
			return true
		}
	}
	return false
}

// Tick advances one sim tick: decrement every active impact's remaining
// lifetime, releasing any that reach zero. Returns the number released.
func (p *ImpactFXPool) Tick() int {
	released := 0
	for i := range p.slots {
		if !p.slots[i].active {
			continue
		}
		p.slots[i].remaining--
		if p.slots[i].remaining <= 0 {
			p.slots[i].active = false
			p.slots[i].handle = 0
			released++
		}
	}
	return released
}

// ActiveCount returns the number of live impacts (≤ MaxImpactFX always).
func (p *ImpactFXPool) ActiveCount() int {
	n := 0
	for i := range p.slots {
		if p.slots[i].active {
			n++
		}
	}
	return n
}

// SnapshotInto fills dst with per-slot state, allocation-free. Returns dst
// resliced to MaxImpactFX. LifeFrac is computed here (remaining/maxLife).
func (p *ImpactFXPool) SnapshotInto(dst []ImpactSlotInfo) []ImpactSlotInfo {
	dst = dst[:0]
	for i := range p.slots {
		s := &p.slots[i]
		var frac float32
		if s.active && s.maxLife > 0 {
			frac = float32(s.remaining) / float32(s.maxLife)
		}
		dst = append(dst, ImpactSlotInfo{
			Active: s.active, Pos: s.pos, Size: s.size, Color: s.color, UV: s.uv,
			Remaining: s.remaining, MaxLife: s.maxLife, LifeFrac: frac, Handle: s.handle,
		})
	}
	return dst
}
