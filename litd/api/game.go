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
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
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

	// effectModels is the setup-time asset table projection used by
	// AddSpecialEffect: public model keys resolve to sim/render model
	// ids, and unknown keys fail closed.
	effectModels map[string]uint16

	// aiControllers holds the attached computer-player strategies (#257),
	// keyed by player slot. This is api-runtime state, not deterministic sim
	// state: the controller is a Go behavior dispatched by the sandboxed AI
	// scheduler domain at M5.5. The replay-safe inputs (difficulty, paused,
	// command stack) live in the sim. nil until the first AttachAI.
	aiControllers map[uint8]AIController

	// aiDomain is the live second scheduler domain (R-EXEC-3) hosting one
	// isolated context per attached computer player. Lazily created on the
	// first AttachAI and ticked every sim tick in the AI sub-phase of phase 2
	// (via w.OnAIPhase). nil until then. aiBudget is the per-player counted
	// resume slice handed to Domain.Tick (0 disables the watchdog).
	aiDomain *ai.Domain
	aiBudget int

	// debug enables R-API-5 invalid-handle assertions; off in shipped
	// maps (WC3 forgiveness), on in development (catch the swallowed
	// bug). Toggled via SetDebug.
	debug bool
	// onInvalid is the optional sink for debug-mode invalid-handle
	// reports; nil routes them to the standard logger.
	onInvalid func(report string)

	// onAudio is the optional presentation sink for the sound/music
	// surface (#244). Audio is render-only and sim-inert (R-AUD-1): the
	// verbs validate + clamp their args and forward the resolved event
	// here. nil (the headless default) makes every audio verb a
	// deterministic no-op. The render driver and tests install a sink.
	onAudio func(AudioEvent)

	// camera surface (#248): per-player render-only view state, zero sim
	// coupling. localPlayer is the local viewer slot (-1 = none/headless);
	// camera verbs for any other player are recorded no-ops so a script
	// can never branch the sim on the local view (R-RND-1 / desync guard).
	// cam holds each player's applied field values + cinematic/follow
	// state (the SoT for the clamp tests); onCamera is the render/test sink.
	localPlayer int32
	cam         [sim.MaxPlayers]cameraState
	onCamera    func(CameraEvent)

	// onUI is the optional presentation sink for the UI text-message
	// surface (#245). The UI frame/dialog/board/quest system is render-
	// only with no v1 G3N backend (deferred-v2); the one sim-inert
	// primitive that ships now is Game.Print/ClearMessages, whose
	// resolved per-recipient events forward here. nil (headless default)
	// makes Print a deterministic no-op.
	onUI func(UIMessageEvent)

	// eventKinds maps a sim event kind to its public dispatch list,
	// consulted only at OnEvent registration time (never on the
	// dispatch hot path — each list is reached through a closure). nil
	// until the first OnEvent call.
	eventKinds map[uint16]*kindList
	// nextHandlerID hands out sim HandlerIDs for the per-kind
	// trampolines from a high base that cannot collide with
	// script-registered handlers.
	nextHandlerID sim.HandlerID

	// timers is the Go-closure timer table (timer.go): one entry per
	// live or retired-and-recyclable slot, indexed by Timer.slot.
	// timerFree holds retired slots for reuse; timerContReg records
	// whether the shared scheduler continuation that fires these timers
	// has been registered on this game's scheduler yet (lazy, once).
	timers       []timerEntry
	timerFree    []uint32
	timerContReg bool

	// forces is the script-side player-group registry (force.go, #218).
	// A force is a set of player slots (a bitset); Force.id is index+1.
	// Forces are transient script convenience state — like triggers, they
	// are not part of the hashed/serialized sim. CreateForce appends.
	forces []uint32

	// damageHandlers is the writable-damage filter channel (damage_event.go,
	// #219): synchronous pre-apply modifiers run inside the sim's combat
	// phase. damageHookInstalled records whether the single sim-side hook
	// that fans out to them has been wired yet (lazy, once).
	damageHandlers      []func(*DamageEvent)
	damageHookInstalled bool

	// queryScratch is the reusable id buffer behind the spatial query
	// verbs (queries.go, #239): AppendUnitsIn* fill it from the sim then
	// project to Unit handles, so the steady-state query path allocates
	// nothing (R-GC-2).
	queryScratch []sim.EntityID

	// storage is the campaign cross-map key-value store (savedata.go,
	// #242) — the gamecache replacement reached via Game.Storage().
	// Lazily created; persisted explicitly via Storage.Save/Load.
	storage *Storage

	// threads is the cooperative Go script-thread table (thread.go, #377):
	// one entry per live or retired-and-recyclable green thread, indexed by
	// Thread.slot. threadFree holds retired slots for reuse; threadContReg
	// records whether the shared scheduler continuation that resumes them
	// has been registered yet (lazy, once). suspendedThreads counts threads
	// currently parked on a wait — the save path fails closed if non-zero
	// (a parked Go stack is not serializable; same posture as Go timers).
	threads          []threadEntry
	threadFree       []uint32
	threadContReg    bool
	suspendedThreads int

	// worldLoader is the host-installed backend for LoadWorld (#268). It is
	// nil until SetWorldLoader wires the luabind-backed loader; litd/api never
	// imports the script runtime, so the loader crosses in through this seam.
	worldLoader WorldLoader
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
		localPlayer:   -1, // no local viewer until set (headless default)
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
