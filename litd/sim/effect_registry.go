package sim

// Per-world runtime effect-primitive registry (#477). The process-global
// registry in effect.go (RegisterEffectExec / FreezeEffectExecs) owns the
// engine's built-in compiled primitives — frozen per sim version. This file
// adds a SECOND, per-world registry a modded map fills DURING SETUP: a script
// registers a named effect/Action (e.g. "lifesteal"), gets a deterministic id,
// and the engine freezes the set at the first Step so row order can never vary
// by callback timing.
//
// Only the NAMES (in registration order) hash + serialize — that is the
// per-match contract two players must agree on. The executable closure cannot
// be serialized; on load the setup re-runs and re-registers the same set, then
// LoadState validates the saved names match the re-bound names (fail-closed,
// like the #455 handler-registry re-bind).

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"

// RuntimeEffectExec is the behavior of a registered effect primitive. It runs
// inside a sim phase (a trigger Action), takes all randomness from the sim
// PRNG, and routes ordering-sensitive writes through deferred buffers — the
// same determinism contract as the built-in EffectExec, minus the compiled
// param arena (a runtime effect closes over its own configuration).
type RuntimeEffectExec func(w *World, ctx EffectCtx)

// RegisterEffect installs a named runtime effect and returns its stable id (an
// index into the per-world registry). Fail-closed: registration during a tick
// phase (so order cannot vary by callback timing), an empty or duplicate name,
// a nil exec, or a full registry are all refused with ok=false.
func (w *World) RegisterEffect(name string, fn RuntimeEffectExec) (uint16, bool) {
	// Setup phase only: not inside a tick, and not after the match has started
	// ticking (tick == 0 ⇒ no Step has run). Frozen at the first Step so the id
	// order is fixed for the match. On load the setup re-registers at tick 0
	// before LoadState restores the saved tick.
	if w.inStep || w.tick > 0 || name == "" || fn == nil ||
		len(w.effectRegNames) >= cap(w.effectRegNames) ||
		w.effectIDByName(name) >= 0 {
		return 0, false
	}
	id := uint16(len(w.effectRegNames))
	w.effectRegNames = append(w.effectRegNames, name)
	w.effectRegExecs = append(w.effectRegExecs, fn)
	return id, true
}

// RegisteredEffectID resolves a registered effect name to its id. Setup/trigger
// wiring uses it; gameplay carries the resolved id, never the name.
func (w *World) RegisteredEffectID(name string) (uint16, bool) {
	if i := w.effectIDByName(name); i >= 0 {
		return uint16(i), true
	}
	return 0, false
}

// RegisteredEffectName returns the name bound to an id (for dumps/diagnostics).
func (w *World) RegisteredEffectName(id uint16) (string, bool) {
	if int(id) < len(w.effectRegNames) {
		return w.effectRegNames[id], true
	}
	return "", false
}

// RegisteredEffectCount is the number of registered runtime effects.
func (w *World) RegisteredEffectCount() int { return len(w.effectRegNames) }

// RunRegisteredEffect executes a registered effect by id against ctx. Returns
// false (a no-op) for an unknown id. This is the invocation seam a trigger
// Action calls when a hit/cast fires the modded primitive.
func (w *World) RunRegisteredEffect(id uint16, ctx EffectCtx) bool {
	if int(id) >= len(w.effectRegExecs) {
		return false
	}
	w.effectRegExecs[id](w, ctx)
	return true
}

// effectIDByName is the linear name scan (small N, setup-only). Returns -1 when
// absent. No map: keeps registration order the single source of truth and avoids
// any map iteration in the registry path (R-SIM-2).
func (w *World) effectIDByName(name string) int {
	for i := range w.effectRegNames {
		if w.effectRegNames[i] == name {
			return i
		}
	}
	return -1
}

// EffectCtxFor builds an invocation context for a registered effect from a
// source/target pair — a convenience for trigger-Action callers that have the
// two entities of a hit. Point/Depth default to zero.
func EffectCtxFor(source, target EntityID) EffectCtx {
	return EffectCtx{Source: source, Target: target}
}

// HealUnit adds life to a unit, clamped to its max — the primitive a "lifesteal"
// runtime effect is built on. Exported so a Go-defined RuntimeEffectExec (and
// the FSV) can heal deterministically without reaching into the Healths store.
// Returns the life actually restored.
func (w *World) HealUnit(id EntityID, amount fixed.F64) fixed.F64 {
	if amount <= 0 {
		return 0
	}
	hr := w.Healths.Row(id)
	if hr == -1 {
		return 0
	}
	before := w.Healths.Life[hr]
	max := w.Healths.MaxLife[hr]
	life := before.Add(amount)
	if life > max {
		life = max
	}
	w.Healths.Life[hr] = life
	return life.Sub(before)
}
