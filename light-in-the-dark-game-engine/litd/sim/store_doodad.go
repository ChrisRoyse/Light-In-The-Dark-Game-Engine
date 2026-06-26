package sim

// DoodadStore (ecs-architecture.md §5 "Doodads — promotion on first
// touch", D-2026-06-11-13). Doodads default to render-side-only
// storage: no entity, no rows, zero sim memory — the store holds
// PROMOTED doodads only. The first script touch promotes: an EntityID
// comes from the scripted-doodad budget and a row is appended.
// Promotion is one-way for the match, so rows are append-only (no
// swap-remove) and row order IS promotion order — which is script
// execution order, deterministic, hashed, and saved (R-SIM-6).
//
// Placement lookup is a binary search over a placement-sorted index
// of the ≤1,024 promoted rows — unpromoted doodads cost nothing, not
// even a lookup slot.

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// Doodad override flag bits: which row fields override the static
// map placement (a zero override field is otherwise meaningless).
const (
	DoodadOverrideAnim uint8 = 1 << 0
	DoodadOverridePos  uint8 = 1 << 1
)

type DoodadStore struct {
	Placement []int32 // map-placement index
	Visible   []bool
	Anim      []uint16 // animation override (with DoodadOverrideAnim)
	Pos       []fixed.Vec2
	Facing    []fixed.Angle
	Overrides []uint8
	Entity    []EntityID

	byPlacement []int32 // row indices sorted by Placement value
	rowOf       []int32 // entity index -> row
	count       int32

	DebugAssert func(msg string, placement int32)
}

func NewDoodadStore(rowCap, entityCap int) *DoodadStore {
	if rowCap <= 0 || entityCap <= 0 || rowCap > entityCap {
		panic("sim: store caps must satisfy 0 < rowCap <= entityCap")
	}
	s := &DoodadStore{
		Placement:   make([]int32, rowCap),
		Visible:     make([]bool, rowCap),
		Anim:        make([]uint16, rowCap),
		Pos:         make([]fixed.Vec2, rowCap),
		Facing:      make([]fixed.Angle, rowCap),
		Overrides:   make([]uint8, rowCap),
		Entity:      make([]EntityID, rowCap),
		byPlacement: make([]int32, 0, rowCap),
		rowOf:       make([]int32, entityCap),
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// placementIdx binary-searches byPlacement; returns (insert position,
// found).
func (s *DoodadStore) placementIdx(placement int32) (int, bool) {
	lo, hi := 0, len(s.byPlacement)
	for lo < hi {
		mid := (lo + hi) / 2
		if s.Placement[s.byPlacement[mid]] < placement {
			lo = mid + 1
		} else {
			hi = mid
		}
	}
	return lo, lo < len(s.byPlacement) && s.Placement[s.byPlacement[lo]] == placement
}

// PromotedRow returns the row of an already-promoted placement, or -1.
func (s *DoodadStore) PromotedRow(placement int32) int32 {
	if i, ok := s.placementIdx(placement); ok {
		return s.byPlacement[i]
	}
	return -1
}

// Promote returns the EntityID for a placement, promoting on first
// touch. Idempotent: a second touch returns the SAME EntityID with no
// new row. The 1,025th distinct promotion fails deterministically.
func (s *DoodadStore) Promote(e *Entities, placement int32) (EntityID, bool) {
	if placement < 0 {
		s.assert("Promote with negative placement", placement)
		return 0, false
	}
	pos, found := s.placementIdx(placement)
	if found {
		return s.Entity[s.byPlacement[pos]], true
	}
	if int(s.count) == len(s.Entity) {
		s.assert("doodad pool exhausted", placement)
		return 0, false
	}
	id, ok := e.Create()
	if !ok {
		s.assert("entity table exhausted", placement)
		return 0, false
	}
	r := s.count
	s.Placement[r] = placement
	s.Visible[r] = true // placements render by default
	s.Anim[r] = 0
	s.Pos[r] = fixed.Vec2{}
	s.Facing[r] = 0
	s.Overrides[r] = 0
	s.Entity[r] = id
	s.rowOf[id.Index()] = r
	s.count++
	// sorted insert into the placement index
	s.byPlacement = s.byPlacement[:len(s.byPlacement)+1]
	copy(s.byPlacement[pos+1:], s.byPlacement[pos:])
	s.byPlacement[pos] = r
	return id, true
}

// Row returns the dense row for a promoted doodad's EntityID, or -1.
func (s *DoodadStore) Row(id EntityID) int32 {
	idx := id.Index()
	if idx >= uint32(len(s.rowOf)) {
		return -1
	}
	return s.rowOf[idx]
}

// Count returns the number of promoted doodads.
func (s *DoodadStore) Count() int32 { return s.count }

// HashInto writes every promoted row in row order (= promotion
// order): promoted rows are authoritative hashed state.
func (s *DoodadStore) HashInto(h *statehash.Hasher) {
	h.WriteU32(uint32(s.count))
	for r := int32(0); r < s.count; r++ {
		h.WriteU32(uint32(s.Placement[r]))
		h.WriteBool(s.Visible[r])
		h.WriteU16(s.Anim[r])
		h.WriteI64(int64(s.Pos[r].X))
		h.WriteI64(int64(s.Pos[r].Y))
		h.WriteU16(uint16(s.Facing[r]))
		h.WriteU8(s.Overrides[r])
		h.WriteU32(uint32(s.Entity[r]))
	}
}

func (s *DoodadStore) assert(msg string, placement int32) {
	if s.DebugAssert != nil {
		s.DebugAssert(msg, placement)
	}
}

// World-level touch helpers: every script-facing doodad verb promotes
// on first touch, then mutates the row.

// PromoteDoodad is the explicit touch (SetDoodadAnimation analogues
// route through here).
func (w *World) PromoteDoodad(placement int32) (EntityID, bool) {
	return w.Doodads.Promote(w.Ents, placement)
}

// ShowDoodad promotes (first touch) and sets visibility.
func (w *World) ShowDoodad(placement int32, visible bool) (EntityID, bool) {
	id, ok := w.Doodads.Promote(w.Ents, placement)
	if !ok {
		return 0, false
	}
	w.Doodads.Visible[w.Doodads.Row(id)] = visible
	return id, true
}

// SetDoodadAnim promotes and installs an animation override.
func (w *World) SetDoodadAnim(placement int32, anim uint16) (EntityID, bool) {
	id, ok := w.Doodads.Promote(w.Ents, placement)
	if !ok {
		return 0, false
	}
	r := w.Doodads.Row(id)
	w.Doodads.Anim[r] = anim
	w.Doodads.Overrides[r] |= DoodadOverrideAnim
	return id, true
}
