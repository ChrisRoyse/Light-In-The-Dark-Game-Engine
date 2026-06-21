package litd

// OnEvent — the trigger-zoo replacement (public-api-design.md §3.4
// R-API-4). CreateTrigger / TriggerRegisterUnitEvent /
// TriggerAddCondition / TriggerAddAction collapse into one call that
// takes a Go closure, with DestroyTrigger/DisableTrigger replaced by
// Subscription.Cancel.
//
// Dispatch order is the deterministic contract (execution-model.md
// §2.4): within one tick, handlers fire in registration order
// (registrationSeq); across multiple firings of the same kind they
// fire in emit order (firingSeq). Both fall straight out of the sim
// ring — it flushes events in emit order and per kind in
// subscription order — so the public layer adds no reordering. One
// sim "trampoline" handler is registered per public kind; it fans out
// to that kind's subscriber list, which the closure captures by
// pointer so the dispatch hot path touches no map and allocates
// nothing at steady state.

import (
	"log"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// subscription is one live OnEvent registration. Since #462 OnEvent is
// sugar over a single-event / single-condition / single-action Trigger:
// the condition replicates the legacy dispatch gate (cancelled → scope →
// filter) and the action calls the handler. `trigger` is the backing sim
// trigger; Cancel flips `cancelled`, which the condition checks.
type subscription struct {
	handler   func(Event)
	filter    func(EventView) bool
	player    int32 // owner-slot scope; -1 = any player
	cancelled bool
	g         *Game
	trigger   sim.TriggerID
}

// Subscription is the handle returned by OnEvent; Cancel stops future
// dispatch (replacing DestroyTrigger / DisableTrigger).
type Subscription struct {
	s *subscription
}

// Cancel stops the subscription from firing on subsequent events,
// including the rest of the current tick. Idempotent; safe to call
// from inside the subscription's own handler. Zero-value Cancel is a
// no-op.
func (s Subscription) Cancel() {
	if s.s != nil {
		s.s.cancelled = true
	}
}

// EventOption configures a subscription at registration (R-API-3
// options; N-7 declarative modifiers).
type EventOption func(*subscription)

// ForPlayer scopes a subscription to events whose primary unit is owned
// by p — the TriggerRegisterPlayerUnitEvent role. Omitting it is the
// any-player default (the AnyUnitEventBJ role).
func ForPlayer(p Player) EventOption {
	return func(s *subscription) { s.player = p.idx }
}

// Where attaches a filter: the handler fires only when pred returns
// true. pred receives a read-only EventView and must be pure — it
// cannot reach a mutating verb, the Game, or a wait (execution-model.md
// §4). In debug mode the filter is sampled twice per firing and a
// differing result is reported as an impurity warning.
func Where(pred func(EventView) bool) EventOption {
	return func(s *subscription) { s.filter = pred }
}

// OnEvent registers handler for kind and returns its Subscription.
// Unknown kinds are rejected fail-closed (a zero-value Subscription,
// plus a debug report) rather than silently never firing. Nil-receiver
// and nil-handler safe.
//
// Debug mode: an unknown kind or a nil handler is reported through
// OnInvalidHandle.
func (g *Game) OnEvent(kind EventKind, handler func(Event), opts ...EventOption) Subscription {
	if g == nil || g.w == nil || handler == nil {
		if g != nil {
			g.reportInvalid("OnEvent (nil handler or game)")
		}
		return Subscription{}
	}
	simKind, ok := simKindOf[kind]
	if !ok {
		g.reportInvalid("OnEvent (unknown event kind)")
		return Subscription{}
	}
	sub := &subscription{handler: handler, player: -1, g: g}
	for _, o := range opts {
		o(sub)
	}
	// OnEvent is sugar over a one-event/one-condition/one-action Trigger
	// (#462, ADR #451). The condition replicates the legacy dispatch gate
	// byte-for-byte: a cancelled sub is skipped, then ForPlayer scope, then
	// the Where filter, in that order with the same EventView. The action
	// runs the handler. Dispatch order and snapshot-at-firing semantics fall
	// out of the trigger substrate: registration order is slot order, and a
	// trigger registered mid-dispatch joins from the next firing (#459/#458).
	t, ok := g.w.Triggers.New()
	if !ok {
		g.reportInvalid("OnEvent (trigger slab full)")
		return Subscription{}
	}
	sub.trigger = t
	g.w.Triggers.AddEvent(t, sim.EventReg{Kind: simKind})

	condRef := g.w.RegisterHandlerID(g.nextTriggerHandlerName("onevent.cond"),
		func(w *sim.World, e sim.Event) bool {
			if sub.cancelled {
				return false
			}
			ev := Event{kind: kind, src: e.Src, dst: e.Dst, arg: e.Arg, g: g, sub: sub}
			if sub.player >= 0 && g.scopePlayerOf(ev) != sub.player {
				return false
			}
			if sub.filter != nil && !g.runFilter(sub, ev) {
				return false
			}
			return true
		})
	g.w.Triggers.SetCondition(t, g.w.Cond(condRef))

	actRef := g.w.RegisterHandlerID(g.nextTriggerHandlerName("onevent.act"),
		func(w *sim.World, e sim.Event) bool {
			ev := Event{kind: kind, src: e.Src, dst: e.Dst, arg: e.Arg, g: g, sub: sub}
			sub.handler(ev)
			return true
		})
	g.w.Triggers.AddAction(t, actRef)

	return Subscription{s: sub}
}

// runFilter evaluates a subscription's filter. In debug mode it samples
// the filter twice on the same view; a differing result means the
// filter is impure (it mutated captured state or read nondeterministic
// data) and is reported — the double-run purity check of
// execution-model.md §4. The first result is authoritative.
func (g *Game) runFilter(s *subscription, ev Event) bool {
	view := EventView{kind: ev.kind, damage: ev.Damage(), ownerPlayer: g.ownerOf(ev.primary())}
	r := s.filter(view)
	if g.debug {
		if r2 := s.filter(view); r2 != r {
			g.warnImpureFilter()
		}
	}
	return r
}

// scopePlayerOf returns the player slot used by ForPlayer. Player
// events carry the slot directly; unit events scope through the primary
// unit owner.
func (g *Game) scopePlayerOf(ev Event) int32 {
	if p := ev.playerSlot(); p >= 0 {
		return p
	}
	return g.ownerOf(ev.primary())
}

// ownerOf returns the owner slot of an entity, or -1 if it has no owner
// row (or is invalid). A store lookup, not an iteration — no allocation.
func (g *Game) ownerOf(id sim.EntityID) int32 {
	if !g.w.Ents.Alive(id) {
		return -1
	}
	r := g.w.Owners.Row(id)
	if r < 0 {
		return -1
	}
	return int32(g.w.Owners.Player[r])
}

// warnImpureFilter routes the debug double-run impurity warning to the
// invalid-handle sink (the package's debug-diagnostic channel) or the
// standard logger.
func (g *Game) warnImpureFilter() {
	const msg = "litd: event filter is impure — returned different results across the debug double-run (it must not mutate captured state or read nondeterministic data)"
	if g.onInvalid != nil {
		g.onInvalid(msg)
		return
	}
	log.Println(msg)
}
