package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// #617 — customevents hash section + save + load. SoT = the
// "customevents" sub-hash and the restored name↔kind mapping.

func TestCustomEventSaveRoundTrip(t *testing.T) {
	src := NewWorld(Caps{Units: 8, CustomEventKinds: 16})
	wave := src.CustomEvents.RegisterEventKind("wave")
	boss := src.CustomEvents.RegisterEventKind("boss")
	// also subscribe a handler to a custom kind — proves subscriptions ride
	// the subs tables and resolve to the restored kind.
	src.RegisterHandler(HandlerID(50), func(*World, Event) {})
	src.Subscribe(wave, HandlerID(50))

	reg := NewHashRegistry()
	var before statehash.Snapshot
	src.HashState(reg, &before)

	var buf bytes.Buffer
	if err := src.SaveState(&buf, 0); err != nil {
		t.Fatalf("SaveState: %v", err)
	}

	dst := NewWorld(Caps{Units: 8, CustomEventKinds: 16})
	dst.RegisterHandler(HandlerID(50), func(*World, Event) {}) // code re-registered before load
	if err := dst.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	var after statehash.Snapshot
	dst.HashState(reg, &after)

	ci := hashSystemIndex(t, "customevents")
	if before.Subs[ci] != after.Subs[ci] {
		t.Fatalf("customevents sub differs: %016x -> %016x", before.Subs[ci], after.Subs[ci])
	}
	if before.Top != after.Top {
		t.Fatalf("top differs; diverged: %v", snapDiff(t, &before, &after))
	}
	// SoT: name→kind mapping survived.
	if dst.CustomEvents.KindOf("wave") != wave || dst.CustomEvents.KindOf("boss") != boss {
		t.Fatalf("mapping lost: wave=%d boss=%d (want %d,%d)",
			dst.CustomEvents.KindOf("wave"), dst.CustomEvents.KindOf("boss"), wave, boss)
	}
	if n, _ := dst.CustomEvents.NameOf(wave); n != "wave" {
		t.Fatalf("NameOf(wave) = %q after load", n)
	}
	// SoT: the custom-kind subscription was restored.
	if !dst.IsSubscribed(wave, HandlerID(50)) {
		t.Fatal("custom-kind subscription not restored")
	}
}

func TestCustomEventHashDeterminismAndLocalization(t *testing.T) {
	mk := func() *World {
		w := NewWorld(Caps{Units: 8, CustomEventKinds: 16})
		w.CustomEvents.RegisterEventKind("a")
		w.CustomEvents.RegisterEventKind("b")
		return w
	}
	reg := NewHashRegistry()
	var a, b statehash.Snapshot
	mk().HashState(reg, &a)
	mk().HashState(reg, &b)
	if a.Top != b.Top {
		t.Fatalf("identical registrations diverged: %v", snapDiff(t, &a, &b))
	}
	w2 := mk()
	w2.CustomEvents.RegisterEventKind("c")
	var c statehash.Snapshot
	w2.HashState(reg, &c)
	ci := hashSystemIndex(t, "customevents")
	if a.Subs[ci] == c.Subs[ci] {
		t.Fatal("new registration did not move the customevents sub")
	}
	for i := range a.Subs {
		if i != ci && a.Subs[i] != c.Subs[i] {
			t.Fatalf("non-customevents sub %d changed — leak", i)
		}
	}
}
