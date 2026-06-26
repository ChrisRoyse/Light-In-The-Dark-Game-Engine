package litd

// Public custom-event surface (PRD2 04, #619). Scripts mint named event
// kinds, emit them, and subscribe via the existing OnEvent (widened to
// accept registered custom kinds). The hot payload is scalar
// (src/dst/arg); a group ref rides arg for fan-out, and richer params
// ride the KV store keyed off the event's src entity (the "bag").

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

func simEvent(kind EventKind, src, dst Unit, arg int64) sim.Event {
	return sim.Event{Kind: uint16(kind), Src: src.id, Dst: dst.id, Arg: arg}
}

// Arg returns the raw scalar payload of a (typically custom) event.
// Built-in kinds expose typed accessors (Damage, BuffStacks, …); custom
// events carry their own meaning in this int64.
func (e Event) Arg() int64 { return e.arg }

// GroupArg interprets the event's arg as a Group reference — the payload
// EmitGroup writes. Stale/zero if the event carried no group.
func (e Event) GroupArg() Group { return Group{id: sim.GroupID(uint32(e.arg)), g: e.g} }

// RegisterEvent registers (or returns the existing id for) a named custom
// event kind — idempotent (R-EVT-1/5). Returns the zero EventKind if the
// registry is full. Call at world setup so the id is deterministic.
func (g *Game) RegisterEvent(name string) EventKind {
	if g == nil || g.w == nil {
		return 0
	}
	return EventKind(g.w.CustomEvents.RegisterEventKind(name))
}

// EventKindByName returns the kind id for an already-registered name, or
// the zero EventKind if it was never registered (read-only).
func (g *Game) EventKindByName(name string) EventKind {
	if g == nil || g.w == nil {
		return 0
	}
	return EventKind(g.w.CustomEvents.KindOf(name))
}

// Emit queues a custom (or built-in) event for this tick's dispatch.
// Returns false on an invalid/unregistered kind (fail-closed) or a full
// event ring. src/dst may be zero Units; arg is the scalar payload.
func (g *Game) Emit(kind EventKind, src, dst Unit, arg int64) bool {
	if g == nil || g.w == nil || !g.w.ValidEventKind(uint16(kind)) {
		if g != nil {
			g.reportInvalid("Emit (invalid event kind)")
		}
		return false
	}
	return g.w.Emit(simEvent(kind, src, dst, arg))
}

// EmitGroup emits a custom event carrying a Group reference in arg, the
// fan-out payload (the handler resolves it with the group verbs). Returns
// false on an invalid kind or full ring.
func (g *Game) EmitGroup(kind EventKind, src Unit, grp Group) bool {
	if g == nil || g.w == nil || !g.w.ValidEventKind(uint16(kind)) {
		if g != nil {
			g.reportInvalid("EmitGroup (invalid event kind)")
		}
		return false
	}
	return g.w.Emit(simEvent(kind, src, Unit{}, int64(uint32(grp.id))))
}
