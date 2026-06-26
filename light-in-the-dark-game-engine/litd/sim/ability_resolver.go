package sim

// World as AbilityResolver — PRD2 06 (epic #549, #599). The composable-ability
// compiler (#594) resolves author names to compiled ids through an
// AbilityResolver; the World is the natural one, backed by its live registries:
// effect-list names (host-registered), custom-event names (the #547 registry),
// mover-kind names (the fixed vocabulary), and KV keys (interned on demand).
// This lets a Go or Lua author register a spec by name with identical results
// (R-ABL-5) — w.RegisterAbilitySpecAuto(src) uses the world as its own resolver.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"

// moverKindNames is the fixed author vocabulary for the eight mover kinds.
// A keyed lookup (never ranged) so it stays deterministic.
var moverKindNames = map[string]MoverKind{
	"linear":      MoverLinear,
	"homing":      MoverHoming,
	"point":       MoverPoint,
	"orbit_unit":  MoverOrbitUnit,
	"orbit_point": MoverOrbitPoint,
	"arc":         MoverArc,
	"spline":      MoverSpline,
	"custom":      MoverCustom,
}

// RegisterEffectListName binds a compiled effect list to an author-facing name
// so abilities can reference it (`effects = "fireball_impact"`). Setup-only;
// the name table is rebuilt identically at load, not serialized. Returns false
// on an empty name or a list whose span falls outside the bound effect arena
// (fail-closed — an ability cannot reference an out-of-range list).
func (w *World) RegisterEffectListName(name string, list data.EffectList) bool {
	if name == "" {
		return false
	}
	if list.Len > 0 && uint32(list.Off)+uint32(list.Len) > uint32(len(w.effects)) {
		return false
	}
	if w.effectListNames == nil {
		w.effectListNames = make(map[string]data.EffectList)
	}
	w.effectListNames[name] = list
	return true
}

// EffectListByName resolves an author effect-list name. Part of AbilityResolver.
func (w *World) EffectListByName(name string) (data.EffectList, bool) {
	l, ok := w.effectListNames[name]
	return l, ok
}

// EventKindByName resolves a custom-event name to its kind. Part of
// AbilityResolver; the event must already be registered at setup (#547).
func (w *World) EventKindByName(name string) (uint16, bool) {
	if w.CustomEvents == nil {
		return 0, false
	}
	if k := w.CustomEvents.KindOf(name); k != 0 {
		return k, true
	}
	return 0, false
}

// MoverKindByName resolves a mover-kind name. Part of AbilityResolver.
func (w *World) MoverKindByName(name string) (MoverKind, bool) {
	return MoverKindFromName(name)
}

// MoverKindFromName resolves a mover-kind name without a World — for offline
// validators (tools/abilitycheck) that compile specs against the fixed
// vocabulary. The single source of truth is moverKindNames.
func MoverKindFromName(name string) (MoverKind, bool) {
	k, ok := moverKindNames[name]
	return k, ok
}

// KeyID interns (or resolves) a KV key name to its stable id. Part of
// AbilityResolver — KV keys are open, so a new key is interned.
func (w *World) KeyID(name string) uint32 {
	if w.KV == nil {
		return 0
	}
	return w.KV.InternKey(name)
}

// EffectListSpan builds an effect-list reference from an arena offset/length —
// a small constructor so callers outside the package (the api authoring
// surface) can name a compiled effect list without importing litd/data.
func EffectListSpan(off, length uint16) data.EffectList {
	return data.EffectList{Off: off, Len: length}
}

// RegisterAbilitySpecAuto compiles + registers a composable ability using the
// world itself as the resolver — the one-call authoring entry shared by the Go
// and Lua surfaces (#599). Returns the ability ref (defIndex+1) or an error.
func (w *World) RegisterAbilitySpecAuto(src data.AbilitySpecLowered) (uint16, error) {
	return w.RegisterAbilitySpec(src, w)
}
