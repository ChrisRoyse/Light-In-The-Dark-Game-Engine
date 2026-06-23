package sim

// DestructableStore (#229, destructables-and-doodads.md, D-2026-06-11-13).
// Destructables are killable, optionally pathing-blocking widgets — trees,
// gates, breakable rocks. Unlike doodads (render-only until first touch), every
// destructable is a real sim entity from creation: it has health, can be
// killed/resurrected, and (when blocking) stamps a static pathing footprint
// that frees the moment it dies, same tick, deterministically.
//
// Rows are append-only for the match — a killed destructable keeps its row with
// the Dead flag set (resurrect flips it back). Row order is creation order:
// deterministic, hashed, and saved.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

type DestructableStore struct {
	Type      []uint16
	Pos       []fixed.Vec2
	Facing    []fixed.Angle
	Life      []int32
	MaxLife   []int32
	Dead      []bool
	Invuln    []bool
	Blocks    []bool  // stamps a static pathing footprint while alive
	Footprint []uint8 // footprint side in pathing cells (0 = no footprint)
	Entity    []EntityID

	rowOf []int32 // entity index -> row, -1 if none
	count int32
}

func NewDestructableStore(rowCap, entityCap int) *DestructableStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: destructable store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &DestructableStore{
		Type:      make([]uint16, 0, rowCap),
		Pos:       make([]fixed.Vec2, 0, rowCap),
		Facing:    make([]fixed.Angle, 0, rowCap),
		Life:      make([]int32, 0, rowCap),
		MaxLife:   make([]int32, 0, rowCap),
		Dead:      make([]bool, 0, rowCap),
		Invuln:    make([]bool, 0, rowCap),
		Blocks:    make([]bool, 0, rowCap),
		Footprint: make([]uint8, 0, rowCap),
		Entity:    make([]EntityID, 0, rowCap),
		rowOf:     make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Cap is the maximum number of destructable rows.
func (s *DestructableStore) Cap() int { return cap(s.Type) }

// Count returns the number of destructable rows (live + dead).
func (s *DestructableStore) Count() int32 { return s.count }

// Row returns the dense row for a destructable's EntityID, or -1.
func (s *DestructableStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

// add appends a row for an already-allocated EntityID. Caller guarantees
// capacity (checked by the World creation verb).
func (s *DestructableStore) add(id EntityID, typ uint16, pos fixed.Vec2, facing fixed.Angle, life, maxLife int32, blocks bool, footprint uint8) int32 {
	r := s.count
	s.Type = append(s.Type, typ)
	s.Pos = append(s.Pos, pos)
	s.Facing = append(s.Facing, facing)
	s.Life = append(s.Life, life)
	s.MaxLife = append(s.MaxLife, maxLife)
	s.Dead = append(s.Dead, false)
	s.Invuln = append(s.Invuln, false)
	s.Blocks = append(s.Blocks, blocks)
	s.Footprint = append(s.Footprint, footprint)
	s.Entity = append(s.Entity, id)
	s.rowOf[id.Index()] = r
	s.count++
	return r
}

// HashInto writes every row in creation order — destructable state is
// authoritative gameplay state (health gates combat, Dead gates pathing).
func (s *DestructableStore) HashInto(h *statehash.Hasher) {
	h.WriteU32(uint32(s.count))
	for r := int32(0); r < s.count; r++ {
		h.WriteU16(s.Type[r])
		h.WriteI64(int64(s.Pos[r].X))
		h.WriteI64(int64(s.Pos[r].Y))
		h.WriteU16(uint16(s.Facing[r]))
		h.WriteI64(int64(s.Life[r]))
		h.WriteI64(int64(s.MaxLife[r]))
		h.WriteBool(s.Dead[r])
		h.WriteBool(s.Invuln[r])
		h.WriteBool(s.Blocks[r])
		h.WriteU8(s.Footprint[r])
		h.WriteU32(uint32(s.Entity[r]))
	}
}

// --- World-level verbs ---

// destructableRect is the pathing footprint of a row, anchored on its position.
func (w *World) destructableRect(r int32) path.Rect {
	side := int32(w.Destructables.Footprint[r])
	fx, fy, fw := footprintCells(w.Destructables.Pos[r], side)
	return path.Rect{X: fx, Y: fy, W: fw, H: fw}
}

// CreateDestructable spawns a destructable and, when it blocks and has a
// footprint, stamps its static pathing cells immediately. Returns 0 on pool or
// entity-table exhaustion (fail-closed, no row).
func (w *World) CreateDestructable(typ uint16, pos fixed.Vec2, facing fixed.Angle, life int32, blocks bool, footprint uint8) EntityID {
	d := w.Destructables
	if int(d.count) >= d.Cap() {
		return 0
	}
	id, ok := w.Ents.Create()
	if !ok {
		return 0
	}
	if life < 0 {
		life = 0
	}
	r := d.add(id, typ, pos, facing, life, life, blocks, footprint)
	if blocks && footprint > 0 {
		w.stampStatic(w.destructableRect(r))
	}
	return id
}

// DestructableLife returns current life, or 0 for an unknown handle.
func (w *World) DestructableLife(id EntityID) int32 {
	r := w.Destructables.Row(id)
	if r < 0 {
		return 0
	}
	return w.Destructables.Life[r]
}

// DestructableMaxLife returns max life, or 0 for an unknown handle.
func (w *World) DestructableMaxLife(id EntityID) int32 {
	r := w.Destructables.Row(id)
	if r < 0 {
		return 0
	}
	return w.Destructables.MaxLife[r]
}

// SetDestructableLife sets current life clamped to [0, MaxLife]. Setting life
// to 0 does NOT kill (matches WC3 — death is the explicit Kill verb that frees
// pathing); a positive life on a dead row does not resurrect it.
func (w *World) SetDestructableLife(id EntityID, v int32) bool {
	r := w.Destructables.Row(id)
	if r < 0 {
		return false
	}
	if v < 0 {
		v = 0
	}
	if v > w.Destructables.MaxLife[r] {
		v = w.Destructables.MaxLife[r]
	}
	w.Destructables.Life[r] = v
	return true
}

// KillDestructable kills a live destructable: life 0, Dead set, and its static
// pathing footprint freed the same tick. No-op (false) on an unknown or
// already-dead handle.
func (w *World) KillDestructable(id EntityID) bool {
	d := w.Destructables
	r := d.Row(id)
	if r < 0 || d.Dead[r] {
		return false
	}
	d.Dead[r] = true
	d.Life[r] = 0
	if d.Blocks[r] && d.Footprint[r] > 0 {
		w.clearStatic(w.destructableRect(r))
	}
	// Presentation cue (non-hashing): render learns the destructable died so it
	// can swap/remove its merged doodad mesh and play a death burst (#72).
	w.EmitRenderEventAt(RenderDestructableDeath, id, d.Type[r], d.Pos[r])
	return true
}

// ResurrectDestructable revives a dead destructable to full life and re-stamps
// its footprint. No-op (false) on an unknown or still-living handle.
func (w *World) ResurrectDestructable(id EntityID) bool {
	d := w.Destructables
	r := d.Row(id)
	if r < 0 || !d.Dead[r] {
		return false
	}
	d.Dead[r] = false
	d.Life[r] = d.MaxLife[r]
	if d.Blocks[r] && d.Footprint[r] > 0 {
		w.stampStatic(w.destructableRect(r))
	}
	return true
}

// DestructableDead reports whether the handle names a dead destructable.
func (w *World) DestructableDead(id EntityID) bool {
	r := w.Destructables.Row(id)
	return r >= 0 && w.Destructables.Dead[r]
}

// DestructableInvulnerable reports the invulnerable flag.
func (w *World) DestructableInvulnerable(id EntityID) bool {
	r := w.Destructables.Row(id)
	return r >= 0 && w.Destructables.Invuln[r]
}

// SetDestructableInvulnerable sets the invulnerable flag.
func (w *World) SetDestructableInvulnerable(id EntityID, v bool) bool {
	r := w.Destructables.Row(id)
	if r < 0 {
		return false
	}
	w.Destructables.Invuln[r] = v
	return true
}
