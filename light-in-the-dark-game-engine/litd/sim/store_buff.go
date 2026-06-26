package sim

// BuffPool (ecs-architecture.md §2, §5): buffs are entities-lite —
// pooled fixed-size instances, not full entities. Typed array + LIFO
// free list; sync.Pool is banned in sim (non-deterministic, GC-emptied
// — ecs §7 pool rule). Exhaustion fails deterministically.

// BuffInstance is one live buff row. All fields fixed-size values.
type BuffInstance struct {
	BuffID         uint16
	Stacks         uint8
	Flags          uint8 // BuffInstAuraChild (#164)
	Target         EntityID
	Source         EntityID
	RemainingTicks uint32
	PeriodicClock  uint32 // absolute next-period tick (CooldownReady)
}

// BuffInstAuraChild marks an aura-maintained child: the aura system
// refreshes its linger while the carrier is in radius, and aura
// maintenance never treats it as an aura source (no chains).
const BuffInstAuraChild uint8 = 1 << 0

// BuffPool hands out rows by index. Index, not pointer, is the handle:
// rows live in one backing array that never moves (R-GC-2).
type BuffPool struct {
	rows []BuffInstance
	free []int32 // LIFO
	live []bool

	DebugAssert func(msg string)
}

func NewBuffPool(capacity int) *BuffPool {
	if capacity <= 0 {
		panic("sim: buff pool capacity must be positive")
	}
	p := &BuffPool{
		rows: make([]BuffInstance, capacity),
		free: make([]int32, capacity),
		live: make([]bool, capacity),
	}
	for i := range p.free {
		p.free[i] = int32(capacity - 1 - i) // pop order: slot 0 first
	}
	return p
}

// Alloc takes a zeroed row, returning its index. Fails closed at
// exhaustion (deterministic: same fill order ⇒ same failure tick).
func (p *BuffPool) Alloc() (int32, bool) {
	if len(p.free) == 0 {
		if p.DebugAssert != nil {
			p.DebugAssert("buff pool exhausted")
		}
		return -1, false
	}
	i := p.free[len(p.free)-1]
	p.free = p.free[:len(p.free)-1]
	p.rows[i] = BuffInstance{}
	p.live[i] = true
	return i, true
}

// Free returns a row to the pool. Double-free is a contract violation:
// assert fires, pool untouched (fail closed).
func (p *BuffPool) Free(i int32) bool {
	if i < 0 || int(i) >= len(p.rows) || !p.live[i] {
		if p.DebugAssert != nil {
			p.DebugAssert("buff pool bad free")
		}
		return false
	}
	p.live[i] = false
	p.free = append(p.free, i)
	return true
}

// Row returns the row for mutation. Index must be live.
func (p *BuffPool) Row(i int32) *BuffInstance {
	if i < 0 || int(i) >= len(p.rows) || !p.live[i] {
		panic("sim: buff pool access to dead row")
	}
	return &p.rows[i]
}

// Live returns the number of allocated rows.
func (p *BuffPool) Live() int { return len(p.rows) - len(p.free) }

// Cap returns the fixed capacity.
func (p *BuffPool) Cap() int { return len(p.rows) }
