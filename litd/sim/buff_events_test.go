package sim

// FSV for the buff-attach events (#469): buffs emitted only EvBuffExpired;
// they now also emit EvBuffApplied (new instance) and EvBuffRefreshed
// (refresh/restack), with aura children flagged in Arg. SoT = the captured
// event stream joined against the buff store (the instance must exist when the
// event says applied) and StateHash.

import (
	"fmt"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func buffEvName(k uint16) string {
	switch k {
	case EvBuffApplied:
		return "Applied"
	case EvBuffRefreshed:
		return "Refreshed"
	case EvBuffExpired:
		return "Expired"
	}
	return fmt.Sprintf("kind%d", k)
}

// captureBuffEvents records Applied/Refreshed/Expired in dispatch order.
func captureBuffEvents(w *World, log *[]abilityEvent) {
	w.RegisterHandler(6, func(w *World, e Event) {
		switch e.Kind {
		case EvBuffApplied, EvBuffRefreshed, EvBuffExpired:
			*log = append(*log, abilityEvent{tick: w.Tick(), kind: e.Kind, src: e.Src, dst: e.Dst, arg: e.Arg})
		}
	})
	for _, k := range []uint16{EvBuffApplied, EvBuffRefreshed, EvBuffExpired} {
		w.Subscribe(k, 6)
	}
}

func logBuffEvents(t *testing.T, log []abilityEvent) {
	t.Helper()
	for _, e := range log {
		t.Logf("  t%d %s src=%d dst=%d arg=%d {id=%d stacks=%d aura=%v}",
			e.tick, buffEvName(e.kind), e.src, e.dst, e.arg,
			BuffArgID(e.arg), BuffArgStacks(e.arg), BuffArgIsAura(e.arg))
	}
}

// TestBuffAppliedEmitsAndStored — applying a buff emits EvBuffApplied AND the
// instance is actually present in the store with the right source/stacks (the
// event's claim verified against the SoT, not the return value).
func TestBuffAppliedEmitsAndStored(t *testing.T) {
	w, tb, carrier, _ := buffWorld(t)
	slow := buffTypeIdx(t, tb, "slow")

	var log []abilityEvent
	captureBuffEvents(w, &log)

	ok := w.ApplyBuff(carrier, carrier, slow, 1)
	w.Step() // flush the queued event
	if !ok {
		t.Fatal("ApplyBuff returned false")
	}
	logBuffEvents(t, log)
	t.Logf("store: %v", dumpInstances(w, carrier))

	// SoT 1: the event fired with the right payload.
	if len(log) != 1 || log[0].kind != EvBuffApplied {
		t.Fatalf("events = %v, want one EvBuffApplied", log)
	}
	e := log[0]
	if e.src != carrier || e.dst != carrier {
		t.Fatalf("Applied src/dst = %d/%d, want %d/%d", e.src, e.dst, carrier, carrier)
	}
	if BuffArgID(e.arg) != uint16(slow) || BuffArgStacks(e.arg) != 1 || BuffArgIsAura(e.arg) {
		t.Fatalf("Arg unpack = id%d stacks%d aura%v, want id%d stacks1 aurafalse",
			BuffArgID(e.arg), BuffArgStacks(e.arg), BuffArgIsAura(e.arg), slow)
	}
	// SoT 2: the instance is really in the store.
	if !w.UnitHasBuff(carrier, uint16(slow)) {
		t.Fatal("EvBuffApplied fired but the instance is absent from the store")
	}
	if got := w.BuffStacks(carrier, uint16(slow)); got != 1 {
		t.Fatalf("stored stacks = %d, want 1", got)
	}
}

// TestBuffRefreshEmits — re-applying a refresh buff emits EvBuffRefreshed and
// resets RemainingTicks (SoT: the stored remaining duration).
func TestBuffRefreshEmits(t *testing.T) {
	w, tb, carrier, _ := buffWorld(t)
	slow := buffTypeIdx(t, tb, "slow")

	var log []abilityEvent
	captureBuffEvents(w, &log)

	w.ApplyBuff(carrier, carrier, slow, 1)
	for i := 0; i < 5; i++ {
		w.Step()
	}
	remBefore := w.BuffRemainingTicks(carrier, uint16(slow))
	w.ApplyBuff(carrier, carrier, slow, 1) // refresh
	w.Step()
	remAfter := w.BuffRemainingTicks(carrier, uint16(slow))
	logBuffEvents(t, log)
	t.Logf("remaining: before refresh=%d after=%d", remBefore, remAfter)

	var refreshed *abilityEvent
	for i := range log {
		if log[i].kind == EvBuffRefreshed {
			refreshed = &log[i]
		}
	}
	if refreshed == nil {
		t.Fatalf("no EvBuffRefreshed in %v", log)
	}
	if remAfter <= remBefore {
		t.Fatalf("RemainingTicks not reset: before=%d after=%d", remBefore, remAfter)
	}
}

// TestBuffStackEmitsNewStacks — a stacking (count) buff: first apply →
// EvBuffApplied(1), second → EvBuffRefreshed carrying the new stack count.
func TestBuffStackEmitsNewStacks(t *testing.T) {
	w, tb, carrier, _ := buffWorld(t)
	poison := buffTypeIdx(t, tb, "poison")

	var log []abilityEvent
	captureBuffEvents(w, &log)

	w.ApplyBuff(carrier, carrier, poison, 1)
	w.ApplyBuff(carrier, carrier, poison, 2) // 1 → 3
	w.Step()
	logBuffEvents(t, log)
	t.Logf("store: %v", dumpInstances(w, carrier))

	if len(log) < 2 {
		t.Fatalf("want Applied then Refreshed, got %v", log)
	}
	if log[0].kind != EvBuffApplied || BuffArgStacks(log[0].arg) != 1 {
		t.Fatalf("event[0] = %s stacks=%d, want Applied stacks1", buffEvName(log[0].kind), BuffArgStacks(log[0].arg))
	}
	if log[1].kind != EvBuffRefreshed || BuffArgStacks(log[1].arg) != 3 {
		t.Fatalf("event[1] = %s stacks=%d, want Refreshed stacks3", buffEvName(log[1].kind), BuffArgStacks(log[1].arg))
	}
	if got := w.BuffStacks(carrier, uint16(poison)); got != 3 {
		t.Fatalf("stored stacks = %d, want 3", got)
	}
}

// TestBuffAuraChildEmitsWithFlag — an ally inside aura radius gains the child
// via applyAuraChild, which emits EvBuffApplied with the aura flag set.
func TestBuffAuraChildEmitsWithFlag(t *testing.T) {
	w, tb := auraWorld(t)
	cmd := buffTypeIdx(t, tb, "command")
	cmdChild := buffTypeIdx(t, tb, "cmd-child")
	src := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	ally := atkUnit(t, w, 0, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}, 0) // in radius

	var log []abilityEvent
	captureBuffEvents(w, &log)

	w.ApplyBuff(src, src, cmd, 1)
	for i := 0; i < 20; i++ { // aura eval is throttled by (tick+targetIndex)%acquireEvery
		w.Step()
	}
	logBuffEvents(t, log)
	t.Logf("ally children: %s", childDump(w, ally))

	// the aura carrier got EvBuffApplied for "command" (non-aura); the ally got
	// EvBuffApplied for "cmd-child" WITH the aura flag.
	var sawAuraChild bool
	for _, e := range log {
		if e.kind == EvBuffApplied && e.dst == ally && BuffArgID(e.arg) == uint16(cmdChild) {
			if !BuffArgIsAura(e.arg) {
				t.Fatalf("aura-child Applied missing the aura flag: arg=%d", e.arg)
			}
			sawAuraChild = true
		}
	}
	if !sawAuraChild {
		t.Fatalf("no aura-child EvBuffApplied for the ally in %v", log)
	}
	// SoT: the child instance exists on the ally.
	if childCount(w, ally) == 0 {
		t.Fatal("event fired but the ally has no aura-child instance")
	}
}

// TestBuffExpiryCarriesAuraFlagAndStacks — #488: EvBuffExpired now packs the full
// buff arg (id + stacks + aura-child flag), not just the type id. An aura child
// that lingers out of radius and expires carries IsAura=true and its stack count,
// so an OnBuffExpired handler can tell an expiring aura child from a direct buff.
func TestBuffExpiryCarriesAuraFlagAndStacks(t *testing.T) {
	w, tb := auraWorld(t)
	cmd := buffTypeIdx(t, tb, "command")
	src := atkUnit(t, w, 0, fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One}, 0)
	ally := atkUnit(t, w, 0, fixed.Vec2{X: 1100 * fixed.One, Y: 1000 * fixed.One}, 0) // in radius
	w.ApplyBuff(src, src, cmd, 1)

	var log []abilityEvent
	captureBuffEvents(w, &log)

	gained := uint32(0)
	tr := w.Transforms.Row(ally)
	for w.Tick() < 60 {
		w.Step()
		if gained == 0 && childCount(w, ally) == 1 {
			gained = w.Tick()
		}
		if gained != 0 && w.Tick() == gained+7 { // walk out → child lingers then expires
			w.Transforms.Pos[tr] = fixed.Vec2{X: 8000 * fixed.One, Y: 8000 * fixed.One}
		}
	}

	var expiry *abilityEvent
	for i := range log {
		if log[i].kind == EvBuffExpired && log[i].src == ally {
			expiry = &log[i]
		}
	}
	if expiry == nil {
		t.Fatalf("no EvBuffExpired for the ally aura child in %v", log)
	}
	// SoT: the expiry arg decodes to the child type, aura-flagged, 1 stack.
	if !BuffArgIsAura(expiry.arg) {
		t.Fatalf("aura-child expiry missing the aura flag (#488): arg=%d IsAura=%v", expiry.arg, BuffArgIsAura(expiry.arg))
	}
	if BuffArgStacks(expiry.arg) != 1 {
		t.Fatalf("aura-child expiry stacks=%d, want 1", BuffArgStacks(expiry.arg))
	}
	if got := buffTypeIdx(t, tb, "cmd-child"); BuffArgID(expiry.arg) != uint16(got) {
		t.Fatalf("expiry buff id=%d, want cmd-child=%d", BuffArgID(expiry.arg), got)
	}
	t.Logf("#488 aura-child expiry arg: id=%d stacks=%d aura=%v",
		BuffArgID(expiry.arg), BuffArgStacks(expiry.arg), BuffArgIsAura(expiry.arg))
}

// TestBuffEventsDoubleRunIdentical — two identical apply sequences produce a
// byte-identical event stream and identical StateHash.
func TestBuffEventsDoubleRunIdentical(t *testing.T) {
	reg := NewHashRegistry()
	run := func() ([]abilityEvent, uint64) {
		w, tb, carrier, _ := buffWorld(t)
		poison := buffTypeIdx(t, tb, "poison")
		var log []abilityEvent
		captureBuffEvents(w, &log)
		w.ApplyBuff(carrier, carrier, poison, 1)
		w.ApplyBuff(carrier, carrier, poison, 2)
		for i := 0; i < 5; i++ {
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
			t.Fatalf("event[%d] diverged: %+v vs %+v", i, log1[i], log2[i])
		}
	}
}
