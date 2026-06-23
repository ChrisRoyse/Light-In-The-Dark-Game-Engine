package render

import "github.com/g3n/engine/math32"

// Buff aura attachment pool (#309, R-GC-2).
//
// A buff/aura wants a sprite that rides a unit — following its position every
// frame, lasting either for the buff's duration (temporary) or until the buff
// is removed (permanent). Like the impact and light pools these are fixed,
// preallocated, recycled billboards (zero steady-state alloc), data-only (the
// render frame draws the active slots). The difference is attachment: a slot is
// keyed to a unit's render entity id and resolves its world position from that
// unit each frame via Update, so the sprite follows the unit. When the unit
// despawns the driver calls ReleaseByUnit so no aura is orphaned.

// MaxBuffAuras is the aura pool capacity. Auras are common (many units, several
// buffs each), so the pool is the largest of the VFX pools.
const MaxBuffAuras = 64

// BuffAuraRequest attaches an aura to a unit. Lifetime > 0 is a temporary aura
// that auto-releases after that many ticks; Lifetime == 0 is a permanent aura
// that lives until Release / ReleaseByUnit (e.g. a toggle/passive buff). Size
// must be positive. A negative Lifetime is refused.
type BuffAuraRequest struct {
	UnitKey  uint32         // render entity id the aura follows
	Offset   math32.Vector3 // local offset added to the unit's anchor
	Size     float32        // quad half-extent; must be > 0
	Color    math32.Color   // tint
	UV       math32.Vector4 // atlas sub-rect (u0,v0,u1,v1)
	Lifetime int32          // ticks; 0 = permanent, > 0 = temporary, < 0 refused
}

// BuffAuraSlotInfo is a read-only snapshot of one slot for the render frame and
// the FSV dump. Pos is resolved by the last Update (unit position + offset);
// Visible is false when that unit was missing at the last Update (culled/gone),
// so the renderer can skip it without the slot being released.
type BuffAuraSlotInfo struct {
	Active    bool           `json:"active"`
	UnitKey   uint32         `json:"unitKey"`
	Pos       math32.Vector3 `json:"-"`
	Visible   bool           `json:"visible"`
	Size      float32        `json:"size"`
	Color     math32.Color   `json:"-"`
	UV        math32.Vector4 `json:"-"`
	Permanent bool           `json:"permanent"`
	Remaining int32          `json:"remaining"` // -1 for a permanent aura
	Handle    uint64         `json:"handle"`
}

// BuffAuraDecision records the outcome of an Acquire, for the FSV event log.
type BuffAuraDecision struct {
	Granted bool   `json:"granted"`
	Slot    int    `json:"slot"`
	Reason  string `json:"reason"`
}

type buffAuraSlot struct {
	active    bool
	unitKey   uint32
	offset    math32.Vector3
	pos       math32.Vector3
	visible   bool
	size      float32
	color     math32.Color
	uv        math32.Vector4
	permanent bool
	remaining int32
	handle    uint64
}

// BuffAuraPool is the fixed pool. The zero value is not usable — call
// NewBuffAuraPool.
type BuffAuraPool struct {
	slots  [MaxBuffAuras]buffAuraSlot
	nextID uint64
}

// NewBuffAuraPool returns an empty aura pool. No per-aura allocation happens
// after this.
func NewBuffAuraPool() *BuffAuraPool { return &BuffAuraPool{} }

// Acquire attaches an aura. Returns the handle (0 when refused) and the
// decision. Refused (fail-closed) on a negative lifetime, a non-positive size,
// or a full pool — an aura is cosmetic, so a full pool drops the newest rather
// than evicting an existing buff's sprite. Zero allocations.
func (p *BuffAuraPool) Acquire(req BuffAuraRequest) (uint64, BuffAuraDecision) {
	if req.Lifetime < 0 {
		return 0, BuffAuraDecision{Slot: -1, Reason: "refused:negative-lifetime"}
	}
	if req.Size <= 0 {
		return 0, BuffAuraDecision{Slot: -1, Reason: "refused:invalid-size"}
	}
	i := p.freeSlot()
	if i < 0 {
		return 0, BuffAuraDecision{Slot: -1, Reason: "refused:pool-full"}
	}
	p.nextID++
	s := &p.slots[i]
	s.active = true
	s.unitKey = req.UnitKey
	s.offset = req.Offset
	s.pos = req.Offset // until the first Update resolves the unit position
	s.visible = false  // not drawn until an Update places it on a live unit
	s.size = req.Size
	s.color = req.Color
	s.uv = req.UV
	s.permanent = req.Lifetime == 0
	if s.permanent {
		s.remaining = -1
	} else {
		s.remaining = req.Lifetime
	}
	s.handle = p.nextID
	return s.handle, BuffAuraDecision{Granted: true, Slot: i, Reason: "free-slot"}
}

func (p *BuffAuraPool) freeSlot() int {
	for i := range p.slots {
		if !p.slots[i].active {
			return i
		}
	}
	return -1
}

// Release frees the slot holding handle (e.g. the buff was removed). Returns
// false if no active slot holds it.
func (p *BuffAuraPool) Release(handle uint64) bool {
	for i := range p.slots {
		if p.slots[i].active && p.slots[i].handle == handle {
			p.slots[i].active = false
			p.slots[i].handle = 0
			return true
		}
	}
	return false
}

// ReleaseByUnit frees every aura attached to unitKey (the unit despawned).
// Returns the number released — keeping a dead unit's auras from orphaning.
func (p *BuffAuraPool) ReleaseByUnit(unitKey uint32) int {
	n := 0
	for i := range p.slots {
		if p.slots[i].active && p.slots[i].unitKey == unitKey {
			p.slots[i].active = false
			p.slots[i].handle = 0
			n++
		}
	}
	return n
}

// Tick advances one sim tick: decrement every temporary aura, releasing any that
// reach zero. Permanent auras (remaining < 0) are untouched. Returns the number
// released.
func (p *BuffAuraPool) Tick() int {
	released := 0
	for i := range p.slots {
		s := &p.slots[i]
		if !s.active || s.permanent {
			continue
		}
		s.remaining--
		if s.remaining <= 0 {
			s.active = false
			s.handle = 0
			released++
		}
	}
	return released
}

// Update resolves each active aura's world position from its unit via lookup
// (unit anchor + offset). An aura whose unit lookup misses is marked not visible
// (the unit is culled or gone) but is NOT released — release is explicit on buff
// removal / unit death. Allocation-free.
func (p *BuffAuraPool) Update(lookup func(unitKey uint32) (math32.Vector3, bool)) {
	for i := range p.slots {
		s := &p.slots[i]
		if !s.active {
			continue
		}
		anchor, ok := lookup(s.unitKey)
		if !ok {
			s.visible = false
			continue
		}
		s.pos = math32.Vector3{X: anchor.X + s.offset.X, Y: anchor.Y + s.offset.Y, Z: anchor.Z + s.offset.Z}
		s.visible = true
	}
}

// ActiveCount returns the number of attached auras (≤ MaxBuffAuras always).
func (p *BuffAuraPool) ActiveCount() int {
	n := 0
	for i := range p.slots {
		if p.slots[i].active {
			n++
		}
	}
	return n
}

// SnapshotInto fills dst with per-slot state, allocation-free. Returns dst
// resliced to MaxBuffAuras.
func (p *BuffAuraPool) SnapshotInto(dst []BuffAuraSlotInfo) []BuffAuraSlotInfo {
	dst = dst[:0]
	for i := range p.slots {
		s := &p.slots[i]
		dst = append(dst, BuffAuraSlotInfo{
			Active: s.active, UnitKey: s.unitKey, Pos: s.pos, Visible: s.visible,
			Size: s.size, Color: s.color, UV: s.uv, Permanent: s.permanent,
			Remaining: s.remaining, Handle: s.handle,
		})
	}
	return dst
}
