package sim

// FSV for the effect-primitive Action library (#465, ADR #452). SoT = the
// primitive's OWN effect — the real damage exec's victim-HP delta (a
// hand-computed X+X=Y), the area combinator's per-target fan-out via a trace
// exec, and the registration error for an invalid spec. Plus parity: the same
// Action delivered by a trigger vs by a data ability produces the identical
// HP outcome.

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

const evActionTest uint16 = 100

// armedDamageWorld: core execs + a magic/none damage matrix (coefficient 1.0)
// + the damage-30 firebolt ability bound; an attacker (team 0) and a victim
// (team 1, 100 life, armor "none").
func armedDamageWorld(t *testing.T) (*World, *data.Tables, EntityID, EntityID) {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs()
	tb := abilityTables(t)
	w := NewWorld(Caps{})
	if err := w.BindDamageMatrix(tb.Coeff); err != nil {
		t.Fatal(err)
	}
	if err := w.BindEffects(tb.Effects); err != nil {
		t.Fatal(err)
	}
	atk := atkUnit(t, w, 0, fixed.Vec2{X: fixed.FromInt(100), Y: fixed.FromInt(100)}, 0)
	vic := atkUnit(t, w, 1, fixed.Vec2{X: fixed.FromInt(120), Y: fixed.FromInt(100)}, 0)
	return w, tb, atk, vic
}

func victimLife(w *World, v EntityID) int64 { return w.Healths.Life[w.Healths.Row(v)].Floor() }

// TestEffectActionDamageRealHP — happy path with the REAL damage exec: a
// trigger Action damage(amount=30, attack-type=magic) on an armor-"none"
// victim (coefficient 1.0) → 30 effective → 100 − 30 = 70.
func TestEffectActionDamageRealHP(t *testing.T) {
	w, _, atk, vic := armedDamageWorld(t)
	ref, err := w.RegisterEffectAction("act.damage30", EffectActionSpec{
		Prim:   data.EPDamage,
		Params: []EffectActionParam{{"amount", 30}, {"attack-type", 0}},
	})
	if err != nil {
		t.Fatalf("register damage Action: %v", err)
	}
	tr, _ := w.Triggers.New()
	w.Triggers.AddEvent(tr, EventReg{Kind: evActionTest})
	w.Triggers.AddAction(tr, ref)

	t.Logf("BEFORE: victim life=%d (want 100)", victimLife(w, vic))
	w.Emit(Event{Kind: evActionTest, Src: atk, Dst: vic})
	for i := 0; i < 5; i++ {
		w.Step()
	}
	got := victimLife(w, vic)
	t.Logf("AFTER trigger Action damage(amount=30,magic): victim life=%d (want 70 = 100 − 30×1.0)", got)
	if got != 70 {
		t.Fatalf("victim life=%d, want 70", got)
	}
}

// TestEffectActionVsDataParity — edge 4: the same damage Action delivers the
// identical HP outcome whether invoked by a trigger or run directly as a data
// ability (the firebolt arena entry is the same damage-30-magic primitive).
// The full StateHash additionally carries the trigger graph in the trigger
// case, so the parity assertion is on the delivered effect (the SoT); the
// trigger path's own determinism is checked separately.
func TestEffectActionVsDataParity(t *testing.T) {
	// trigger-delivered.
	wT, _, atkT, vicT := armedDamageWorld(t)
	refT, err := wT.RegisterEffectAction("act.dmg", EffectActionSpec{
		Prim: data.EPDamage, Params: []EffectActionParam{{"amount", 30}, {"attack-type", 0}}})
	if err != nil {
		t.Fatal(err)
	}
	trT, _ := wT.Triggers.New()
	wT.Triggers.AddEvent(trT, EventReg{Kind: evActionTest})
	wT.Triggers.AddAction(trT, refT)
	wT.Emit(Event{Kind: evActionTest, Src: atkT, Dst: vicT})
	for i := 0; i < 5; i++ {
		wT.Step()
	}

	// data-ability-delivered: run the firebolt composition directly.
	wD, tbD, atkD, vicD := armedDamageWorld(t)
	wD.ExecuteEffects(tbD.Abilities[0].Effects, EffectCtx{Source: atkD, Target: vicD})
	for i := 0; i < 5; i++ {
		wD.Step()
	}

	hpT, hpD := victimLife(wT, vicT), victimLife(wD, vicD)
	t.Logf("trigger-Action victim life=%d | data-ability victim life=%d (want equal, 70)", hpT, hpD)
	if hpT != hpD || hpT != 70 {
		t.Fatalf("effect parity broken: trigger=%d data=%d (want both 70)", hpT, hpD)
	}
}

// TestEffectActionAreaCombinator — edge 2: an `area` combinator Action runs
// its child effect once per in-radius target. Uses trace execs so the fan-out
// is directly observable (the area exec's synthetic fan-out hits two targets).
func TestEffectActionAreaCombinator(t *testing.T) {
	var trace []string
	registerTraceExecs(t, &trace) // resets + registers recording execs for all prims

	w := NewWorld(Caps{})
	ref, err := w.RegisterEffectAction("act.area", EffectActionSpec{
		Prim:   data.EPArea,
		Params: []EffectActionParam{{"radius", int64(fixed.FromInt(300))}, {"max-targets", 2}},
		Children: []EffectActionSpec{{
			Prim:   data.EPDamage,
			Params: []EffectActionParam{{"amount", 25}, {"attack-type", 0}},
		}},
	})
	if err != nil {
		t.Fatalf("register area Action: %v", err)
	}
	tr, _ := w.Triggers.New()
	w.Triggers.AddEvent(tr, EventReg{Kind: evActionTest})
	w.Triggers.AddAction(tr, ref)

	w.Emit(Event{Kind: evActionTest, Src: 7, Dst: 9})
	w.Step()
	for i, line := range trace {
		t.Logf("trace[%d] = %s", i, line)
	}
	// area(src=7) then its damage child once per synthetic target (100,101).
	want := []string{
		"area(src=7 tgt=9 depth=0)",
		"damage(amount=25 src=7 tgt=100 depth=1)",
		"damage(amount=25 src=7 tgt=101 depth=1)",
	}
	if len(trace) != len(want) {
		t.Fatalf("area Action trace len %d, want %d: %v", len(trace), len(want), trace)
	}
	for i := range want {
		if trace[i] != want[i] {
			t.Fatalf("trace[%d]=%q, want %q", i, trace[i], want[i])
		}
	}
}

// TestEffectActionDispatchZeroAlloc — the constraint: invoking an Action on
// the dispatch path allocates nothing (R-GC-1). Measured over a no-op exec so
// the only allocations would be the adapter's own (none — ExecuteEffects runs
// a stack ctx over the read-only arena).
func TestEffectActionDispatchZeroAlloc(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	for id := data.EffectPrimID(0); id < data.EffectPrimCount; id++ {
		RegisterEffectExec(id, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {})
	}
	w := NewWorld(Caps{})
	ref, err := w.RegisterEffectAction("act.noop", EffectActionSpec{
		Prim: data.EPHeal, Params: []EffectActionParam{{"amount", 5}}})
	if err != nil {
		t.Fatal(err)
	}
	fn, ok := w.ResolveHandlerRef(ref)
	if !ok {
		t.Fatal("action handler not resolvable")
	}
	e := Event{Kind: evActionTest, Src: 1, Dst: 2}
	n := testing.AllocsPerRun(1000, func() { fn(w, e) })
	t.Logf("action dispatch: %v allocs/op", n)
	if n != 0 {
		t.Fatalf("action dispatch allocates %v/op, want 0", n)
	}
}

// TestEffectActionFailClosed — edge 3 + the registration-validation surface:
// every malformed spec is refused at registration (never a silent no-op), and
// a refused registration leaves no handler and no partial arena growth.
func TestEffectActionFailClosed(t *testing.T) {
	cases := []struct {
		name string
		spec EffectActionSpec
		want string
	}{
		{"negative amount (below Min 0)", EffectActionSpec{
			Prim: data.EPDamage, Params: []EffectActionParam{{"amount", -5}, {"attack-type", 0}}}, "out of bounds"},
		{"missing required param", EffectActionSpec{
			Prim: data.EPDamage, Params: []EffectActionParam{{"attack-type", 0}}}, "missing required param"},
		{"unknown param", EffectActionSpec{
			Prim: data.EPHeal, Params: []EffectActionParam{{"amount", 10}, {"bogus", 1}}}, `no param "bogus"`},
		{"leaf with children", EffectActionSpec{
			Prim: data.EPHeal, Params: []EffectActionParam{{"amount", 10}},
			Children: []EffectActionSpec{{Prim: data.EPHeal, Params: []EffectActionParam{{"amount", 1}}}}}, "cannot have children"},
		{"combinator without children", EffectActionSpec{
			Prim: data.EPArea, Params: []EffectActionParam{{"radius", int64(fixed.FromInt(100))}, {"max-targets", 1}}}, "requires a child"},
	}
	for _, tc := range cases {
		// fresh world per case, with ALL execs registered so the only failure is
		// the spec itself (not a missing exec).
		var trace []string
		registerTraceExecs(t, &trace)
		w := NewWorld(Caps{})
		arenaBefore := len(w.effects)
		ref, err := w.RegisterEffectAction("act."+tc.name, tc.spec)
		t.Logf("%-30s → err=%v (want contains %q)", tc.name, err, tc.want)
		if err == nil {
			t.Fatalf("%s: registration SUCCEEDED, want fail-closed", tc.name)
		}
		if !strings.Contains(err.Error(), tc.want) {
			t.Fatalf("%s: err=%q, want it to contain %q", tc.name, err, tc.want)
		}
		if ref != NoHandler {
			t.Fatalf("%s: returned ref %d on failure, want NoHandler", tc.name, ref)
		}
		if len(w.effects) != arenaBefore {
			t.Fatalf("%s: arena grew from %d to %d on a failed registration (no rollback)", tc.name, arenaBefore, len(w.effects))
		}
	}
}

// TestEffectActionUnimplementedPrimFailClosed — an Action naming a primitive
// with no registered exec is refused, exactly like BindEffects (#465 "same
// fail-closed validation"). EPSummon has no core exec.
func TestEffectActionUnimplementedPrimFailClosed(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	RegisterCoreEffectExecs() // damage/area/apply-buff only — summon is unimplemented
	w := NewWorld(Caps{})
	_, err := w.RegisterEffectAction("act.summon", EffectActionSpec{
		Prim: data.EPSummon, Params: []EffectActionParam{{"unit-type", 1}, {"count", 1}}})
	t.Logf("summon Action (no exec) → err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "no registered exec") {
		t.Fatalf("unimplemented primitive Action: err=%v, want 'no registered exec' fail-closed", err)
	}
}
