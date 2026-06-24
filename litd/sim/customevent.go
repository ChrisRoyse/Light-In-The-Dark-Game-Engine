package sim

// Script-defined custom event kinds — PRD2 04-custom-events (epic #547).
// The existing event machinery (events.go: Event/Subscribe/Emit/ring/
// phase-6 dispatch) is UNCHANGED; this only mints new uint16 `kind`
// values so scripts can pub/sub their own events through the same
// deterministic path. This file is the registry foundation (#614):
// interned names → stable kind ids, idempotent registration, capacity.

// KBuiltinMax is the highest kind id reserved for built-in EventKinds.
// The built-ins currently top out at 38 (EvBuffRefreshed); the reserve to
// 63 gives headroom so adding a future built-in does NOT shift any custom
// kind id (which would break saved registries). Custom kinds are assigned
// from KBuiltinMax+1 upward.
const KBuiltinMax uint16 = 63

// CustomEventRegistry maps script-registered event names to stable custom
// kind ids. A name's kind is KBuiltinMax + its 1-based intern id, so the
// names intern table IS the registry's serialized truth (#617) — no
// separate id column. Registration is append-only within a match (ids
// never recycle), so a kind id is stable across save/load (R-EVT-6).
type CustomEventRegistry struct {
	names internTable // event name → 1-based nameId; kind = KBuiltinMax + nameId
	cap   uint16      // max custom kinds (Caps.CustomEventKinds)

	// Dropped counts RegisterEventKind calls refused at capacity — hashed
	// state (#617) so a capacity divergence fails closed.
	Dropped uint32
}

// NewCustomEventRegistry returns a registry bounded to cap custom kinds.
func NewCustomEventRegistry(cap int) *CustomEventRegistry {
	if cap < 0 || cap > int(^uint16(0)-KBuiltinMax) {
		panic("sim: custom-event capacity out of range")
	}
	return &CustomEventRegistry{names: newInternTable(), cap: uint16(cap)}
}

// Count is the number of registered custom kinds.
func (r *CustomEventRegistry) Count() uint16 { return uint16(r.names.count()) }

// Cap is the maximum number of custom kinds.
func (r *CustomEventRegistry) Cap() uint16 { return r.cap }

// RegisterEventKind interns name and returns its custom kind id. Idempotent
// (R-EVT-5): a name already registered returns its existing id. Returns 0
// (invalid kind) and increments Dropped when the registry is full and the
// name is new (R-EVT-6, fail-closed). Setup-time op; not per-tick.
func (r *CustomEventRegistry) RegisterEventKind(name string) uint16 {
	if id := r.names.id(name); id != 0 {
		return KBuiltinMax + uint16(id)
	}
	if uint16(r.names.count()) >= r.cap {
		r.Dropped++
		return 0
	}
	id := r.names.intern(name)
	return KBuiltinMax + uint16(id)
}

// KindOf returns the kind id for an already-registered name, or 0 if the
// name was never registered (read-only — does not register).
func (r *CustomEventRegistry) KindOf(name string) uint16 {
	if id := r.names.id(name); id != 0 {
		return KBuiltinMax + uint16(id)
	}
	return 0
}

// NameOf returns the registered name for a custom kind id, ok=false for a
// built-in/out-of-range/unregistered kind.
func (r *CustomEventRegistry) NameOf(kind uint16) (string, bool) {
	if kind <= KBuiltinMax {
		return "", false
	}
	return r.names.lookup(uint32(kind - KBuiltinMax))
}

// IsCustomKind reports whether kind is a registered custom kind.
func (r *CustomEventRegistry) IsCustomKind(kind uint16) bool {
	return kind > KBuiltinMax && kind <= KBuiltinMax+uint16(r.names.count())
}

// ValidEventKind reports whether kind may be emitted/subscribed: a non-
// zero built-in (1..KBuiltinMax) or a registered custom kind (#615). The
// public emit/subscribe surface (#619) uses this to fail closed on a
// custom kind nobody registered, rather than silently queueing an event
// no handler can name. The sim's built-in emitters use compile-time kind
// constants and bypass this check (the hot path stays a bare Emit).
func (w *World) ValidEventKind(kind uint16) bool {
	return kind != 0 && (kind <= KBuiltinMax || w.CustomEvents.IsCustomKind(kind))
}
