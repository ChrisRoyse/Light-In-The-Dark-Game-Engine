package litd

// R-API-5 — error semantics and zero-value handles
// (public-api-design.md §3.5, execution-model.md §8 rule 2,
// naming-and-style.md G-6).
//
// WC3 scripts are crash-tolerant: a native called on a null or removed
// handle silently does nothing. LitD keeps those semantics, formalized:
// gameplay verbs on an invalid handle are no-ops, getters return zero
// values, and handle chains degrade safely instead of crashing the map.
// Validity is the generation-checked Entities.Alive (handles.go), so a
// reference to a recycled entity slot is detectably stale rather than
// silently aliased to whatever now lives there.
//
// The forgiveness is asymmetric by mode: shipped maps swallow the bad
// call (WC3 behavior); with Debug on, the same call logs its script
// call site so development catches what WC3 would have hidden.
//
// This file carries the validity substrate (debug toggle, the guard
// tail) plus the seed gameplay verbs whose R-API-5 behavior the issue
// pins down; the full game-state surface extends them on this substrate.

import (
	"fmt"
	"log"
	"runtime"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// f64Scale is the 32.32 fixed-point unit as a float64; the boundary
// scale for converting sim fixed-point quantities to the public
// float64 surface (R-API-2) and back.
const f64Scale = float64(fixed.One)

// toFloat converts a sim fixed-point scalar to the public float64.
func toFloat(v fixed.F64) float64 { return float64(v) / f64Scale }

// fromFloat converts a public float64 to the sim fixed-point scalar.
// Pure and deterministic (same float64 → same F64), so a value chosen
// by a script writes the same bits on every machine.
func fromFloat(v float64) fixed.F64 { return fixed.F64(v * f64Scale) }

// SetDebug enables or disables debug-mode invalid-handle assertions
// (R-API-5). This is a setup-time toggle, not a gameplay verb; with it
// on, any verb called on an invalid handle reports its call site
// through OnInvalidHandle (or the standard logger). Nil-receiver safe.
//
// Debug mode: when on, invalid-handle calls are reported, not
// swallowed silently.
func (g *Game) SetDebug(on bool) {
	if g != nil {
		g.debug = on
	}
}

// OnInvalidHandle installs the sink for debug-mode invalid-handle
// reports. Each report names the verb and the caller's call site. When
// unset, reports go to the standard logger. Has no effect outside debug
// mode. Nil-receiver safe.
func (g *Game) OnInvalidHandle(f func(report string)) {
	if g != nil {
		g.onInvalid = f
	}
}

// reportInvalid is the guard tail every gameplay verb calls when its
// receiver fails validation. In a shipped map it is a single
// predictable branch and returns (the call was already a no-op); in
// debug mode it captures the caller's call site so the WC3-swallowed
// bug surfaces. skip=2 walks past reportInvalid and the verb frame to
// name the script-level call site. Nil-receiver safe — a zero-value
// handle has a nil game pointer.
func (g *Game) reportInvalid(verb string) {
	if g == nil || !g.debug {
		return
	}
	site := "?"
	if _, file, line, ok := runtime.Caller(2); ok {
		site = fmt.Sprintf("%s:%d", file, line)
	}
	report := fmt.Sprintf("litd: %s called on invalid handle at %s", verb, site)
	if g.onInvalid != nil {
		g.onInvalid(report)
		return
	}
	log.Println(report)
}

// -- Seed gameplay verbs (R-API-5 reference behavior) -----------------
//
// These are the verbs whose zero-value/no-op contract this issue pins
// down. Each follows the same shape: validate the receiver, report +
// degrade on failure, otherwise translate to the sim store. The wider
// game-state surface is built on the same substrate.

// Life returns the unit's current life, or 0 on an invalid handle.
// JASS: GetUnitState(UNIT_STATE_LIFE).
func (u Unit) Life() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Life")
		return 0
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return 0
	}
	return toFloat(u.g.w.Healths.Life[r])
}

// MaxLife returns the unit's maximum life, or 0 on an invalid handle.
// JASS: GetUnitState(UNIT_STATE_MAX_LIFE).
func (u Unit) MaxLife() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.MaxLife")
		return 0
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return 0
	}
	return toFloat(u.g.w.Healths.MaxLife[r])
}

// SetLife sets the unit's current life, clamped to [0, MaxLife]. No-op
// on an invalid handle.
//
// With no clamp adjustment, behaves like the WC3 SetUnitLifeBJ default:
// values below 0 clamp to 0 (a unit is never set to negative life), and
// values above MaxLife clamp to MaxLife. Setting life to 0 (or below) is
// lethal — the unit is killed, firing the death event in the sim step, just
// as SetUnitState(UNIT_STATE_LIFE, 0) kills in WC3.
// JASS: SetUnitState(UNIT_STATE_LIFE), SetUnitLifeBJ.
func (u Unit) SetLife(v float64) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetLife")
		return
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return
	}
	nv := fromFloat(v)
	if nv < 0 {
		nv = 0
	}
	if max := u.g.w.Healths.MaxLife[r]; nv > max {
		nv = max
	}
	u.g.w.Healths.Life[r] = nv
	if nv == 0 {
		// Lethal set: a unit at 0 life is dead. Mark it for the step's death
		// pass (idempotent with a combat kill the same tick).
		u.g.w.KillUnit(u.id)
	}
}

// Owner returns the unit's owning player, or the zero-value Player on
// an invalid handle (so a chain off an environmental death degrades to
// a no-op, not a crash). JASS: GetOwningPlayer(u).
func (u Unit) Owner() Player {
	if !u.Valid() {
		u.g.reportInvalid("Unit.Owner")
		return Player{}
	}
	r := u.g.w.Owners.Row(u.id)
	if r < 0 {
		return Player{}
	}
	return Player{idx: int32(u.g.w.Owners.Player[r]), g: u.g}
}

// Name returns the player's display name, or "" on an invalid handle —
// the tail that lets Unit{}.Owner().Name() degrade to "" rather than
// panic. With no name assigned, returns the slot-derived default.
// JASS: GetPlayerName(p).
func (p Player) Name() string {
	if !p.Valid() {
		p.g.reportInvalid("Player.Name")
		return ""
	}
	return fmt.Sprintf("Player %d", p.idx)
}
