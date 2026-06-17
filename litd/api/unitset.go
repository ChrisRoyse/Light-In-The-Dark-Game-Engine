package litd

// UnitSet (#239; groups-and-enumeration.md) — the persistent counterpart
// to the transient slice queries. JASS `group` handles that scripts keep
// around (add/remove members across ticks) become a UnitSet: an
// insertion-ordered set of units, never a Go map (map iteration order is
// nondeterministic — forbidden in gameplay code, R-SIM-2). Membership is a
// linear scan over the insertion-ordered backing slice; UnitSets are
// script-scale, so the scan is cheap and the order is a stable contract.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// UnitSet is a mutable, insertion-ordered set of units. The zero value is
// not usable — make one with Game.NewUnitSet.
type UnitSet struct {
	g   *Game
	ids []sim.EntityID
}

// NewUnitSet returns an empty UnitSet. JASS: CreateGroup (for the
// persistent-group use; transient enumeration uses the slice queries).
// JASS: CreateGroup
func (g *Game) NewUnitSet() *UnitSet {
	if g == nil || g.w == nil {
		return nil
	}
	return &UnitSet{g: g}
}

// Valid reports whether the set is usable (made by Game.NewUnitSet, not the
// nil/zero value). Every noun handle exposes Valid() bool (R-API-5).
func (s *UnitSet) Valid() bool { return s != nil && s.g != nil && s.g.w != nil }

// indexOf returns the position of u in the set, or -1.
func (s *UnitSet) indexOf(id sim.EntityID) int {
	for i, e := range s.ids {
		if e == id {
			return i
		}
	}
	return -1
}

// Add inserts u if absent and from this game; duplicates are ignored, so
// insertion order is preserved. No-op on a nil set or foreign/zero unit.
// JASS: GroupAddUnit, GroupAddUnitSimple
func (s *UnitSet) Add(u Unit) {
	if s == nil || u.g != s.g || u.id == 0 {
		return
	}
	if s.indexOf(u.id) == -1 {
		s.ids = append(s.ids, u.id)
	}
}

// Remove drops u, preserving the order of the rest. No-op if absent.
// JASS: GroupRemoveUnit, GroupRemoveUnitSimple
func (s *UnitSet) Remove(u Unit) {
	if s == nil {
		return
	}
	if i := s.indexOf(u.id); i != -1 {
		s.ids = append(s.ids[:i], s.ids[i+1:]...)
	}
}

// Clear empties the set, keeping the backing capacity. JASS: GroupClear.
// JASS: GroupClear
func (s *UnitSet) Clear() {
	if s != nil {
		s.ids = s.ids[:0]
	}
}

// Contains reports membership. JASS: IsUnitInGroup.
// JASS: IsUnitInGroup
func (s *UnitSet) Contains(u Unit) bool {
	return s != nil && s.indexOf(u.id) != -1
}

// Count returns the number of members (including any whose unit has since
// died — call Compact to drop the dead). JASS: CountUnitsInGroup /
// BlzGroupGetSize.
// JASS: BlzGroupGetSize, CountUnitsInGroup, CountUnitsInGroupEnum
func (s *UnitSet) Count() int {
	if s == nil {
		return 0
	}
	return len(s.ids)
}

// Compact removes members whose unit is no longer alive, preserving the
// insertion order of the survivors. The deterministic replacement for
// GroupRefresh / the "FirstOfGroup pop until empty" dead-unit purge.
func (s *UnitSet) Compact() {
	if s == nil {
		return
	}
	keep := s.ids[:0]
	for _, id := range s.ids {
		if s.g.w.Ents.Alive(id) {
			keep = append(keep, id)
		}
	}
	s.ids = keep
}

// Units returns the members as a slice in insertion order. The slice is a
// fresh copy — mutating the set afterward does not change it. JASS:
// ForGroup enumeration collapsed to a slice (R-EXEC-4).
// JASS: FirstOfGroup, ForGroup, ForGroupBJ
func (s *UnitSet) Units() []Unit {
	if s == nil {
		return make([]Unit, 0)
	}
	out := make([]Unit, len(s.ids))
	for i, id := range s.ids {
		out[i] = Unit{id: id, g: s.g}
	}
	return out
}

// AppendUnits appends the members (insertion order) to dst — the
// zero-alloc enumeration twin. JASS: ForGroup over a pooled buffer.
func (s *UnitSet) AppendUnits(dst []Unit) []Unit {
	if s == nil {
		return dst
	}
	for _, id := range s.ids {
		dst = append(dst, Unit{id: id, g: s.g})
	}
	return dst
}
