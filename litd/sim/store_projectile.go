package sim

// ProjectilePool (ecs-architecture.md §2, §5; combat-and-orders.md
// §3.4): pooled fixed-size projectile rows. The payload is a damage
// packet VALUE struct rolled at launch (WC3 semantics — PRNG call
// order anchors to the FIRE event); no interface{} anywhere. The
// target is an entity OR a point: TargetEnt == 0 selects the point
// variant — an encoding, not a boxed union.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// DamagePacket is the §3.4 deferred-damage value struct.
type DamagePacket struct {
	Source     EntityID
	Target     EntityID
	Amount     fixed.F64 // rolled at launch
	AttackType uint8
	Flags      uint8
}

// Projectile flag bits.
const (
	ProjHoming uint8 = 1 << 0 // tracks the target entity (WC3 default)
	ProjAoE    uint8 = 1 << 1 // detonates at last position if target invalidates
)

// ProjectileInstance is one in-flight projectile row.
type ProjectileInstance struct {
	Source    EntityID
	TargetEnt EntityID   // 0 = point target
	TargetPt  fixed.Vec2 // point target / last known position
	Pos       fixed.Vec2
	Speed     fixed.F64 // world units per tick
	Arc       fixed.F64
	Payload   DamagePacket
	Flags     uint8
}

// ProjectilePool: typed array + LIFO free list (ecs §7 pool rule —
// never sync.Pool).
type ProjectilePool struct {
	rows []ProjectileInstance
	free []int32
	live []bool

	DebugAssert func(msg string)
}

func NewProjectilePool(capacity int) *ProjectilePool {
	if capacity <= 0 {
		panic("sim: projectile pool capacity must be positive")
	}
	p := &ProjectilePool{
		rows: make([]ProjectileInstance, capacity),
		free: make([]int32, capacity),
		live: make([]bool, capacity),
	}
	for i := range p.free {
		p.free[i] = int32(capacity - 1 - i)
	}
	return p
}

// Alloc takes a zeroed row. Fails closed at exhaustion.
func (p *ProjectilePool) Alloc() (int32, bool) {
	if len(p.free) == 0 {
		if p.DebugAssert != nil {
			p.DebugAssert("projectile pool exhausted")
		}
		return -1, false
	}
	i := p.free[len(p.free)-1]
	p.free = p.free[:len(p.free)-1]
	p.rows[i] = ProjectileInstance{}
	p.live[i] = true
	return i, true
}

// Free returns a row. Double-free asserts and is a no-op.
func (p *ProjectilePool) Free(i int32) bool {
	if i < 0 || int(i) >= len(p.rows) || !p.live[i] {
		if p.DebugAssert != nil {
			p.DebugAssert("projectile pool bad free")
		}
		return false
	}
	p.live[i] = false
	p.free = append(p.free, i)
	return true
}

// Row returns the live row for mutation.
func (p *ProjectilePool) Row(i int32) *ProjectileInstance {
	if i < 0 || int(i) >= len(p.rows) || !p.live[i] {
		panic("sim: projectile pool access to dead row")
	}
	return &p.rows[i]
}

// Live returns the number of in-flight projectiles.
func (p *ProjectilePool) Live() int { return len(p.rows) - len(p.free) }

// Cap returns the fixed capacity.
func (p *ProjectilePool) Cap() int { return len(p.rows) }
