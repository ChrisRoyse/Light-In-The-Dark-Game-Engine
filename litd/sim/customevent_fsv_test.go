package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// #581 — custom-event acceptance suite: ordering, idempotent register,
// FSM-via-events determinism, save/resume parity, zero-alloc. Per-feature
// behavior lives in customevent_test.go / _dispatch_test.go / _save_test.go.

const cePulseHandler = HandlerID(300)

// ceRegisterConts re-registers the handler (code, not state) — the setup a
// loaded world performs before LoadState.
func ceRegisterConts(w *World) uint16 {
	k := w.CustomEvents.RegisterEventKind("pulse")
	w.RegisterHandler(cePulseHandler, func(w *World, e Event) {
		// FSM: phase = (phase+1) % 3, persisted in the KV global scope —
		// a custom event drives a deterministic state-machine transition.
		o := GlobalKVOwner()
		key := w.KV.InternKey("phase")
		_, v, _, _ := w.KV.KVGet(o, key)
		w.KV.KVSet(o, key, KVInt, (v+1)%3, 0)
	})
	w.Subscribe(k, cePulseHandler)
	return k
}

// cePulse emits one pulse and steps a tick (dispatch).
func cePulse(w *World, k uint16, n int) {
	for i := 0; i < n; i++ {
		w.Emit(Event{Kind: k})
		w.Step()
	}
}

func ceTopHash(w *World) uint64 {
	var s statehash.Snapshot
	w.HashState(NewHashRegistry(), &s)
	return s.Top
}

func TestCustomEventScenarioGolden(t *testing.T) {
	w := NewWorld(Caps{Units: 8, KVPairs: 64, CustomEventKinds: 8})
	k := ceRegisterConts(w)
	cePulse(w, k, 7) // 7 transitions → phase = 7 % 3 = 1
	// SoT cross-check: the FSM landed on phase 1.
	_, phase, _, _ := w.KV.KVGet(GlobalKVOwner(), w.KV.KeyID("phase"))
	if phase != 1 {
		t.Fatalf("FSM phase = %d after 7 pulses, want 1", phase)
	}
	const golden = uint64(0xd0db146402061510) // recorded 2026-06-24 (#581); rebumped #590 movers sub
	if got := ceTopHash(w); golden != 0 && got != golden {
		t.Fatalf("ce golden hash %016x != recorded %016x", got, golden)
	}
	t.Logf("ce scenario golden = %#016x (phase=%d)", ceTopHash(w), phase)
}

func TestCustomEventTwoRunDeterminism(t *testing.T) {
	run := func() uint64 {
		w := NewWorld(Caps{Units: 8, KVPairs: 64, CustomEventKinds: 8})
		cePulse(w, ceRegisterConts(w), 7)
		return ceTopHash(w)
	}
	if a, b := run(), run(); a != b {
		t.Fatalf("ce scenario diverged: %016x != %016x", a, b)
	}
}

func TestCustomEventSaveResumeParity(t *testing.T) {
	caps := Caps{Units: 8, KVPairs: 64, CustomEventKinds: 8}

	// Unbroken: 7 pulses.
	wu := NewWorld(caps)
	cePulse(wu, ceRegisterConts(wu), 7)
	want := ceTopHash(wu)

	// Save after 3 pulses, resume in a fresh world (re-register handler =
	// code), finish 4 more.
	ws := NewWorld(caps)
	ks := ceRegisterConts(ws)
	cePulse(ws, ks, 3)
	var buf bytes.Buffer
	if err := ws.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	wl := NewWorld(caps)
	ceRegisterConts(wl) // re-bind handler + re-register kind before load
	if err := wl.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	// The restored kind id resolves by name.
	kl := wl.CustomEvents.KindOf("pulse")
	cePulse(wl, kl, 4)
	if got := ceTopHash(wl); got != want {
		t.Fatalf("ce save/resume hash %016x != unbroken %016x", got, want)
	}
	t.Logf("FSV #581: custom-event save/resume parity holds; hash %#016x", want)
}

func TestCustomEventEmitZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{Units: 8, CustomEventKinds: 8})
	k := w.CustomEvents.RegisterEventKind("z")
	avg := testing.AllocsPerRun(1000, func() {
		w.Emit(Event{Kind: k, Arg: 1})
		w.eventCount = 0 // drain the ring without a full Step (measure Emit only)
	})
	if avg != 0 {
		t.Fatalf("Emit allocated %.2f objs/op, want 0", avg)
	}
}

func TestCustomEventRegisterIdempotentInSuite(t *testing.T) {
	w := NewWorld(Caps{CustomEventKinds: 8})
	a := w.CustomEvents.RegisterEventKind("x")
	b := w.CustomEvents.RegisterEventKind("x")
	if a != b || a == 0 {
		t.Fatalf("idempotent register broken: %d vs %d", a, b)
	}
}
