package litd

// Trigger noun — the public ECA surface (#461, ADR #451). Mirrors the
// WC3 World-Editor trigger model on top of the sim trigger substrate
// (#456–#460): a Trigger holds Events (what fires it), a Condition tree
// (all must pass), and Actions (run when it fires and the condition
// passes). Builder-style: NewTrigger().On(kind).When(pred).Do(action).
//
//	JASS mapping (R-API-4 dedup):
//	  g.NewTrigger()  → CreateTrigger
//	  t.On(kind, …)   → TriggerRegisterUnitEvent / …PlayerUnitEvent / …EnterRegion
//	  t.When(pred)    → TriggerAddCondition (boolexpr leaf)
//	  t.Do(action)    → TriggerAddAction
//	  t.Enable/Disable→ EnableTrigger / DisableTrigger
//	  t.Execute()     → TriggerExecute (run actions, skip events+conditions)
//	  t.Evaluate()    → TriggerEvaluate (conditions only)
//	  t.Destroy()     → DestroyTrigger
//
// Conditions and actions are registered into the sim handler-identity
// registry (#455) under a stable per-game name, so the trigger graph is
// data that survives save/load — the Go closures are re-established by
// re-running the same setup on load, relinking the same names to the
// same refs.

import (
	"fmt"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// Trigger is a generation-checked handle to a sim trigger. The zero
// value is invalid; methods on it are no-ops (R-API-5).
type Trigger struct {
	id sim.TriggerID
	g  *Game
}

// pubKindOf reverses simKindOf so an action can recover the public kind
// of the event that fired it. Built once, lazily.
func (g *Game) pubKindOf(simKind uint16) EventKind {
	if g.pubKindRev == nil {
		g.pubKindRev = make(map[uint16]EventKind, len(simKindOf))
		for pk, sk := range simKindOf {
			// first writer wins for a shared sim kind; deterministic enough
			// for action payload kind (the dispatch path already fixed it).
			if _, ok := g.pubKindRev[sk]; !ok {
				g.pubKindRev[sk] = pk
			}
		}
	}
	return g.pubKindRev[simKind]
}

// nextTriggerHandlerName returns a stable, deterministic name for the
// next condition/action adapter. Setup runs in fixed order, so the Nth
// adapter always gets the same name across a run and a reload.
func (g *Game) nextTriggerHandlerName(role string) string {
	n := g.trigHandlerSeq
	g.trigHandlerSeq++
	return fmt.Sprintf("api.trig.%s.%d", role, n)
}

// NewTrigger creates an empty, enabled trigger. Returns a zero Trigger
// (fail-closed) if the slab is full or the game is invalid.
func (g *Game) NewTrigger() Trigger {
	if g == nil || g.w == nil {
		return Trigger{}
	}
	id, ok := g.w.Triggers.New()
	if !ok {
		g.reportInvalid("NewTrigger (trigger slab full)")
		return Trigger{}
	}
	return Trigger{id: id, g: g}
}

// Valid reports whether the trigger still names a live sim trigger.
func (t Trigger) Valid() bool {
	return t.g != nil && t.g.w != nil && t.g.w.Triggers.Valid(t.id)
}

// TriggerScope configures an On() registration: an event may be global,
// scoped to one unit (a true index scope on the event source), or scoped
// to a player / region (which compile to a condition, since the sim
// scope key is the source entity, not a player or area).
type TriggerScope func(*triggerScopeCfg)

type triggerScopeCfg struct {
	scopeUnit sim.EntityID // event index scope key (0 = global)
	// rawConds gate on the sim event directly (they need the source entity,
	// which the pure EventView deliberately hides).
	rawConds []func(*sim.World, sim.Event) bool
}

// ForUnit scopes the event to a single unit source (the
// TriggerRegisterUnitEvent role) — a true index scope (#458).
func ForUnit(u Unit) TriggerScope {
	return func(c *triggerScopeCfg) { c.scopeUnit = u.id }
}

// OwnedBy scopes the event to units owned by p (the
// TriggerRegisterPlayerUnitEvent role). Compiles to an owner condition.
func OwnedBy(p Player) TriggerScope {
	slot := p.idx
	g := p.g
	return func(c *triggerScopeCfg) {
		c.rawConds = append(c.rawConds, func(w *sim.World, e sim.Event) bool {
			return g != nil && g.ownerOf(e.Src) == slot
		})
	}
}

// InRegion scopes the event to its source being inside region r (the
// TriggerRegisterEnterRegion role, simplified to a containment gate).
// Compiles to a region-containment condition.
func InRegion(r Region) TriggerScope {
	g := r.g
	return func(c *triggerScopeCfg) {
		c.rawConds = append(c.rawConds, func(w *sim.World, e sim.Event) bool {
			return g != nil && w.RegionContainsUnit(r.id, r.gen, e.Src)
		})
	}
}

// On registers an event that fires the trigger, applying any scopes.
// Unknown kinds are rejected fail-closed. Returns t for chaining.
func (t Trigger) On(kind EventKind, scopes ...TriggerScope) Trigger {
	if !t.Valid() {
		return t
	}
	simKind, ok := simKindOf[kind]
	if !ok {
		t.g.reportInvalid("Trigger.On (unknown event kind)")
		return t
	}
	var cfg triggerScopeCfg
	for _, s := range scopes {
		s(&cfg)
	}
	t.g.w.Triggers.AddEvent(t.id, sim.EventReg{Kind: simKind, Scope: cfg.scopeUnit})
	for _, c := range cfg.rawConds {
		t.addRawCondition(c)
	}
	return t
}

// When adds a condition: the trigger fires only when pred returns true.
// Multiple conditions AND together (the WC3 all-conditions-pass rule).
// pred receives a read-only EventView and must be pure.
func (t Trigger) When(pred func(EventView) bool) Trigger {
	if !t.Valid() || pred == nil {
		return t
	}
	t.addCondition(EventKind(0), pred)
	return t
}

// WhenEvent adds a condition that receives the full public Event rather
// than the pure EventView. It is the compile target for a script
// TriggerAddCondition, where a condition reads the triggering unit /
// player / region through the Event accessors (the WC3 GetTriggerUnit
// idiom) that the deliberately entity-free EventView hides. Like When,
// conditions AND together and pred must be pure. Registered under the same
// "cond" handler-name role as When, so a Go When and a script WhenEvent at
// the same position carry identical sim identity (save/load + parity).
func (t Trigger) WhenEvent(pred func(Event) bool) Trigger {
	if !t.Valid() || pred == nil {
		return t
	}
	g := t.g
	t.andLeaf(g.w.RegisterHandlerID(g.nextTriggerHandlerName("cond"),
		func(w *sim.World, e sim.Event) bool { return pred(g.eventOf(e)) }))
	return t
}

// addCondition registers a pure EventView predicate as a sim condition
// handler and ANDs it into the trigger's condition root. pubKindHint
// builds the EventView (0 = derive from the fired kind).
func (t Trigger) addCondition(pubKindHint EventKind, pred func(EventView) bool) {
	g := t.g
	t.andLeaf(g.w.RegisterHandlerID(g.nextTriggerHandlerName("cond"),
		func(w *sim.World, e sim.Event) bool { return pred(g.eventViewOf(pubKindHint, e)) }))
}

// addRawCondition registers a sim-level condition (it sees the source
// entity) and ANDs it in. Used by the scope options that need the
// entity the pure EventView hides.
func (t Trigger) addRawCondition(fn func(*sim.World, sim.Event) bool) {
	g := t.g
	t.andLeaf(g.w.RegisterHandlerID(g.nextTriggerHandlerName("scope"),
		func(w *sim.World, e sim.Event) bool { return fn(w, e) }))
}

// andLeaf folds a registered condition ref into the trigger's condition
// tree as a new AND leaf.
func (t Trigger) andLeaf(ref sim.HandlerRef) {
	g := t.g
	leaf := g.w.Cond(ref)
	if cur := g.w.Triggers.Condition(t.id); cur == sim.NoExpr {
		g.w.Triggers.SetCondition(t.id, leaf)
	} else {
		g.w.Triggers.SetCondition(t.id, g.w.And(cur, leaf))
	}
}

// Do adds an action run when the trigger fires and its condition passes.
// Actions run on the cooperative scheduler in add order.
func (t Trigger) Do(action func(Event)) Trigger {
	if !t.Valid() || action == nil {
		return t
	}
	g := t.g
	name := g.nextTriggerHandlerName("act")
	ref := g.w.RegisterHandlerID(name, func(w *sim.World, e sim.Event) bool {
		action(g.eventOf(e))
		return true
	})
	g.w.Triggers.AddAction(t.id, ref)
	return t
}

// viewOf builds the read-only EventView a condition sees. pubKindHint of
// 0 means "use the fired event's kind".
func (g *Game) eventViewOf(pubKindHint EventKind, e sim.Event) EventView {
	pk := pubKindHint
	if pk == 0 {
		pk = g.pubKindOf(e.Kind)
	}
	ev := Event{kind: pk, src: e.Src, dst: e.Dst, arg: e.Arg, g: g}
	return EventView{kind: pk, damage: ev.Damage(), ownerPlayer: g.ownerOf(ev.primary())}
}

// eventOf builds the public Event an action sees.
func (g *Game) eventOf(e sim.Event) Event {
	return Event{kind: g.pubKindOf(e.Kind), src: e.Src, dst: e.Dst, arg: e.Arg, g: g}
}

// Enable turns the trigger on (EnableTrigger). No-op on a stale handle.
func (t Trigger) Enable() Trigger {
	if t.Valid() {
		t.g.w.Triggers.Enable(t.id)
	}
	return t
}

// Disable turns the trigger off; it keeps its registration but never
// fires until re-enabled (DisableTrigger).
func (t Trigger) Disable() Trigger {
	if t.Valid() {
		t.g.w.Triggers.Disable(t.id)
	}
	return t
}

// IsEnabled reports the live enabled flag.
func (t Trigger) IsEnabled() bool {
	return t.Valid() && t.g.w.Triggers.IsEnabled(t.id)
}

// SetInitiallyOn seeds the trigger's initial enabled state (Initially-On
// editor flag). SetInitiallyOn(false) starts it disabled.
func (t Trigger) SetInitiallyOn(on bool) Trigger {
	if t.Valid() {
		t.g.w.Triggers.SetInitiallyOn(t.id, on)
	}
	return t
}

// Every arms the trigger to fire on a periodic-timer event every `period`
// of game time (quantized up to whole sim ticks, drift-free), first firing
// one period from now. Unlike a Go-closure timer the schedule is a
// value-typed scheduler continuation (sim #464), so it round-trips through
// save/load. The trigger's actions run each period when its condition
// passes. This is the serializable substrate under the script Game_Every.
func (t Trigger) Every(period time.Duration) Trigger {
	if t.Valid() {
		t.g.w.ArmPeriodic(t.id, durationToTicks(period))
	}
	return t
}

// Execute runs the trigger's actions now, bypassing its events and
// conditions (TriggerExecute) — the run-from-another-trigger verb.
func (t Trigger) Execute() {
	if t.Valid() {
		t.g.w.ExecuteTrigger(t.id, sim.Event{})
	}
}

// Evaluate runs only the trigger's conditions and returns the result
// (TriggerEvaluate); no actions, no side effects.
func (t Trigger) Evaluate() bool {
	return t.Valid() && t.g.w.EvaluateTrigger(t.id, sim.Event{})
}

// Destroy removes the trigger; its handle goes stale (DestroyTrigger).
func (t Trigger) Destroy() {
	if t.Valid() {
		t.g.w.Triggers.Destroy(t.id)
	}
}

// OwnerIs is a ready-made When predicate: the event's primary unit is
// owned by p. The common "only my units" condition.
func OwnerIs(p Player) func(EventView) bool {
	slot := p.idx
	return func(v EventView) bool { return int32(v.OwnerPlayer()) == slot }
}
