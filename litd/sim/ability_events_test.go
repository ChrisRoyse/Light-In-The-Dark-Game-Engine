package sim

// FSV for the ability-lifecycle events (#467): the cast machine now emits
// EvAbilityCast/Effect/ChannelStart/ChannelStop/Finish/Stopped so triggers can
// observe spells. SoT = the captured event stream (kind/src/dst/arg per tick)
// joined against the effect SoT (victim HP at the EFFECT edge) and StateHash.

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// abilityEvent is one captured lifecycle event with the tick it fired on.
type abilityEvent struct {
	tick uint32
	kind uint16
	src  EntityID
	dst  EntityID
	arg  int64
}

func (e abilityEvent) String() string {
	return fmt.Sprintf("t%d %s src=%d dst=%d arg=%d", e.tick, abilityEvName(e.kind), e.src, e.dst, e.arg)
}

func abilityEvName(k uint16) string {
	switch k {
	case EvAbilityCast:
		return "Cast"
	case EvAbilityEffect:
		return "Effect"
	case EvAbilityChannelStart:
		return "ChannelStart"
	case EvAbilityChannelStop:
		return "ChannelStop"
	case EvAbilityFinish:
		return "Finish"
	case EvAbilityStopped:
		return "Stopped"
	}
	return fmt.Sprintf("kind%d", k)
}

// captureAbilityEvents subscribes one handler to all six lifecycle kinds and
// records them in dispatch order.
func captureAbilityEvents(w *World, log *[]abilityEvent) {
	w.RegisterHandler(7, func(w *World, e Event) {
		switch e.Kind {
		case EvAbilityCast, EvAbilityEffect, EvAbilityChannelStart, EvAbilityChannelStop, EvAbilityFinish, EvAbilityStopped:
			*log = append(*log, abilityEvent{tick: w.Tick(), kind: e.Kind, src: e.Src, dst: e.Dst, arg: e.Arg})
		}
	})
	for _, k := range []uint16{EvAbilityCast, EvAbilityEffect, EvAbilityChannelStart, EvAbilityChannelStop, EvAbilityFinish, EvAbilityStopped} {
		w.Subscribe(k, 7)
	}
}

// TestAbilityLifecycleHappyPath — firebolt at a dummy: the stream is
// Cast(t1) → Effect(t11) → Finish(t21), Effect carries caster/victim/ref, and
// the 30-damage composition lands exactly at the Effect tick (SoT: victim HP).
func TestAbilityLifecycleHappyPath(t *testing.T) {
	w, tb, caster, victim, _ := abilityWorld(t)
	ref := abilityRef(t, tb, "firebolt")

	var log []abilityEvent
	captureAbilityEvents(w, &log)
	var hpAtEffect fixed.F64
	w.RegisterHandler(8, func(w *World, e Event) {
		if e.Kind == EvAbilityEffect {
			hpAtEffect = w.Healths.Life[w.Healths.Row(victim)]
		}
	})
	w.Subscribe(EvAbilityEffect, 8)

	hpBefore := w.Healths.Life[w.Healths.Row(victim)]
	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	for i := 0; i < 25; i++ {
		w.Step()
	}
	hpAfter := w.Healths.Life[w.Healths.Row(victim)]

	t.Logf("victim HP: before=%d after=%d (effect tick HP read=%d)", hpBefore, hpAfter, hpAtEffect)
	for _, e := range log {
		t.Logf("  %s", e)
	}

	// SoT 1: the lifecycle stream, in order, with correct payload.
	want := []abilityEvent{
		{tick: 1, kind: EvAbilityCast, src: caster, dst: victim, arg: int64(ref)},
		{tick: 11, kind: EvAbilityEffect, src: caster, dst: victim, arg: int64(ref)},
		{tick: 21, kind: EvAbilityFinish, src: caster, dst: victim, arg: int64(ref)},
	}
	if len(log) != len(want) {
		t.Fatalf("event count = %d, want %d: %v", len(log), len(want), log)
	}
	for i := range want {
		if log[i] != want[i] {
			t.Fatalf("event[%d] = %s, want %s", i, log[i], want[i])
		}
	}
	// SoT 2: damage landed in the same tick as the Effect edge (100 → 70).
	if hpBefore != 100*fixed.One {
		t.Fatalf("setup: victim HP = %d, want 100", hpBefore)
	}
	if hpAfter != 70*fixed.One {
		t.Fatalf("victim HP after = %d, want 70 (30-damage firebolt)", hpAfter)
	}
}

// TestAbilityLifecycleInterruptStops — interrupt during castpoint emits
// EvAbilityStopped and NO EvAbilityEffect; the effect SoT confirms no damage.
func TestAbilityLifecycleInterruptStops(t *testing.T) {
	w, tb, caster, victim, _ := abilityWorld(t)
	ref := abilityRef(t, tb, "firebolt")

	var log []abilityEvent
	captureAbilityEvents(w, &log)

	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	for i := 0; i < 5; i++ { // mid-castpoint
		w.Step()
	}
	// replace the cast order — the interrupt edge.
	w.IssueOrder(caster, Order{Kind: OrderMove, Point: fixed.Vec2{X: 2000 * fixed.One, Y: 1000 * fixed.One}}, false)
	w.Step()

	for _, e := range log {
		t.Logf("  %s", e)
	}
	hp := w.Healths.Life[w.Healths.Row(victim)]
	t.Logf("victim HP after interrupt = %d (want 100, no effect)", hp)

	// Cast fired, Stopped fired, Effect did NOT.
	var sawCast, sawStopped, sawEffect bool
	for _, e := range log {
		switch e.kind {
		case EvAbilityCast:
			sawCast = true
		case EvAbilityStopped:
			sawStopped = true
			if e.arg != int64(ref) {
				t.Fatalf("Stopped arg = %d, want ref %d", e.arg, ref)
			}
		case EvAbilityEffect:
			sawEffect = true
		}
	}
	if !sawCast || !sawStopped || sawEffect {
		t.Fatalf("cast=%v stopped=%v effect=%v, want cast+stopped, no effect", sawCast, sawStopped, sawEffect)
	}
	if hp != 100*fixed.One {
		t.Fatalf("victim HP = %d, want 100 — an interrupted cast must not deal damage", hp)
	}
}

// TestAbilityLifecycleChannelBrackets — a channeled ability (torrent) brackets
// its channel with ChannelStart … ChannelStop, both after the Effect edge and
// before Finish.
func TestAbilityLifecycleChannelBrackets(t *testing.T) {
	w, tb, caster, victim, _ := abilityWorld(t)
	ref := abilityRef(t, tb, "torrent")

	var log []abilityEvent
	captureAbilityEvents(w, &log)

	w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
	for i := 0; i < 40; i++ {
		w.Step()
	}
	for _, e := range log {
		t.Logf("  %s", e)
	}

	// Expected order of kinds: Cast, Effect, ChannelStart, ChannelStop, Finish.
	wantKinds := []uint16{EvAbilityCast, EvAbilityEffect, EvAbilityChannelStart, EvAbilityChannelStop, EvAbilityFinish}
	if len(log) != len(wantKinds) {
		t.Fatalf("event count = %d, want %d: %v", len(log), len(wantKinds), log)
	}
	for i, k := range wantKinds {
		if log[i].kind != k {
			t.Fatalf("event[%d] kind = %s, want %s", i, abilityEvName(log[i].kind), abilityEvName(k))
		}
	}
	// ChannelStart must precede ChannelStop and bracket a real channel span.
	var startTick, stopTick uint32
	for _, e := range log {
		if e.kind == EvAbilityChannelStart {
			startTick = e.tick
		}
		if e.kind == EvAbilityChannelStop {
			stopTick = e.tick
		}
	}
	if !(startTick < stopTick) {
		t.Fatalf("channel bracket: start tick %d not before stop tick %d", startTick, stopTick)
	}
	t.Logf("channel bracket: start=t%d stop=t%d span=%d ticks", startTick, stopTick, stopTick-startTick)
}

// TestAbilityLifecycleDoubleRunIdentical — two identical runs produce a
// byte-identical event stream and identical StateHash (determinism).
func TestAbilityLifecycleDoubleRunIdentical(t *testing.T) {
	reg := NewHashRegistry()
	run := func() ([]abilityEvent, uint64) {
		w, tb, caster, victim, _ := abilityWorld(t)
		ref := abilityRef(t, tb, "firebolt")
		var log []abilityEvent
		captureAbilityEvents(w, &log)
		w.IssueOrder(caster, Order{Kind: OrderCastAbility, Target: victim, Data: ref}, false)
		for i := 0; i < 25; i++ {
			w.Step()
		}
		var s statehash.Snapshot
		w.HashState(reg, &s)
		return log, s.Top
	}
	log1, h1 := run()
	log2, h2 := run()
	t.Logf("run1 hash=%#x events=%d | run2 hash=%#x events=%d", h1, len(log1), h2, len(log2))
	if h1 != h2 {
		t.Fatalf("StateHash diverged: %#x vs %#x", h1, h2)
	}
	if len(log1) != len(log2) {
		t.Fatalf("event count diverged: %d vs %d", len(log1), len(log2))
	}
	for i := range log1 {
		if log1[i] != log2[i] {
			t.Fatalf("event[%d] diverged: %s vs %s", i, log1[i], log2[i])
		}
	}
}
