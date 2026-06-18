package litd

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// Handle marshaling seam (#267). The binding layer (Lua userdata) and the save
// system (#264) must round-trip a noun handle without touching api internals or
// sim types — but noun handles carry unexported fields ({id sim.EntityID; g
// *Game}) by design (public-api-design.md §2: zero exported fields). This file
// is the one sanctioned seam across that boundary: a handle <-> HandleRef codec
// of plain types.
//
// Decision D-2026-06-17-267-A (ADR issue): handle identity crosses the api
// boundary as an OPAQUE HandleRef{Kind, Raw uint32}, not raw sim types and not
// per-field accessors. Rationale: keeps the public noun surface field-free,
// leaks no sim type, and gives the generated binding layer (and saves) a single
// stable token to persist. Resolve rebuilds the handle; its generation-checked
// Valid() then reports staleness (a recycled slot is detectably stale, R-API-5).

// HandleKind tags which noun a HandleRef reconstructs to. The zero value
// (HandleNone) is the null ref.
type HandleKind uint8

// The Handle* values tag which noun a HandleRef reconstructs to; HandleNone is
// the null ref.
const (
	HandleNone HandleKind = iota
	HandleUnit
	HandleItem
	HandleDestructable
	HandleMissile
	HandleEffect
)

// HandleRef is the persistable, language-portable identity of an entity-backed
// noun handle: the noun kind plus the opaque packed sim entity id (index +
// generation). Plain types only, so it marshals to a Lua userdata payload or a
// save record verbatim.
type HandleRef struct {
	// Kind is the handle's noun type (unit, item, …), for typed re-resolution.
	Kind HandleKind `json:"kind"`
	// Raw is the packed id+generation the handle marshals to.
	Raw uint32 `json:"raw"`
}

// IsZero reports the null ref (no entity).
func (r HandleRef) IsZero() bool { return r.Kind == HandleNone }

// Handle is the common surface of the noun handles (the boxed form Resolve
// returns). Every noun handle already provides Valid/IsZero.
type Handle interface {
	Valid() bool
	IsZero() bool
}

// RefOf extracts the persistable HandleRef of an entity-backed handle. ok is
// false for a handle that is not entity-backed (e.g. Camera/Frame, which are
// index-based) — callers must treat that as "not marshalable through this seam"
// and fail loudly rather than silently drop it.
func RefOf(h Handle) (HandleRef, bool) {
	switch t := h.(type) {
	case Unit:
		return HandleRef{Kind: HandleUnit, Raw: uint32(t.id)}, true
	case Item:
		return HandleRef{Kind: HandleItem, Raw: uint32(t.id)}, true
	case Destructable:
		return HandleRef{Kind: HandleDestructable, Raw: uint32(t.id)}, true
	case Missile:
		return HandleRef{Kind: HandleMissile, Raw: uint32(t.id)}, true
	case Effect:
		return HandleRef{Kind: HandleEffect, Raw: uint32(t.id)}, true
	default:
		return HandleRef{}, false
	}
}

// Resolve rebuilds the noun handle named by ref against this game. ok is false
// for HandleNone or an unknown kind. The returned handle may be stale
// (Valid()==false) if the entity's slot was recycled since ref was taken — the
// caller observes that through Valid(), exactly as for any handle.
func (g *Game) Resolve(ref HandleRef) (Handle, bool) {
	id := sim.EntityID(ref.Raw)
	switch ref.Kind {
	case HandleUnit:
		return Unit{id: id, g: g}, true
	case HandleItem:
		return Item{id: id, g: g}, true
	case HandleDestructable:
		return Destructable{id: id, g: g}, true
	case HandleMissile:
		return Missile{id: id, g: g}, true
	case HandleEffect:
		return Effect{id: id, g: g}, true
	default:
		return nil, false
	}
}
