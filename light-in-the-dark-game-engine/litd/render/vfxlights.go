package render

import (
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/math32"
)

// VFX point-light pool (materials-and-lighting.md §6.2–6.3, R-RND-4, R-GC-2).
//
// Spell VFX want dynamic point lights, but an unbounded light set wrecks both
// the forward shader's fixed light loop and the zero-steady-state-alloc rule.
// So lights live in a fixed pool of MaxVFXLights, preallocated once and
// recycled — never created at runtime. The world returns to sun+ambient at
// steady state because every VFX light carries a mandatory finite lifetime and
// is released back to the pool when it expires.
//
// When all slots are busy, a new request evicts the least-valuable active
// light by a deterministic policy: lowest priority first; ties broken by
// shortest remaining lifetime; further ties by greatest distance from screen
// centre. A request strictly lower in priority than every active light is
// denied rather than evicting a more important light.

const MaxVFXLights = 8

// VFXPriority orders eviction. Higher class wins a contested slot.
type VFXPriority uint8

const (
	VFXAmbientFlicker VFXPriority = iota // lowest — decorative flicker
	VFXStandardSpell                     // standard ability VFX
	VFXUltimate                          // highest — ultimate/hero VFX
)

// VFXRequest is a request to light a VFX. Lifetime (ticks) and Radius must be
// positive — an infinite or radius-less VFX light is refused (the validator
// rule that keeps steady state at sun+ambient).
type VFXRequest struct {
	Priority   VFXPriority
	Lifetime   int32 // ticks remaining; must be > 0
	Radius     float32
	ScreenDist float32 // distance from screen centre, tiebreak only
	Color      math32.Color
	Intensity  float32
	Pos        math32.Vector3
}

// VFXDecision records the outcome of an Acquire, for the FSV event log.
type VFXDecision struct {
	Granted bool   `json:"granted"`
	Slot    int    `json:"slot"`   // -1 when denied
	Victim  int    `json:"victim"` // evicted slot, -1 when none
	Reason  string `json:"reason"`
}

// VFXSlotInfo is a read-only snapshot of one pool slot for the dump.
type VFXSlotInfo struct {
	Active    bool        `json:"active"`
	Priority  VFXPriority `json:"priority"`
	Remaining int32       `json:"remaining"`
	Radius    float32     `json:"radius"`
	Handle    uint64      `json:"handle"`
}

type vfxSlot struct {
	active     bool
	priority   VFXPriority
	remaining  int32
	radius     float32
	screenDist float32
	handle     uint64
	light      *light.Point
}

// VFXLightPool is the fixed pool. The zero value is not usable — call
// NewVFXLightPool.
type VFXLightPool struct {
	slots     [MaxVFXLights]vfxSlot
	lowPreset bool
	nextID    uint64
}

// NewVFXLightPool preallocates MaxVFXLights point lights and (when scene is
// non-nil) adds them to the scene, hidden. No further light nodes are ever
// created. lowPreset accounts requests but binds no light (the unlit low
// preset path). scene may be nil for headless policy use.
func NewVFXLightPool(scene *core.Node, lowPreset bool) *VFXLightPool {
	p := &VFXLightPool{lowPreset: lowPreset}
	for i := range p.slots {
		if !lowPreset {
			lp := light.NewPoint(&math32.Color{R: 1, G: 1, B: 1}, 0)
			lp.SetVisible(false)
			if scene != nil {
				scene.Add(lp)
			}
			p.slots[i].light = lp
		}
	}
	return p
}

// Acquire requests a VFX light. Returns the handle (0 when denied) and the
// decision. Zero allocations: no slice growth, no node creation.
func (p *VFXLightPool) Acquire(req VFXRequest) (uint64, VFXDecision) {
	if p.lowPreset {
		return 0, VFXDecision{Granted: false, Slot: -1, Victim: -1, Reason: "denied:low-preset"}
	}
	if req.Lifetime <= 0 {
		return 0, VFXDecision{Granted: false, Slot: -1, Victim: -1, Reason: "denied:invalid-lifetime"}
	}
	if req.Radius <= 0 {
		return 0, VFXDecision{Granted: false, Slot: -1, Victim: -1, Reason: "denied:invalid-radius"}
	}
	// Free slot first.
	if i := p.freeSlot(); i >= 0 {
		p.bind(i, req)
		return p.slots[i].handle, VFXDecision{Granted: true, Slot: i, Victim: -1, Reason: "free-slot"}
	}
	// All busy — pick the least-valuable victim.
	v := p.victim()
	if req.Priority < p.slots[v].priority {
		return 0, VFXDecision{Granted: false, Slot: -1, Victim: -1, Reason: "denied:lower-priority"}
	}
	reason := p.evictReason(v, req)
	p.bind(v, req)
	return p.slots[v].handle, VFXDecision{Granted: true, Slot: v, Victim: v, Reason: reason}
}

// freeSlot returns the first inactive slot index, or -1.
func (p *VFXLightPool) freeSlot() int {
	for i := range p.slots {
		if !p.slots[i].active {
			return i
		}
	}
	return -1
}

// victim returns the least-valuable active slot: lowest priority, then shortest
// remaining lifetime, then greatest distance from screen centre. Deterministic
// (lowest index wins a full tie).
func (p *VFXLightPool) victim() int {
	best := -1
	for i := range p.slots {
		if !p.slots[i].active {
			continue
		}
		if best < 0 || p.lessValuable(i, best) {
			best = i
		}
	}
	return best
}

// lessValuable reports whether slot a should be evicted before slot b.
func (p *VFXLightPool) lessValuable(a, b int) bool {
	sa, sb := &p.slots[a], &p.slots[b]
	if sa.priority != sb.priority {
		return sa.priority < sb.priority
	}
	if sa.remaining != sb.remaining {
		return sa.remaining < sb.remaining
	}
	return sa.screenDist > sb.screenDist
}

// evictReason classifies why the victim lost the contested slot, for the log.
func (p *VFXLightPool) evictReason(v int, req VFXRequest) string {
	if req.Priority > p.slots[v].priority {
		return "evict:lower-priority"
	}
	// Equal priority: the tiebreaks decided it.
	return "evict:tie-lifetime-or-distance"
}

func (p *VFXLightPool) bind(i int, req VFXRequest) {
	p.nextID++
	s := &p.slots[i]
	s.active = true
	s.priority = req.Priority
	s.remaining = req.Lifetime
	s.radius = req.Radius
	s.screenDist = req.ScreenDist
	s.handle = p.nextID
	if s.light != nil {
		s.light.SetColor(&req.Color)
		s.light.SetIntensity(req.Intensity)
		s.light.SetPosition(req.Pos.X, req.Pos.Y, req.Pos.Z)
		// Radius drives quadratic falloff: larger radius → gentler decay.
		s.light.SetLinearDecay(0)
		s.light.SetQuadraticDecay(1.0 / (req.Radius * req.Radius))
		s.light.SetVisible(true)
	}
}

// Release frees the slot holding handle, returning it to the pool. Returns
// false if no active slot holds that handle (stale/already-expired handle).
func (p *VFXLightPool) Release(handle uint64) bool {
	for i := range p.slots {
		if p.slots[i].active && p.slots[i].handle == handle {
			p.free(i)
			return true
		}
	}
	return false
}

func (p *VFXLightPool) free(i int) {
	s := &p.slots[i]
	s.active = false
	s.handle = 0
	if s.light != nil {
		s.light.SetVisible(false)
		s.light.SetIntensity(0)
	}
}

// Tick advances one sim tick: decrement every active light's remaining
// lifetime, releasing any that reach zero. Returns the number released.
func (p *VFXLightPool) Tick() int {
	released := 0
	for i := range p.slots {
		if !p.slots[i].active {
			continue
		}
		p.slots[i].remaining--
		if p.slots[i].remaining <= 0 {
			p.free(i)
			released++
		}
	}
	return released
}

// ActiveCount returns the number of bound lights (≤ MaxVFXLights always).
func (p *VFXLightPool) ActiveCount() int {
	n := 0
	for i := range p.slots {
		if p.slots[i].active {
			n++
		}
	}
	return n
}

// SnapshotInto fills dst (must have len MaxVFXLights) with per-slot state,
// allocation-free. Returns dst resliced to MaxVFXLights.
func (p *VFXLightPool) SnapshotInto(dst []VFXSlotInfo) []VFXSlotInfo {
	dst = dst[:0]
	for i := range p.slots {
		s := &p.slots[i]
		dst = append(dst, VFXSlotInfo{
			Active: s.active, Priority: s.priority, Remaining: s.remaining,
			Radius: s.radius, Handle: s.handle,
		})
	}
	return dst
}
