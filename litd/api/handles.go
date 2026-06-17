package litd

import (
	"math"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// This file declares the public noun handles and value types
// (public-api-design.md §2, the 21-row inventory). Every noun handle
// is a small copyable struct with zero exported fields and a *Game
// back-pointer; value types (Vec2, Rect, Angle, Order, Event) carry
// their data directly. The methods here are the skeleton contract —
// Valid()/IsZero() and value math/accessors — that the per-noun
// gameplay-verb issues extend.
//
// Handle-size budget: every noun handle stays at or under 24 bytes
// (public-api-design.md §2; checked by TestHandleSizeBudget). The
// shape {id; *Game} is 16 bytes on a 64-bit target.

// -- Entity-backed nouns ----------------------------------------------
//
// These wrap a sim.EntityID and resolve Valid() through the
// generation-checked entity table, so a handle to a recycled slot is
// detectably stale (R-API-5).

// Unit is a controllable game entity (public-api-design.md §2 row 4):
// the JASS unit/unitstate/unittype/unitpool families plus the order
// and animation natives collapse onto methods of this type.
type Unit struct {
	id sim.EntityID
	g  *Game
}

// Valid reports whether the unit still exists. A zero-value Unit{} and
// a handle to a destroyed unit both report false. JASS: a no-op-on-null
// guard, made queryable.
func (u Unit) Valid() bool { return u.g.alive(u.id) }

// IsZero reports whether this is the zero-value handle (no bound game).
func (u Unit) IsZero() bool { return u == Unit{} }

func (Unit) isWidget() {}

// Item is a pickup/inventory entity (public-api-design.md §2 row 5).
type Item struct {
	id sim.EntityID
	g  *Game
}

// Valid reports whether the item still exists. A zero-value Item{} and a
// handle to a destroyed/recycled item both report false (R-API-5).
func (i Item) Valid() bool { return i.g.alive(i.id) }

// IsZero reports whether this is the zero-value handle (no bound game).
func (i Item) IsZero() bool { return i == Item{} }
func (Item) isWidget()      {}

// Destructable is a destroyable doodad — trees, gates, breakable rocks
// (public-api-design.md §2 row 6).
type Destructable struct {
	id sim.EntityID
	g  *Game
}

// Valid reports whether the destructable still exists. A zero-value
// Destructable{} and a handle to a destroyed one both report false (R-API-5).
func (d Destructable) Valid() bool { return d.g.alive(d.id) }

// IsZero reports whether this is the zero-value handle (no bound game).
func (d Destructable) IsZero() bool { return d == Destructable{} }
func (Destructable) isWidget()      {}

// Missile is a first-class flying sim object (public-api-design.md §2
// row 21, R-SIM-7) — no JASS analogue. Spawnable, queryable, and
// retargetable mid-flight; backed by the sim missile entity pool.
type Missile struct {
	id sim.EntityID
	g  *Game
}

// Valid reports whether the missile still exists in the flight pool. A
// zero-value Missile{} and a handle to an expired missile both report false.
func (m Missile) Valid() bool { return m.g.alive(m.id) }

// IsZero reports whether this is the zero-value handle (no bound game).
func (m Missile) IsZero() bool { return m == Missile{} }

// Widget is the common surface of the targetable world nouns —
// Unit, Item, Destructable (public-api-design.md §2 row 7). The sealing
// isWidget method keeps the set closed to this package: no outside type
// can claim to be a Widget.
type Widget interface {
	Valid() bool
	IsZero() bool
	isWidget()
}

// -- Index/sentinel nouns ---------------------------------------------
//
// These do not map to an entity slot; their validity is a stub bound
// to the game pointer and a non-zero identifier, refined by the
// per-noun issues.

// Player is a player slot (public-api-design.md §2 row 2): the
// player/playerstate/playercolor/alliance families collapse here.
type Player struct {
	idx int32
	g   *Game
}

// Valid reports whether the handle names a real player slot. A
// zero-value Player{} has no bound game and reports false even though
// slot 0 is itself a legal player index.
func (p Player) Valid() bool { return p.g.playerValid(p.idx) }

// IsZero reports whether this is the zero-value handle (no bound game).
func (p Player) IsZero() bool { return p == Player{} }

// Force is a stateful player group (public-api-design.md §2 row 3):
// kept a noun, not a bare slice, because alliance and visibility state
// live per-force.
type Force struct {
	id uint32
	g  *Game
}

// Valid reports whether the handle names a live force in the bound game.
func (f Force) Valid() bool { return f.g != nil && f.id != 0 && int(f.id) <= len(f.g.forces) }

// IsZero reports whether this is the zero-value handle (no bound game).
func (f Force) IsZero() bool { return f == Force{} }

// Ability is one ability instance on a unit (public-api-design.md §2
// row 8): the D5 BlzGetAbility*Field reflection matrix collapses onto
// this type's field accessors. Bound to its owning unit so validity
// follows the owner's lifetime.
type Ability struct {
	owner sim.EntityID
	ref   uint32
	g     *Game
}

// Valid reports whether the ability instance still exists on its owning unit;
// it reports false once the owner dies or the ability is removed (R-API-5).
func (a Ability) Valid() bool {
	_, _, ok := a.g.abilitySlot(a.owner, a.ref)
	return ok
}

// IsZero reports whether this is the zero-value handle (no bound game).
func (a Ability) IsZero() bool { return a == Ability{} }

// Buff is one buff/aura instance on a unit (public-api-design.md §2
// row 9), which JASS hides in ability+unitstate corners.
type Buff struct {
	owner sim.EntityID
	ref   uint32
	g     *Game
}

// Valid reports whether the buff instance still exists on its owning unit
// (false once the owner dies or the buff lapses).
func (b Buff) Valid() bool { return b.g.alive(b.owner) && b.ref != 0 }

// IsZero reports whether this is the zero-value handle (no bound game).
func (b Buff) IsZero() bool { return b == Buff{} }

// Timer is a game-time timer (public-api-design.md §2 row 11), backed
// by the deterministic scheduler. Durations quantize to sim ticks on
// entry. The handle is a (slot, generation) pair into the Game's timer
// table: a slot is reused after Stop, but always under a fresh
// generation, so a stale handle to a stopped timer is detectably
// invalid rather than silently aliased onto whatever now occupies the
// slot (R-API-5; timers.md porting hazard 5 — identities never recycle).
type Timer struct {
	slot uint32
	gen  uint32
	g    *Game
}

// Valid reports whether the timer still exists (created, not yet
// Stopped, not auto-retired after a one-shot fire). A zero-value
// Timer{} and a handle to a retired slot both report false.
func (t Timer) Valid() bool { return t.entry() != nil }

// IsZero reports whether this is the zero-value handle (no bound game).
func (t Timer) IsZero() bool { return t == Timer{} }

// Region is a cell-based area (public-api-design.md §2 row 13): a
// script-created set of 32-wu grid cells with point/unit containment.
// The handle is a (id, generation) pair into the sim region store, so a
// stale handle to a removed region is detectably invalid, never aliased
// (R-API-5). Enter/leave events arrive separately (tracked on #371).
type Region struct {
	id  uint32
	gen uint32
	g   *Game
}

// Valid reports whether the region still exists (created, not Removed).
func (r Region) Valid() bool { return r.g != nil && r.g.w != nil && r.g.w.Regions.Alive(r.id, r.gen) }

// IsZero reports whether this is the zero-value handle (no bound game).
func (r Region) IsZero() bool { return r == Region{} }

// Sound is a playable sound handle (public-api-design.md §2 row 17). In
// headless mode its verbs are deterministic no-ops.
type Sound struct {
	id uint32
	g  *Game
}

// Valid reports whether the handle names a live sound in the bound game.
func (s Sound) Valid() bool { return s.g != nil && s.id != 0 }

// IsZero reports whether this is the zero-value handle (no bound game).
func (s Sound) IsZero() bool { return s == Sound{} }

// Effect is a persistent presentation entity — special effects now,
// with lightning, text tags, and minimap icons joining the same noun
// family as their stores land (public-api-design.md §2 row 18).
type Effect struct {
	id sim.EntityID
	g  *Game
}

// Valid reports whether the effect is still playing (false once it has been
// destroyed or has finished a one-shot animation).
func (e Effect) Valid() bool { return e.g.effectAlive(e.id) }

// IsZero reports whether this is the zero-value handle (no bound game).
func (e Effect) IsZero() bool { return e == Effect{} }

// Camera is the RTS camera control surface (public-api-design.md §2
// row 19), clamped per R-RND-1.
type Camera struct {
	id uint32
	g  *Game
}

// Valid reports whether the handle names a live camera in the bound game.
func (c Camera) Valid() bool { return c.g != nil && c.id != 0 }

// IsZero reports whether this is the zero-value handle (no bound game).
func (c Camera) IsZero() bool { return c == Camera{} }

// Frame is a UI element under Game.UI() (public-api-design.md §2 row
// 20): the WC3 frame-native capability (dialogs, buttons, leaderboards,
// multiboards, quests).
type Frame struct {
	id uint32
	g  *Game
}

// Valid reports whether the handle names a live UI frame in the bound game.
func (f Frame) Valid() bool { return f.g != nil && f.id != 0 }

// IsZero reports whether this is the zero-value handle (no bound game).
func (f Frame) IsZero() bool { return f == Frame{} }

// -- Value types ------------------------------------------------------
//
// Plain values: copied on the stack, zero allocations, trivially
// serializable and Lua-marshalable (R-API-2). They carry no game
// pointer (Event excepted — it carries one to resolve its context
// nouns) and need no generation check.

// Vec2 is a 2-D world position or displacement (public-api-design.md §2
// row 15): JASS's heap-allocated location and every (real x, real y)
// pair collapse to this value, ending the RemoveLocation leak hazard.
type Vec2 struct {
	X, Y float64
}

// Add returns the component-wise sum. Vector math is value-in,
// value-out — no aliasing, no allocation.
// JASS: OffsetLocation
func (v Vec2) Add(o Vec2) Vec2 { return Vec2{X: v.X + o.X, Y: v.Y + o.Y} }

// Sub returns the component-wise difference.
func (v Vec2) Sub(o Vec2) Vec2 { return Vec2{X: v.X - o.X, Y: v.Y - o.Y} }

// IsZero reports whether this is the zero vector.
func (v Vec2) IsZero() bool { return v == Vec2{} }

// Rect is an axis-aligned world rectangle (public-api-design.md §2 row
// 14), the value form of JASS rect.
type Rect struct {
	MinX, MinY, MaxX, MaxY float64
}

// IsZero reports whether this is the zero rectangle.
func (r Rect) IsZero() bool { return r == Rect{} }

// Angle is an orientation value (public-api-design.md §2 row 16). The
// radians/degrees confusion that plagued JASS facing reals ends here:
// the field is private and reachable only through the unit-tagged
// constructors Deg/Rad and the Degrees/Radians accessors (N-10).
type Angle struct {
	rad float64
}

// Deg constructs an Angle from a degree measure.
// JASS: Deg2Rad
func Deg(d float64) Angle { return Angle{rad: d * math.Pi / 180} }

// Rad constructs an Angle from a radian measure.
func Rad(r float64) Angle { return Angle{rad: r} }

// Degrees returns the angle in degrees.
// JASS: Rad2Deg
func (a Angle) Degrees() float64 { return a.rad * 180 / math.Pi }

// Radians returns the angle in radians.
func (a Angle) Radians() float64 { return a.rad }

// IsZero reports whether this is the zero angle (0 radians).
func (a Angle) IsZero() bool { return a == Angle{} }

// Order is the order verb value (public-api-design.md §2 row 10): the
// string-order and integer-order native twins collapse to one typed
// value (D3). The catalog of named orders (litd.OrderAttackMove, …) is
// defined alongside the order-issuing verbs.
type Order struct {
	id uint16
}

// IsZero reports whether this is the zero (unset) order.
func (o Order) IsZero() bool { return o == Order{} }

// Event is an event payload (public-api-design.md §2 row 12): the JASS
// event/eventid handles plus every Get* event-context native collapse
// onto methods of this value, dispatched through Game.OnEvent. The type
// and its accessors are defined in event_payload.go.

// Compile-time proof that the targetable nouns satisfy Widget
// (public-api-design.md §2 row 7). These fail the build if a Widget
// method is dropped from any of the three.
var (
	_ Widget = Unit{}
	_ Widget = Item{}
	_ Widget = Destructable{}
)
