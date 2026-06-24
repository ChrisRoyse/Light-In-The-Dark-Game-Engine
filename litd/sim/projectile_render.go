package sim

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// ProjectileRender is the render-only side table for mover-driven
// projectiles (#590). When the missile→mover migration routes SpawnMissile
// onto a mover + a lightweight body entity, the body still needs to render
// as an arced billboard (not a unit model), exactly like a missile did. The
// deterministic motion lives in the MoverStore (hashed/saved); the
// PRESENTATION statics that the old MissileStore carried render-only — the
// parabola peak height (Arc), the sprite-selecting guidance kind, and the
// whole-unit flight distance (Span, the LifeFrac denominator, #528) — live
// here instead, so the MoverStore stays a pure deterministic store.
//
// This table is NEVER hashed and NEVER saved (it is rebuilt at spawn, like
// the missile's render-only Span). It is keyed by the body entity, with a
// dense row layout for zero-alloc iteration in the snapshot publish.
type ProjectileRender struct {
	Entity   []EntityID
	Arc      []fixed.F64 // presentation parabola peak height (flight itself is straight)
	Guidance []uint16    // MissileGuidance* sprite selector (carried for render parity)
	Span     []int32     // whole-unit total flight distance: the LifeFrac denominator
	Mover    []MoverID   // the mover driving this body (for live flight-progress)

	rowOf []int32
	count int32
}

// NewProjectileRender returns a render store sized for rowCap concurrent
// projectiles over an entityCap index space. rowCap mirrors the projectile
// pool (caps.Projectiles); entityCap is the world's entity index space.
func NewProjectileRender(rowCap, entityCap int) *ProjectileRender {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: ProjectileRender caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &ProjectileRender{
		Entity:   make([]EntityID, rowCap),
		Arc:      make([]fixed.F64, rowCap),
		Guidance: make([]uint16, rowCap),
		Span:     make([]int32, rowCap),
		Mover:    make([]MoverID, rowCap),
		rowOf:    make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Count is the number of live projectile render records.
func (s *ProjectileRender) Count() int32 { return s.count }

// Row returns the dense row of a body entity, or -1 if it has no record
// (i.e. it is not a mover-driven billboard projectile).
func (s *ProjectileRender) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

// Add records render statics for a projectile body. Returns false on a
// double-add or when the render pool is full (fail-closed: a projectile that
// cannot register a render record simply will not draw, never panics).
func (s *ProjectileRender) Add(id EntityID, mover MoverID, arc fixed.F64, guidance uint16, span int32) bool {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) || s.rowOf[idx] != -1 || int(s.count) == len(s.Entity) {
		return false
	}
	r := s.count
	s.Entity[r] = id
	s.Arc[r] = arc
	s.Guidance[r] = guidance
	s.Span[r] = span
	s.Mover[r] = mover
	s.rowOf[idx] = r
	s.count++
	return true
}

// Remove drops a body's render record (dense swap-with-last), called from the
// entity-death cleanup. Returns false if the entity had no record.
func (s *ProjectileRender) Remove(id EntityID) bool {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return false
	}
	r := s.rowOf[idx]
	if r == -1 {
		return false
	}
	last := s.count - 1
	if r != last {
		s.Entity[r] = s.Entity[last]
		s.Arc[r] = s.Arc[last]
		s.Guidance[r] = s.Guidance[last]
		s.Span[r] = s.Span[last]
		s.Mover[r] = s.Mover[last]
		s.rowOf[s.Entity[last].Index()] = r
	}
	s.rowOf[idx] = -1
	s.count--
	return true
}
