// Package litd is the public, idiomatic Go API for the Light in the
// Dark engine (import path litd/api, package name litd per
// naming-and-style.md N-9). It is the only package a game developer
// imports.
//
// The package owns no gameplay state of its own (architecture.md
// §1.1): every public noun is a small, copyable handle — an entity id
// plus a back-pointer to the Game — whose methods translate directly
// into litd/sim commands and queries. No litd/sim, litd/render, or G3N
// type appears in any exported signature (architecture.md import rules
// IMP-3, public-api-design.md R-API-6).
//
// This file establishes the root object and the shared validity
// primitives; the noun handles live in handles.go. Gameplay verbs
// (creation, orders, state accessors, the OnEvent dispatcher) land in
// the per-noun issues that build on this skeleton.
package litd

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Game is the root object (public-api-design.md §2 row 1): all
// authority flows from the Game value a developer is handed, which is
// what keeps the surface sandbox-friendly for the Lua binding (every
// capability hangs off this object — there are no package-level
// mutating functions). It holds the deterministic simulation world but
// exposes none of its internal types across the public boundary.
type Game struct {
	w *sim.World

	// driver is the wall-clock loop hook for root game-state verbs
	// such as Pause/Resume/SetSpeed. It is deliberately an unexported
	// tiny interface so no driver, render, or G3N type leaks into the
	// public API.
	driver driverHook

	// match holds immutable map/match metadata once a map loader exists.
	// Tests seed it directly; production construction will fill it from
	// map assets.
	match matchConfig

	// debug enables R-API-5 invalid-handle assertions; off in shipped
	// maps (WC3 forgiveness), on in development (catch the swallowed
	// bug). Toggled via SetDebug.
	debug bool
	// onInvalid is the optional sink for debug-mode invalid-handle
	// reports; nil routes them to the standard logger.
	onInvalid func(report string)

	// eventKinds maps a sim event kind to its public dispatch list,
	// consulted only at OnEvent registration time (never on the
	// dispatch hot path — each list is reached through a closure). nil
	// until the first OnEvent call.
	eventKinds map[uint16]*kindList
	// nextHandlerID hands out sim HandlerIDs for the per-kind
	// trampolines from a high base that cannot collide with
	// script-registered handlers.
	nextHandlerID sim.HandlerID
}

// newGame wraps a simulation world. The public setup path —
// NewGame(cfg) (*Game, error) with map load and the headless toggle —
// lands with its own issue; this unexported constructor is the seam
// internal callers and tests use to obtain a Game over an existing
// world without leaking a litd/sim type through an exported signature.
func newGame(w *sim.World) *Game {
	return &Game{
		w:             w,
		eventKinds:    make(map[uint16]*kindList),
		nextHandlerID: apiHandlerBase,
	}
}

func newGameWithDriver(w *sim.World, d driverHook) *Game {
	g := newGame(w)
	g.driver = d
	return g
}

// apiHandlerBase starts the api's sim-HandlerID allocation high enough
// that it never collides with a script- or sim-registered handler.
const apiHandlerBase sim.HandlerID = 1 << 30

// alive reports whether an entity handle still names a live sim entity.
// It is the shared validity primitive for every entity-backed noun
// (Unit, Item, Destructable, Missile): the generation check in
// Entities.Alive makes a handle to a recycled slot detectably invalid
// rather than silently aliased (R-API-5). Nil-safe on the receiver and
// on the wrapped world so a zero-value handle's Valid() is a clean
// false, never a panic.
func (g *Game) alive(id sim.EntityID) bool {
	return g != nil && g.w != nil && g.w.Ents.Alive(id)
}

// playerValid reports whether idx names a real player slot. Player
// handles are index-based, not entity-based, so they validate against
// the fixed slot count rather than the entity generation table.
func (g *Game) playerValid(idx int32) bool {
	return g != nil && g.w != nil && idx >= 0 && idx < sim.MaxPlayers
}
