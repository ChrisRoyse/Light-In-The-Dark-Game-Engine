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
// JASS: GetUnitState, GetUnitStateSwap
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
// JASS: BlzGetUnitMaxHP
func (u Unit) MaxLife() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.MaxLife")
		return 0
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return 0
	}
	return toFloat(u.g.w.BuffedMaxLife(u.id, u.g.w.Healths.MaxLife[r]))
}

// SetMaxLife sets the unit's maximum life (D5 typed accessor over the unitstate
// table; JASS SetUnitState with UNIT_STATE_MAX_LIFE). The new max floors at 1 —
// a unit always has at least one max HP (WC3 semantics). Current life is left
// where it is, except it is clamped down when the new max drops below it.
// No-op on an invalid handle or a unit without a Health component.
// JASS: BlzSetUnitMaxHP
func (u Unit) SetMaxLife(v float64) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetMaxLife")
		return
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return
	}
	nv := fromFloat(v)
	if min := fromFloat(1); nv < min {
		nv = min
	}
	u.g.w.Healths.MaxLife[r] = nv
	// Clamp current life to the buffed cap (a +max-life mod can sit on top of
	// the base we just set), preserving an active buff's headroom (#522).
	if cap := u.g.w.BuffedMaxLife(u.id, nv); u.g.w.Healths.Life[r] > cap {
		u.g.w.Healths.Life[r] = cap
	}
}

// LifePercent returns the unit's current life as a percentage of its
// maximum, in [0,100]. Returns 0 on an invalid handle or a unit with no
// max life (mirrors the BJ's divide-by-zero guard). D4: GetUnitLifePercent,
// GetUnitStatePercent(LIFE, MAX_LIFE) = 100*Life/MaxLife.
// JASS: GetUnitLifePercent, GetUnitStatePercent
func (u Unit) LifePercent() float64 {
	if !u.Valid() {
		u.g.reportInvalid("Unit.LifePercent")
		return 0
	}
	r := u.g.w.Healths.Row(u.id)
	if r < 0 {
		return 0
	}
	max := u.g.w.BuffedMaxLife(u.id, u.g.w.Healths.MaxLife[r])
	if max == 0 {
		return 0
	}
	return toFloat(u.g.w.Healths.Life[r]) / toFloat(max) * 100
}

// SetLife sets the unit's current life, clamped to [0, MaxLife]. No-op
// on an invalid handle.
//
// With no clamp adjustment, behaves like the WC3 SetUnitLifeBJ default:
// values below 0 clamp to 0 (a unit is never set to negative life), and
// values above MaxLife clamp to MaxLife. Setting life to 0 (or below) is
// lethal — the unit is killed, firing the death event in the sim step, just
// as SetUnitState(UNIT_STATE_LIFE, 0) kills in WC3.
// JASS: SetUnitLifeBJ, SetUnitLifePercentBJ, SetUnitState
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
	if max := u.g.w.BuffedMaxLife(u.id, u.g.w.Healths.MaxLife[r]); nv > max {
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
// JASS: GetOwningPlayer
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

// SetOwner reassigns the unit to player p. When changeColor is true the unit's
// team color changes to p's; otherwise it keeps its current color (the WC3
// SetUnitOwner changeColor flag). No-op on an invalid handle or a foreign
// player. This is not a raw store write — it migrates the food ledger and
// refreshes visibility through the sim's ChangeOwner primitive (#362), so the
// derived per-player state moves with the unit. Team defaults to p's own slot
// (FFA, #361) until the alliance model lands (#218). JASS: SetUnitOwner.
// JASS: SetUnitOwner
func (u Unit) SetOwner(p Player, changeColor bool) {
	if !u.Valid() {
		u.g.reportInvalid("Unit.SetOwner")
		return
	}
	if p.g != u.g || !p.Valid() {
		u.g.reportInvalid("Unit.SetOwner (player not from this game)")
		return
	}
	slot := uint8(p.idx)
	u.g.w.ChangeOwner(u.id, slot, slot, changeColor)
}

// OwnedBy reports whether p owns this unit. Returns false on an invalid
// unit handle, an invalid/foreign player, or when the owner slot does
// not match p — never panics. SoT: the unit's Owners.Player slot.
// JASS: IsUnitOwnedByPlayer
func (u Unit) OwnedBy(p Player) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.OwnedBy")
		return false
	}
	if p.g != u.g || !p.Valid() {
		u.g.reportInvalid("Unit.OwnedBy (player not from this game)")
		return false
	}
	r := u.g.w.Owners.Row(u.id)
	if r < 0 {
		return false
	}
	return int32(u.g.w.Owners.Player[r]) == p.idx
}

// VisibleTo reports whether player p can currently see this unit: true
// for p's own units, and for foreign units standing on a cell p has in
// active sight (and not hidden by undetected invisibility). Returns
// false on an invalid unit handle or an invalid/foreign player — never
// panics. SoT: the sim visibility system (CanSeeEntity / fog state).
// JASS: IsUnitVisible
func (u Unit) VisibleTo(p Player) bool {
	if !u.Valid() {
		u.g.reportInvalid("Unit.VisibleTo")
		return false
	}
	if p.g != u.g || !p.Valid() {
		u.g.reportInvalid("Unit.VisibleTo (player not from this game)")
		return false
	}
	return u.g.w.CanSeeEntity(uint8(p.idx), u.id)
}

// Name returns the player's display name, or "" on an invalid handle —
// the tail that lets Unit{}.Owner().Name() degrade to "" rather than
// panic. With no name assigned, returns the slot-derived default.
// JASS: GetPlayerName
func (p Player) Name() string {
	if !p.Valid() {
		p.g.reportInvalid("Player.Name")
		return ""
	}
	if n := p.g.w.PlayerName(uint8(p.idx)); n != "" {
		return n
	}
	return fmt.Sprintf("Player %d", p.idx)
}
