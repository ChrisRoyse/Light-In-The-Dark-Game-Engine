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
	// HandleStorage is the per-game campaign store singleton (Game.Storage()).
	// It is not entity-backed — Raw is unused — but it must marshal so a script
	// closure that captures the store (e.g. a Game_Every callback, #464)
	// round-trips through save/load. It resolves back to the live game's store.
	HandleStorage
	// HandlePlayer / HandleUnitType / HandleBuffType / HandleFogModifier marshal
	// the non-entity handles a world script commonly captures as a Game_Every /
	// trigger upvalue (#481): a player slot, a bound type-catalog ref, or a live
	// fog modifier. Raw carries the player slot, the type ref (typeIdx+1), or the
	// fog-modifier id respectively — all stable across a save/load of the same
	// world (its type tables rebind in the same order; the fog modifier is sim
	// state). Without these a script holding e.g. Game_BuffType("burn") in an
	// upvalue fails closed at SaveScripts.
	HandlePlayer
	HandleUnitType
	HandleBuffType
	HandleFogModifier
	// HandleItemType / HandleResourceNodeType marshal the remaining stable
	// type-catalog refs a world script can capture as an upvalue (#489) —
	// e.g. Game_ItemType("ember-cloak") or Game_ResourceNodeType("emberwell")
	// held by a Game_Every / trigger closure. Raw carries the type ref
	// (typeIdx+1), stable across a save/load of the same world exactly like
	// HandleUnitType/HandleBuffType. Without these a script holding such a ref
	// fails closed at SaveScripts.
	HandleItemType
	HandleResourceNodeType
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
	case *Storage:
		// the campaign store is a per-game singleton; Raw is unused.
		return HandleRef{Kind: HandleStorage}, true
	case Player:
		return HandleRef{Kind: HandlePlayer, Raw: uint32(t.idx)}, true
	case UnitType:
		return HandleRef{Kind: HandleUnitType, Raw: uint32(t.ref)}, true
	case BuffType:
		return HandleRef{Kind: HandleBuffType, Raw: uint32(t.ref)}, true
	case FogModifier:
		return HandleRef{Kind: HandleFogModifier, Raw: uint32(t.id)}, true
	case ItemType:
		return HandleRef{Kind: HandleItemType, Raw: uint32(t.ref)}, true
	case ResourceNodeType:
		return HandleRef{Kind: HandleResourceNodeType, Raw: uint32(t.ref)}, true
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
	case HandleStorage:
		return g.Storage(), true // the per-game campaign store singleton
	case HandlePlayer:
		return Player{idx: int32(ref.Raw), g: g}, true
	case HandleUnitType:
		return UnitType{ref: uint16(ref.Raw)}, true
	case HandleBuffType:
		return BuffType{ref: uint16(ref.Raw)}, true
	case HandleFogModifier:
		return FogModifier{g: g, id: sim.FogModifierID(ref.Raw)}, true
	case HandleItemType:
		return ItemType{ref: uint16(ref.Raw)}, true
	case HandleResourceNodeType:
		return ResourceNodeType{ref: uint16(ref.Raw)}, true
	default:
		return nil, false
	}
}
