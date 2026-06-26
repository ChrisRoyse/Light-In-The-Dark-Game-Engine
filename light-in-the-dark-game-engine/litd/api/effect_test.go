package litd

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestAddSpecialEffectMutatorsStore(t *testing.T) {
	w := sim.NewWorld(effectAPITestCaps())
	g := newGame(w)
	g.RegisterEffectModel("fx/flare", 41)

	beforeSpawn := apiEffectStoreDump(w)
	e := g.AddSpecialEffect("fx/flare", Vec2{X: 12, Y: 34})
	afterSpawn := apiEffectStoreDump(w)
	t.Logf("FSV AddSpecialEffect happy BEFORE: %s", beforeSpawn)
	t.Logf("FSV AddSpecialEffect happy AFTER spawn: handle=%#x valid=%v %s", uint32(e.id), e.Valid(), afterSpawn)

	if !e.Valid() {
		t.Fatal("AddSpecialEffect returned an invalid handle for a registered model")
	}
	r := effectAPIRow(t, w, e)
	tr := effectAPITransformRow(t, w, e)
	if w.Effects.ModelID[r] != 41 {
		t.Fatalf("model id = %d, want 41; store=%s", w.Effects.ModelID[r], afterSpawn)
	}
	if w.Effects.Scale[r] != fixed.One {
		t.Fatalf("default scale raw = %d, want %d; store=%s", int64(w.Effects.Scale[r]), int64(fixed.One), afterSpawn)
	}
	if w.Effects.ColorRGBA[r] != defaultEffectColorRGBA {
		t.Fatalf("default color = %#08x, want %#08x; store=%s", w.Effects.ColorRGBA[r], defaultEffectColorRGBA, afterSpawn)
	}
	if w.Transforms.Pos[tr] != vec(Vec2{X: 12, Y: 34}) {
		t.Fatalf("spawn position = %+v, want (12,34); store=%s", w.Transforms.Pos[tr], afterSpawn)
	}

	beforeMutate := apiEffectStoreDump(w)
	e.SetScale(2.5)
	e.SetColor(0xaa, 0xbb, 0xcc)
	e.SetPosition(Vec2{X: -5.5, Y: 99.25})
	afterMutate := apiEffectStoreDump(w)
	t.Logf("FSV Effect mutators BEFORE: %s", beforeMutate)
	t.Logf("FSV Effect mutators AFTER SetScale(2.5)/SetColor/SetPosition: %s", afterMutate)

	r = effectAPIRow(t, w, e)
	tr = effectAPITransformRow(t, w, e)
	if got, want := w.Effects.Scale[r], fromFloat(2.5); got != want {
		t.Fatalf("scale raw = %d, want exact 2.5 fixed raw %d; store=%s", int64(got), int64(want), afterMutate)
	}
	if got, want := w.Effects.ColorRGBA[r], uint32(0xaabbccff); got != want {
		t.Fatalf("color = %#08x, want %#08x; store=%s", got, want, afterMutate)
	}
	if got, want := w.Transforms.Pos[tr], vec(Vec2{X: -5.5, Y: 99.25}); got != want {
		t.Fatalf("position = %+v, want %+v; store=%s", got, want, afterMutate)
	}

	beforeDestroy := apiEffectStoreDump(w)
	e.Destroy()
	afterDestroy := apiEffectStoreDump(w)
	t.Logf("FSV Effect.Destroy BEFORE: %s", beforeDestroy)
	t.Logf("FSV Effect.Destroy AFTER:  handleValid=%v %s", e.Valid(), afterDestroy)
	if e.Valid() || w.Effects.Count() != 0 {
		t.Fatalf("Destroy did not remove the effect: valid=%v store=%s", e.Valid(), afterDestroy)
	}
}

func TestAddSpecialEffectUnknownModelNoMutation(t *testing.T) {
	w := sim.NewWorld(effectAPITestCaps())
	g := newGame(w)
	var reports []string
	g.OnInvalidHandle(func(report string) { reports = append(reports, report) })
	g.SetDebug(true)

	before := apiEffectStoreDump(w)
	e := g.AddSpecialEffect("fx/missing", Vec2{X: 1, Y: 2})
	after := apiEffectStoreDump(w)
	t.Logf("FSV unknown model BEFORE: %s", before)
	t.Logf("FSV unknown model AFTER:  handleZero=%v handleValid=%v reports=%v %s", e.IsZero(), e.Valid(), reports, after)

	if !e.IsZero() || e.Valid() {
		t.Fatalf("unknown model returned non-zero/live handle: zero=%v valid=%v", e.IsZero(), e.Valid())
	}
	if before != after {
		t.Fatalf("unknown model mutated store: before=%s after=%s", before, after)
	}
	if !reportsContain(reports, "Game.AddSpecialEffect", "unknown model") {
		t.Fatalf("debug report missing unknown-model detail: %v", reports)
	}
}

func TestEffectStaleHandleNoOpsAndRecycledSlot(t *testing.T) {
	w := sim.NewWorld(effectAPITestCaps())
	g := newGame(w)
	g.RegisterEffectModel("fx/first", 7)
	g.RegisterEffectModel("fx/second", 8)

	first := g.AddSpecialEffect("fx/first", Vec2{X: 3, Y: 4})
	if !first.Valid() {
		t.Fatal("first effect did not spawn")
	}
	firstID := first.id
	beforeDestroy := apiEffectStoreDump(w)
	first.Destroy()
	afterDestroy := apiEffectStoreDump(w)
	t.Logf("FSV stale setup BEFORE first.Destroy: %s", beforeDestroy)
	t.Logf("FSV stale setup AFTER first.Destroy:  firstValid=%v %s", first.Valid(), afterDestroy)

	second := g.AddSpecialEffect("fx/second", Vec2{X: 5, Y: 6})
	if !second.Valid() {
		t.Fatal("second effect did not spawn")
	}
	secondID := second.id
	if firstID.Index() != secondID.Index() {
		t.Fatalf("test setup expected slot reuse: first idx=%d second idx=%d", firstID.Index(), secondID.Index())
	}
	if firstID.Generation() == secondID.Generation() {
		t.Fatalf("slot reused without generation change: first=%#x second=%#x", uint32(firstID), uint32(secondID))
	}

	var reports []string
	g.OnInvalidHandle(func(report string) { reports = append(reports, report) })
	g.SetDebug(true)
	beforeStale := apiEffectStoreDump(w)
	first.SetScale(9)
	first.SetColor(1, 2, 3)
	first.SetPosition(Vec2{X: 99, Y: 99})
	first.Destroy()
	afterStale := apiEffectStoreDump(w)
	t.Logf("FSV recycled slot BEFORE stale verbs: first=%#x second=%#x firstValid=%v secondValid=%v %s",
		uint32(firstID), uint32(secondID), first.Valid(), second.Valid(), beforeStale)
	t.Logf("FSV recycled slot AFTER stale verbs:  reports=%v %s", reports, afterStale)

	if first.Valid() {
		t.Fatal("stale effect handle reported Valid()=true after slot recycle")
	}
	if !second.Valid() {
		t.Fatal("live recycled-slot effect became invalid")
	}
	if beforeStale != afterStale {
		t.Fatalf("stale verbs mutated the recycled slot: before=%s after=%s", beforeStale, afterStale)
	}
	for _, verb := range []string{"Effect.SetScale", "Effect.SetColor", "Effect.SetPosition", "Effect.Destroy"} {
		if !reportsContain(reports, verb) {
			t.Fatalf("debug reports missing %s: %v", verb, reports)
		}
	}
}

func effectAPITestCaps() sim.Caps {
	return sim.Caps{Units: 1, Projectiles: 1, Effects: 4, ScriptedDoodads: 1}
}

func effectAPIRow(t *testing.T, w *sim.World, e Effect) int32 {
	t.Helper()
	r := w.Effects.Row(e.id)
	if r == -1 {
		t.Fatalf("effect row missing for handle %#x; store=%s", uint32(e.id), apiEffectStoreDump(w))
	}
	return r
}

func effectAPITransformRow(t *testing.T, w *sim.World, e Effect) int32 {
	t.Helper()
	r := w.Transforms.Row(e.id)
	if r == -1 {
		t.Fatalf("transform row missing for handle %#x; store=%s", uint32(e.id), apiEffectStoreDump(w))
	}
	return r
}

func reportsContain(reports []string, needles ...string) bool {
	for _, report := range reports {
		matched := true
		for _, needle := range needles {
			if !strings.Contains(report, needle) {
				matched = false
				break
			}
		}
		if matched {
			return true
		}
	}
	return false
}

func apiEffectStoreDump(w *sim.World) string {
	if w == nil || w.Effects == nil {
		return "<nil>"
	}
	var b strings.Builder
	fmt.Fprintf(&b, "count=%d [", w.Effects.Count())
	for r := 0; r < int(w.Effects.Count()); r++ {
		if r > 0 {
			b.WriteByte(' ')
		}
		id := w.Effects.Entity[r]
		tr := w.Transforms.Row(id)
		fmt.Fprintf(&b, "row=%d ent=%#x idx=%d gen=%d alive=%v model=%d scaleRaw=%d scale=%g color=%#08x birth=%d transformRow=%d",
			r,
			uint32(id),
			id.Index(),
			id.Generation(),
			w.Ents.Alive(id),
			w.Effects.ModelID[r],
			int64(w.Effects.Scale[r]),
			toFloat(w.Effects.Scale[r]),
			w.Effects.ColorRGBA[r],
			w.Effects.BirthTick[r],
			tr,
		)
		if tr >= 0 {
			pos := w.Transforms.Pos[tr]
			fmt.Fprintf(&b, " pos=(%g,%g)", toFloat(pos.X), toFloat(pos.Y))
		} else {
			b.WriteString(" pos=<missing>")
		}
	}
	b.WriteByte(']')
	return b.String()
}
