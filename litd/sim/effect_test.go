package sim

import (
	"fmt"
	"strings"
	"testing"
	"testing/fstest"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// effectTables loads a synthetic data set whose one ability carries
// the canonical nested composition: damage, then area{damage, heal}.
func effectTables(t *testing.T) *data.Tables {
	t.Helper()
	fsys := fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte(`
attack-types = ["normal", "piercing"]
armor-types = ["light", "heavy"]
[coefficients]
normal = [1000, 700]
piercing = [2000, 350]
`)},
		"abilities/core.toml": &fstest.MapFile{Data: []byte(`
[[ability]]
id = "firebolt"
name = "Firebolt"

[[ability.effects]]
prim = "damage"
amount = 50
attack-type = "piercing"

[[ability.effects]]
prim = "area"
radius = 300.0
max-targets = 2

[[ability.effects.effects]]
prim = "damage"
amount = 25
attack-type = "normal"

[[ability.effects.effects]]
prim = "heal"
amount = 10
`)},
	}
	tb, err := data.Load(fsys)
	if err != nil {
		t.Fatalf("synthetic tables must load: %v", err)
	}
	return tb
}

// registerTraceExecs installs recording execs for every primitive;
// the area combinator fans out to two synthetic targets. Returns the
// trace sink.
func registerTraceExecs(t *testing.T, trace *[]string) {
	t.Helper()
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	for id := data.EffectPrimID(0); id < data.EffectPrimCount; id++ {
		id := id
		schema := &data.EffectSchemas[id]
		if !schema.Combinator {
			RegisterEffectExec(id, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {
				*trace = append(*trace, fmt.Sprintf("%s(amount=%d src=%d tgt=%d depth=%d)",
					schema.Name, e.Params[0], ctx.Source, ctx.Target, ctx.Depth))
			})
			continue
		}
		RegisterEffectExec(id, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {
			*trace = append(*trace, fmt.Sprintf("%s(src=%d tgt=%d depth=%d)",
				schema.Name, ctx.Source, ctx.Target, ctx.Depth))
			// synthetic fan-out: two targets, ids 100+depth-scoped
			for n := EntityID(0); n < 2; n++ {
				child := ctx
				child.Target = 100 + n
				w.RunEffectChildren(e, child)
			}
		})
	}
}

// The walker runs the compiled composition in arena order and
// combinators recurse per target with depth bookkeeping.
func TestEffectExecuteWalker(t *testing.T) {
	tb := effectTables(t)
	var trace []string
	registerTraceExecs(t, &trace)

	w := NewWorld(Caps{})
	if err := w.BindEffects(tb.Effects); err != nil {
		t.Fatalf("bind: %v", err)
	}
	w.ExecuteEffects(tb.Abilities[0].Effects, EffectCtx{Source: 7, Target: 9})

	want := []string{
		"damage(amount=50 src=7 tgt=9 depth=0)",
		"area(src=7 tgt=9 depth=0)",
		"damage(amount=25 src=7 tgt=100 depth=1)",
		"heal(amount=10 src=7 tgt=100 depth=1)",
		"damage(amount=25 src=7 tgt=101 depth=1)",
		"heal(amount=10 src=7 tgt=101 depth=1)",
	}
	for i, line := range trace {
		t.Logf("trace[%d] = %s", i, line)
	}
	if len(trace) != len(want) {
		t.Fatalf("trace len %d, want %d", len(trace), len(want))
	}
	for i := range want {
		if trace[i] != want[i] {
			t.Errorf("trace[%d] = %q, want %q", i, trace[i], want[i])
		}
	}
}

// Fail-closed bind: an arena using a primitive nobody registered is
// refused, naming the primitive.
func TestEffectBindFailClosed(t *testing.T) {
	tb := effectTables(t)
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	// register everything EXCEPT heal
	for id := data.EffectPrimID(0); id < data.EffectPrimCount; id++ {
		if id == data.EPHeal {
			continue
		}
		RegisterEffectExec(id, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {})
	}
	w := NewWorld(Caps{})
	err := w.BindEffects(tb.Effects)
	if err == nil || !strings.Contains(err.Error(), `uses primitive "heal" but no exec is registered`) {
		t.Fatalf("bind err = %v, want unregistered-heal refusal", err)
	}
	if w.effects != nil {
		t.Fatal("failed bind must not install the arena")
	}
	// empty arena binds against an empty registry — nothing used,
	// nothing required
	if err := w.BindEffects(nil); err != nil {
		t.Fatalf("empty arena: %v", err)
	}
}

// Registry discipline: duplicate, nil, out-of-range, and post-freeze
// registrations panic — programming errors never limp.
func TestEffectRegisterPanics(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	mustPanic := func(name string, fn func()) {
		t.Helper()
		defer func() {
			if r := recover(); r == nil {
				t.Fatalf("%s: no panic", name)
			} else {
				t.Logf("%s panicked: %v", name, r)
			}
		}()
		fn()
	}
	noop := func(w *World, ctx EffectCtx, e *data.CompiledEffect) {}
	RegisterEffectExec(data.EPDamage, noop)
	mustPanic("duplicate", func() { RegisterEffectExec(data.EPDamage, noop) })
	mustPanic("nil exec", func() { RegisterEffectExec(data.EPHeal, nil) })
	mustPanic("out of range", func() { RegisterEffectExec(data.EffectPrimCount, noop) })
	FreezeEffectExecs()
	mustPanic("after freeze", func() { RegisterEffectExec(data.EPHeal, noop) })
}

// Registry agreement (the #146 cross-check): the data schema table is
// fully populated and the sim can register an exec for every ID —
// the two sides cover the same closed set.
func TestEffectRegistryAgreement(t *testing.T) {
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	for id := data.EffectPrimID(0); id < data.EffectPrimCount; id++ {
		name := data.EffectSchemas[id].Name
		if name == "" {
			t.Fatalf("data schema %d has no name — registry hole", id)
		}
		RegisterEffectExec(id, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {})
		t.Logf("prim %d %-14s combinator=%v params=%d", id, name,
			data.EffectSchemas[id].Combinator, len(data.EffectSchemas[id].Params))
	}
}

// R-GC-1: executing a composition allocates nothing — contexts are
// stack values, the arena is read-only.
func TestEffectExecuteAllocs(t *testing.T) {
	tb := effectTables(t)
	resetEffectExecs()
	t.Cleanup(resetEffectExecs)
	for id := data.EffectPrimID(0); id < data.EffectPrimCount; id++ {
		schema := &data.EffectSchemas[id]
		if schema.Combinator {
			RegisterEffectExec(id, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {
				child := ctx
				child.Target = 100
				w.RunEffectChildren(e, child)
			})
		} else {
			RegisterEffectExec(id, func(w *World, ctx EffectCtx, e *data.CompiledEffect) {})
		}
	}
	w := NewWorld(Caps{})
	if err := w.BindEffects(tb.Effects); err != nil {
		t.Fatal(err)
	}
	list := tb.Abilities[0].Effects
	allocs := testing.AllocsPerRun(100, func() {
		w.ExecuteEffects(list, EffectCtx{Source: 1, Target: 2, Point: fixed.Vec2{}})
	})
	if allocs != 0 {
		t.Fatalf("ExecuteEffects allocates %v/run, want 0 (R-GC-1)", allocs)
	}
}
