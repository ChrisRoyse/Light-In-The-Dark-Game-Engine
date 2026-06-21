package sim

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"

// Serializable handler-identity registry (ADR #451, issue #455). The
// ECA substrate stores a trigger's conditions and actions by a stable
// identity — never as a Go closure — so the whole trigger graph is
// data and survives save/load (root fix for the #433/#450 class).
//
// A HandlerRef is that identity in its hot form: a small integer that
// indexes a []TriggerHandler with no map and no allocation. The stable
// *name* is the cold form: it is what serializes, and what a fresh
// runtime re-registers to reconstruct the identical ref table.
//
// The determinism contract: refs are assigned in registration order
// starting at 1. Registration is setup-only (cold path), running in
// deterministic script/construction order, so the Nth handler
// registered always receives ref N. A reloaded match re-runs the same
// registration and reproduces the identical name->ref mapping; the
// serialized name table is then a manifest the loader checks the
// re-registered table against (fail-closed on any mismatch).

// HandlerRef is the serializable identity of one registered ECA
// condition or action. Valid refs start at 1; NoHandler (0) means "no
// handler bound".
type HandlerRef uint32

// NoHandler is the zero ref — no condition/action bound.
const NoHandler HandlerRef = 0

// maxHandlerNames bounds the registry on load: a hostile or corrupt
// save cannot drive an unbounded allocation. Far above any real
// trigger script's handler count.
const maxHandlerNames = 1 << 16

// maxHandlerNameLen bounds a single serialized handler name on load.
// Generous for any registered-name / script-site+ordinal identity.
const maxHandlerNameLen = 1024

// TriggerHandler is the single ECA handler shape. Conditions consult
// the returned bool (all conditions must return true to gate actions);
// actions ignore it and return true. One type lets conditions and
// actions share a single identity space and a single zero-alloc
// dispatch table (#457/#459).
type TriggerHandler func(w *World, e Event) bool

// handlerRegistry maps stable names <-> refs and refs -> funcs. The
// hot path indexes fns by (ref-1) with no map; byName is consulted
// only on the cold path (registration and load validation) and is
// never touched in Step.
type handlerRegistry struct {
	names  []string         // index = ref-1; the serialized identity
	fns    []TriggerHandler // index = ref-1; the hot dispatch table
	byName map[string]HandlerRef
}

func newHandlerRegistry() handlerRegistry {
	return handlerRegistry{byName: make(map[string]HandlerRef)}
}

// RegisterHandlerID binds a stable name to fn and returns its ref. It
// panics, fail-closed, on a nil func, an empty name, or a duplicate name.
//
// Registration is allowed during Step: ECA scripts legitimately create
// triggers (and thus register condition/action handlers) at runtime from
// within a firing trigger, and dispatch is deterministic, so the
// registration order — and therefore the ref assignment — is reproducible
// on replay. (Round-tripping a registry grown at runtime through
// save/load is the #464 concern, not a determinism break.)
//
// Re-registering the same names in the same order on a fresh world
// reproduces the identical ref table (the save/load invariant).
func (w *World) RegisterHandlerID(name string, fn TriggerHandler) HandlerRef {
	if fn == nil {
		panic("sim: RegisterHandlerID with nil func: " + name)
	}
	if name == "" {
		panic("sim: RegisterHandlerID with empty name")
	}
	if _, dup := w.handlerReg.byName[name]; dup {
		panic("sim: duplicate handler name: " + name)
	}
	w.handlerReg.names = append(w.handlerReg.names, name)
	w.handlerReg.fns = append(w.handlerReg.fns, fn)
	ref := HandlerRef(len(w.handlerReg.names)) // 1-based
	w.handlerReg.byName[name] = ref
	return ref
}

// ResolveHandlerRef returns the func bound to ref and true, or nil and
// false for NoHandler or an out-of-range ref. Callers fail closed on
// the false return — an unknown ref is never silently skipped. This is
// the hot-path lookup: a single slice index, no map, no allocation.
func (w *World) ResolveHandlerRef(ref HandlerRef) (TriggerHandler, bool) {
	if ref == NoHandler || int(ref) > len(w.handlerReg.fns) {
		return nil, false
	}
	return w.handlerReg.fns[ref-1], true
}

// HandlerNameOf returns the stable name bound to ref, or "" for an
// unknown ref. Cold-path / diagnostic use.
func (w *World) HandlerNameOf(ref HandlerRef) string {
	if ref == NoHandler || int(ref) > len(w.handlerReg.names) {
		return ""
	}
	return w.handlerReg.names[ref-1]
}

// HandlerRefOf returns the ref bound to name and true, or NoHandler and
// false. Cold-path lookup (map) — used at authoring/registration time,
// never in Step.
func (w *World) HandlerRefOf(name string) (HandlerRef, bool) {
	ref, ok := w.handlerReg.byName[name]
	return ref, ok
}

// HandlerCount is the number of registered handlers (= the highest ref).
func (w *World) HandlerCount() int { return len(w.handlerReg.names) }

// hashHandlerReg folds the registry's identity table into h (R-SIM-6):
// the count, then each name length-prefixed in ref order. Names are the
// serialized identity, so the hash binds the same bytes the save format
// writes — a divergent registration (different count, different name,
// different order) is caught at the state-hash level, not just at load.
func (w *World) hashHandlerReg(h *statehash.Hasher) {
	h.WriteU32(uint32(len(w.handlerReg.names)))
	for _, name := range w.handlerReg.names {
		hashString(h, name)
	}
	// #473: damage-formula override identity rides the same ECA setup-identity
	// sub-hash. It contributes NOTHING for the shipped base formula, so a
	// non-overriding world's hash is byte-identical; an override binds its
	// identity here so a divergent formula is caught in the hash, not only at
	// load (same discipline as the handler name table above).
	w.hashDamageFormula(h)
}
