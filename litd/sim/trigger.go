package sim

// First-class Trigger (ADR #451, issue #456) — the ECA substrate object
// matching the WC3 World-Editor trigger model. A Trigger holds:
//   - events:  which event kinds (optionally scoped) fire it (#458),
//   - cond:    a serializable boolexpr condition tree (#457),
//   - actions: the ordered Action handler refs run when it fires (#459),
//   - enabled: master on/off (a disabled trigger never fires; #460),
//   - on:      the Initially-On flag carried for lifecycle setup (#460).
//
// Triggers live in a generation-checked slab like Units and Regions: a
// handle is {index, generation}; destroying a trigger bumps its slot's
// generation so any outstanding handle is detectably stale rather than
// silently aliased to the next trigger (R-API-5). The slab is bounded by
// Caps.Triggers — creation past the ceiling fails closed, it never grows
// unbounded (R-GC-2). Conditions and actions are stored by ref, never as
// closures, so the whole trigger graph is data that hashes and
// serializes (#455).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"

// TriggerID is the packed trigger handle: [generation:32 | index:32].
// The zero value (NoTrigger) is never a live handle — live slots carry a
// generation >= 1.
type TriggerID uint64

// NoTrigger is the invalid/zero trigger handle.
const NoTrigger TriggerID = 0

// Per-trigger event/action count ceilings — bound a corrupt save's
// allocations on load. Generous for any real authored trigger.
const (
	maxTriggerEvents  = 4096
	maxTriggerActions = 4096
)

func makeTriggerID(index, gen uint32) TriggerID {
	return TriggerID(uint64(gen)<<32 | uint64(index))
}

// Index addresses the slab slot.
func (t TriggerID) Index() uint32 { return uint32(t) }

// Generation is the slot-reuse counter the handle was minted with.
func (t TriggerID) Generation() uint32 { return uint32(t >> 32) }

// ExprRef indexes a node in the per-world boolexpr arena (#457). The
// arena and EvalExpr land with the condition-tree issue; the Trigger
// store only stores the ref. NoExpr means "no condition" — a vacuous
// AND that always passes.
type ExprRef int32

// NoExpr is the empty-condition ref.
const NoExpr ExprRef = -1

// EventReg is one event registration on a trigger: the event Kind that
// fires it, plus an optional Scope entity (NoEntity / 0 = global, fire
// for any source). The trigger event index (#458) interprets Scope; the
// store only holds and serializes it.
type EventReg struct {
	Kind  uint16
	Scope EntityID
}

// triggerSlot is one slab row. events/actions are per-trigger slices
// grown at authoring time (cold path); the tick path never allocates
// them (R-GC-1).
type triggerSlot struct {
	gen     uint32
	alive   bool
	enabled bool
	on      bool
	cond    ExprRef
	events  []EventReg
	actions []HandlerRef
}

// TriggerStore is the bounded generation-checked trigger slab. Slots
// recycle through a free list under bumped generations.
type TriggerStore struct {
	slots []triggerSlot
	free  []uint32
	cap   int
}

// NewTriggerStore returns an empty store bounded to capCount slots.
func NewTriggerStore(capCount int) *TriggerStore {
	if capCount <= 0 {
		panic("sim: trigger store cap must be > 0")
	}
	return &TriggerStore{
		slots: make([]triggerSlot, 0, capCount),
		cap:   capCount,
	}
}

// Cap is the slab ceiling.
func (s *TriggerStore) Cap() int { return s.cap }

// Count is the number of live triggers.
func (s *TriggerStore) Count() int { return len(s.slots) - len(s.free) }

// New allocates a trigger and returns its handle. A fresh trigger is
// enabled and Initially-On with no events, no condition, no actions —
// authoring fills those in (#461). Returns NoTrigger,false when the slab
// is full (fail-closed; R-GC-2).
func (s *TriggerStore) New() (TriggerID, bool) {
	if n := len(s.free); n > 0 {
		id := s.free[n-1]
		s.free = s.free[:n-1]
		sl := &s.slots[id]
		sl.alive = true
		sl.enabled = true
		sl.on = true
		sl.cond = NoExpr
		sl.events = sl.events[:0]   // generation was bumped at destroy
		sl.actions = sl.actions[:0] // reuse backing arrays, no churn
		return makeTriggerID(id, sl.gen), true
	}
	if len(s.slots) >= s.cap {
		return NoTrigger, false
	}
	s.slots = append(s.slots, triggerSlot{gen: 1, alive: true, enabled: true, on: true, cond: NoExpr})
	return makeTriggerID(uint32(len(s.slots)-1), 1), true
}

// slot resolves a handle to its live slot, or nil if the handle is stale
// (generation mismatch), dead, or out of range — fail-closed.
func (s *TriggerStore) slot(t TriggerID) *triggerSlot {
	idx := t.Index()
	if idx >= uint32(len(s.slots)) {
		return nil
	}
	sl := &s.slots[idx]
	if !sl.alive || sl.gen != t.Generation() {
		return nil
	}
	return sl
}

// Valid reports whether t still names a live trigger.
func (s *TriggerStore) Valid(t TriggerID) bool { return s.slot(t) != nil }

// Destroy retires a trigger, bumping its slot generation so outstanding
// handles go stale, and frees the slot for reuse. False on a stale/dead
// handle.
func (s *TriggerStore) Destroy(t TriggerID) bool {
	sl := s.slot(t)
	if sl == nil {
		return false
	}
	sl.alive = false
	sl.enabled = false
	sl.on = false
	sl.cond = NoExpr
	sl.events = sl.events[:0]
	sl.actions = sl.actions[:0]
	sl.gen++
	if sl.gen == 0 {
		sl.gen = 1
	}
	s.free = append(s.free, t.Index())
	return true
}

// SetEnabled / Enabled — master on/off (lifecycle #460). No-op / false on
// a stale handle.
func (s *TriggerStore) SetEnabled(t TriggerID, v bool) bool {
	sl := s.slot(t)
	if sl == nil {
		return false
	}
	sl.enabled = v
	return true
}

// Enabled reports the master flag; false on a stale handle.
func (s *TriggerStore) Enabled(t TriggerID) bool {
	sl := s.slot(t)
	return sl != nil && sl.enabled
}

// SetInitiallyOn / InitiallyOn — the Initially-On lifecycle flag (#460).
func (s *TriggerStore) SetInitiallyOn(t TriggerID, v bool) bool {
	sl := s.slot(t)
	if sl == nil {
		return false
	}
	sl.on = v
	return true
}

// InitiallyOn reports the Initially-On flag; false on a stale handle.
func (s *TriggerStore) InitiallyOn(t TriggerID) bool {
	sl := s.slot(t)
	return sl != nil && sl.on
}

// SetCondition binds the condition expr root (#457). False on a stale handle.
func (s *TriggerStore) SetCondition(t TriggerID, e ExprRef) bool {
	sl := s.slot(t)
	if sl == nil {
		return false
	}
	sl.cond = e
	return true
}

// Condition returns the bound condition root, or NoExpr on a stale handle.
func (s *TriggerStore) Condition(t TriggerID) ExprRef {
	sl := s.slot(t)
	if sl == nil {
		return NoExpr
	}
	return sl.cond
}

// AddEvent appends an event registration to a trigger (#458 wires the
// index). False on a stale handle.
func (s *TriggerStore) AddEvent(t TriggerID, ev EventReg) bool {
	sl := s.slot(t)
	if sl == nil {
		return false
	}
	sl.events = append(sl.events, ev)
	return true
}

// Events returns the trigger's event registrations (read-only view).
func (s *TriggerStore) Events(t TriggerID) []EventReg {
	sl := s.slot(t)
	if sl == nil {
		return nil
	}
	return sl.events
}

// AddAction appends an action handler ref to a trigger (#459 runs them).
// False on a stale handle.
func (s *TriggerStore) AddAction(t TriggerID, h HandlerRef) bool {
	sl := s.slot(t)
	if sl == nil {
		return false
	}
	sl.actions = append(sl.actions, h)
	return true
}

// Actions returns the trigger's action handler refs (read-only view).
func (s *TriggerStore) Actions(t TriggerID) []HandlerRef {
	sl := s.slot(t)
	if sl == nil {
		return nil
	}
	return sl.actions
}

// HashInto folds the whole trigger slab into h in slot order, then the
// free list (R-SIM-6). Slot order and free-list order are state — slab
// reuse steers every future trigger id — so both hash, exactly like the
// entity table. Dead slots contribute their generation only.
func (s *TriggerStore) HashInto(h *statehash.Hasher) {
	h.WriteU32(uint32(len(s.slots)))
	for i := range s.slots {
		sl := &s.slots[i]
		h.WriteU32(sl.gen)
		h.WriteBool(sl.alive)
		if !sl.alive {
			continue
		}
		h.WriteBool(sl.enabled)
		h.WriteBool(sl.on)
		h.WriteI64(int64(sl.cond))
		h.WriteU32(uint32(len(sl.events)))
		for _, ev := range sl.events {
			h.WriteU16(ev.Kind)
			h.WriteU32(uint32(ev.Scope))
		}
		h.WriteU32(uint32(len(sl.actions)))
		for _, a := range sl.actions {
			h.WriteU32(uint32(a))
		}
	}
	h.WriteU32(uint32(len(s.free)))
	for _, f := range s.free {
		h.WriteU32(f)
	}
}
