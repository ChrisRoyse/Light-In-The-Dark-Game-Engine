package sim

// Live weapon field overrides (#476): a sparse per-instance overlay over the
// data-default weapon values held in the Combats store. A trigger Action re-arms
// a unit at runtime (attack type, dice/sides/base, range, cooldown, delivery)
// without disturbing the unit-type default — clearing an override reverts to
// that default. Rows are pooled and indexed by entity-slot-field for O(1),
// map-free, zero-alloc resolve on the attack hot path (mirrors the #353 ability
// field store). Empty by default, so a world that never overrides a weapon
// contributes nothing to the hash/save.

// WeaponField names one overridable per-instance weapon field. Append-only —
// the numeric ids persist in the hash and save.
type WeaponField uint8

const (
	WeaponFieldAttackType WeaponField = iota
	WeaponFieldDamageBase
	WeaponFieldDice
	WeaponFieldSides
	WeaponFieldCooldown
	WeaponFieldRange       // fixed.F64 bits
	WeaponFieldDamagePoint // ticks
	WeaponFieldBackswing   // ticks
	WeaponFieldProjSpeed   // fixed.F64 bits

	// WeaponFieldCount is the number of known field ids.
	WeaponFieldCount
)

// WeaponOverrideCapPerUnit bounds override rows per unit (every field of every
// slot may be overridden at once).
const WeaponOverrideCapPerUnit = WeaponSlots * int(WeaponFieldCount)

// WeaponFieldStore owns sparse override rows. Values are stored as int64 (raw
// field bits — type ids, tick counts, or fixed.F64 bits per field). rowOf is
// keyed by entity index, slot, and field; each row carries the full EntityID so
// a stale generation cannot alias a recycled slot.
type WeaponFieldStore struct {
	Ent   []EntityID
	Slot  []uint8
	Field []uint8
	Value []int64

	live    []bool
	free    []int32
	rowOf   []int32
	perUnit []uint8

	count    int32
	rejected uint64
}

func NewWeaponFieldStore(rowCap, entityCap int) *WeaponFieldStore {
	if rowCap <= 0 || entityCap <= 0 {
		panic("sim: weapon field caps must be positive")
	}
	s := &WeaponFieldStore{
		Ent:     make([]EntityID, rowCap),
		Slot:    make([]uint8, rowCap),
		Field:   make([]uint8, rowCap),
		Value:   make([]int64, rowCap),
		live:    make([]bool, rowCap),
		free:    make([]int32, rowCap),
		rowOf:   make([]int32, entityCap*WeaponSlots*int(WeaponFieldCount)),
		perUnit: make([]uint8, entityCap),
	}
	for i := range s.free {
		s.free[i] = int32(rowCap - 1 - i) // pop order: row 0 first
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	return s
}

// Set writes an override row, updating in place when the same entity-slot-field
// already exists. New rows fail closed when the per-unit cap or pool is empty.
func (s *WeaponFieldStore) Set(e *Entities, id EntityID, slot int, field WeaponField, value int64) bool {
	if e == nil || !e.Alive(id) || !validWeaponSlot(slot) || !validWeaponField(field) {
		return false
	}
	idx := id.Index()
	if idx >= uint32(len(s.perUnit)) {
		return false
	}
	k := s.key(idx, slot, field)
	if r := s.rowOf[k]; r != -1 {
		if s.live[r] && s.Ent[r] == id {
			s.Value[r] = value
			return true
		}
		return false
	}
	if int(s.perUnit[idx]) >= WeaponOverrideCapPerUnit || len(s.free) == 0 {
		s.rejected++
		return false
	}
	r := s.free[len(s.free)-1]
	s.free = s.free[:len(s.free)-1]
	s.Ent[r] = id
	s.Slot[r] = uint8(slot)
	s.Field[r] = uint8(field)
	s.Value[r] = value
	s.live[r] = true
	s.rowOf[k] = r
	s.perUnit[idx]++
	s.count++
	return true
}

// Get returns the override value for entity-slot-field, ok=false if none.
func (s *WeaponFieldStore) Get(id EntityID, slot int, field WeaponField) (int64, bool) {
	if !validWeaponSlot(slot) || !validWeaponField(field) {
		return 0, false
	}
	idx := id.Index()
	if idx >= uint32(len(s.perUnit)) {
		return 0, false
	}
	r := s.rowOf[s.key(idx, slot, field)]
	if r == -1 || !s.live[r] || s.Ent[r] != id {
		return 0, false
	}
	return s.Value[r], true
}

// Remove drops a single override (reverts that field to the data default).
func (s *WeaponFieldStore) Remove(id EntityID, slot int, field WeaponField) bool {
	if !validWeaponSlot(slot) || !validWeaponField(field) {
		return false
	}
	idx := id.Index()
	if idx >= uint32(len(s.perUnit)) {
		return false
	}
	r := s.rowOf[s.key(idx, slot, field)]
	if r == -1 || !s.live[r] || s.Ent[r] != id {
		return false
	}
	s.freeRow(r)
	return true
}

// RemoveSlot drops every override on one weapon slot.
func (s *WeaponFieldStore) RemoveSlot(id EntityID, slot int) int {
	if !validWeaponSlot(slot) {
		return 0
	}
	removed := 0
	for f := WeaponField(0); f < WeaponFieldCount; f++ {
		if s.Remove(id, slot, f) {
			removed++
		}
	}
	return removed
}

// RemoveEntity drops all overrides on a (typically dying) unit.
func (s *WeaponFieldStore) RemoveEntity(id EntityID) int {
	idx := id.Index()
	if idx >= uint32(len(s.perUnit)) || s.perUnit[idx] == 0 {
		return 0
	}
	removed := 0
	for slot := 0; slot < WeaponSlots; slot++ {
		removed += s.RemoveSlot(id, slot)
	}
	return removed
}

// reset clears every row (used at load before re-applying saved overrides).
func (s *WeaponFieldStore) reset() {
	for i := range s.live {
		s.live[i] = false
		s.Ent[i] = 0
		s.Slot[i] = 0
		s.Field[i] = 0
		s.Value[i] = 0
	}
	for i := range s.rowOf {
		s.rowOf[i] = -1
	}
	for i := range s.perUnit {
		s.perUnit[i] = 0
	}
	s.free = s.free[:cap(s.free)]
	for i := range s.free {
		s.free[i] = int32(len(s.live) - 1 - i)
	}
	s.count = 0
}

func (s *WeaponFieldStore) Count() int32     { return s.count }
func (s *WeaponFieldStore) Cap() int         { return len(s.Ent) }
func (s *WeaponFieldStore) Rejected() uint64 { return s.rejected }

func validWeaponSlot(slot int) bool       { return slot >= 0 && slot < WeaponSlots }
func validWeaponField(f WeaponField) bool { return f < WeaponFieldCount }

func (s *WeaponFieldStore) key(idx uint32, slot int, field WeaponField) int {
	return (int(idx)*WeaponSlots+slot)*int(WeaponFieldCount) + int(field)
}

func (s *WeaponFieldStore) freeRow(r int32) {
	idx := s.Ent[r].Index()
	s.rowOf[s.key(idx, int(s.Slot[r]), WeaponField(s.Field[r]))] = -1
	if idx < uint32(len(s.perUnit)) && s.perUnit[idx] > 0 {
		s.perUnit[idx]--
	}
	s.Ent[r] = 0
	s.Slot[r] = 0
	s.Field[r] = 0
	s.Value[r] = 0
	s.live[r] = false
	s.free = append(s.free, r)
	s.count--
}

// SetUnitWeaponField installs a live override on one weapon field of a unit
// (#476). Fail-closed: an invalid slot/field, a dead unit, a unit with no
// weapon in that slot, or an out-of-range value is rejected (returns false) and
// nothing changes. The override hashes + serializes, so a re-armed unit's next
// attack reproduces across save/load.
func (w *World) SetUnitWeaponField(id EntityID, slot int, field WeaponField, value int64) bool {
	cr := w.Combats.Row(id)
	if cr == -1 || !validWeaponSlot(slot) || !validWeaponField(field) {
		return false
	}
	if !weaponUsed(w.Combats, cr, slot) {
		return false // no weapon in that slot to re-arm
	}
	if !w.validWeaponValue(field, value) {
		return false
	}
	return w.WeaponOverrides.Set(w.Ents, id, slot, field, value)
}

// ClearUnitWeaponField drops one override, reverting that field to the unit's
// data default. Returns false if there was no override.
func (w *World) ClearUnitWeaponField(id EntityID, slot int, field WeaponField) bool {
	return w.WeaponOverrides.Remove(id, slot, field)
}

// ClearUnitWeapon drops every override on one weapon slot (full revert).
func (w *World) ClearUnitWeapon(id EntityID, slot int) int {
	return w.WeaponOverrides.RemoveSlot(id, slot)
}

// GetUnitWeaponField returns the effective value of a weapon field (override if
// present, else the data default), with ok=false for an invalid target.
func (w *World) GetUnitWeaponField(id EntityID, slot int, field WeaponField) (int64, bool) {
	cr := w.Combats.Row(id)
	if cr == -1 || !validWeaponSlot(slot) || !validWeaponField(field) {
		return 0, false
	}
	return w.weaponOverride(cr, slot, field, w.weaponBaseField(cr, slot, field)), true
}

// weaponBaseField reads the data-default value of a weapon field from Combats.
func (w *World) weaponBaseField(cr int32, slot int, field WeaponField) int64 {
	c := w.Combats
	switch field {
	case WeaponFieldAttackType:
		return int64(c.AttackType[cr][slot])
	case WeaponFieldDamageBase:
		return int64(c.DmgBase[cr][slot])
	case WeaponFieldDice:
		return int64(c.DmgDice[cr][slot])
	case WeaponFieldSides:
		return int64(c.DmgSides[cr][slot])
	case WeaponFieldCooldown:
		return int64(c.Cooldown[cr][slot])
	case WeaponFieldRange:
		return int64(c.Range[cr][slot])
	case WeaponFieldDamagePoint:
		return int64(c.DamagePt[cr][slot])
	case WeaponFieldBackswing:
		return int64(c.Backswing[cr][slot])
	case WeaponFieldProjSpeed:
		return int64(c.ProjSpeed[cr][slot])
	default:
		return 0
	}
}

// validWeaponValue range-checks a field value (fail-closed). An attack-type id
// must be a declared/bound matrix row; counts fit their storage widths; cooldown
// must stay positive (a zero cooldown disables the slot).
func (w *World) validWeaponValue(field WeaponField, v int64) bool {
	switch field {
	case WeaponFieldAttackType:
		// must index the bound matrix (or, if types are declared, the table)
		if v < 0 {
			return false
		}
		if len(w.coeff) > 0 {
			return int(v) < len(w.coeff)
		}
		return v <= 255
	case WeaponFieldDamageBase:
		return v >= 0 && v <= 1<<31-1
	case WeaponFieldDice, WeaponFieldSides:
		return v >= 0 && v <= 255
	case WeaponFieldCooldown:
		return v > 0 && v <= 1<<16-1
	case WeaponFieldDamagePoint, WeaponFieldBackswing:
		return v >= 0 && v <= 1<<16-1
	case WeaponFieldRange, WeaponFieldProjSpeed:
		return v >= 0
	default:
		return false
	}
}

// hashWeaponOverrides folds the live override rows into a hasher in canonical
// (entity, slot, field) order — the rowOf key order, which is independent of
// internal pool layout, so two stores with the same logical overrides hash
// identically regardless of set/clear history (the row index never surfaces in
// gameplay; resolve is by key). Contributes NOTHING when empty, so a world that
// never re-arms a weapon keeps a byte-identical hash.
func (w *World) hashWeaponOverrides(h interface {
	WriteU32(uint32)
	WriteU8(uint8)
	WriteI64(int64)
}) {
	s := w.WeaponOverrides
	if s.count == 0 {
		return
	}
	h.WriteU32(uint32(s.count))
	for k := 0; k < len(s.rowOf); k++ {
		r := s.rowOf[k]
		if r == -1 || !s.live[r] {
			continue
		}
		h.WriteU32(uint32(s.Ent[r]))
		h.WriteU8(s.Slot[r])
		h.WriteU8(s.Field[r])
		h.WriteI64(s.Value[r])
	}
}

// weaponOverride returns the live override for a combat row's weapon field, or
// base when none — the hot-path resolve (O(1), zero-alloc).
func (w *World) weaponOverride(cr int32, slot int, field WeaponField, base int64) int64 {
	if w.WeaponOverrides.count == 0 {
		return base // fast path: no overrides anywhere
	}
	if v, ok := w.WeaponOverrides.Get(w.Combats.Entity[cr], slot, field); ok {
		return v
	}
	return base
}
