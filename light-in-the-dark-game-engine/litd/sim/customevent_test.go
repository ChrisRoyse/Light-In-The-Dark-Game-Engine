package sim

import "testing"

// #614 — CustomEventRegistry. SoT = the assigned kind ids + the
// name↔kind mapping read back from the registry.

func TestCustomEventRegisterIdempotent(t *testing.T) {
	r := NewCustomEventRegistry(8)
	// First custom kind is KBuiltinMax+1.
	wave := r.RegisterEventKind("wave")
	if wave != KBuiltinMax+1 {
		t.Fatalf("first kind = %d, want %d", wave, KBuiltinMax+1)
	}
	boss := r.RegisterEventKind("boss")
	if boss != KBuiltinMax+2 {
		t.Fatalf("second kind = %d, want %d", boss, KBuiltinMax+2)
	}
	// Idempotent: re-register returns the SAME id, no new kind.
	if again := r.RegisterEventKind("wave"); again != wave {
		t.Fatalf("re-register wave = %d, want %d (idempotent)", again, wave)
	}
	if r.Count() != 2 {
		t.Fatalf("count = %d, want 2", r.Count())
	}
}

func TestCustomEventNameKindMapping(t *testing.T) {
	r := NewCustomEventRegistry(8)
	k := r.RegisterEventKind("spawn")
	// SoT round-trip: name→kind→name.
	if r.KindOf("spawn") != k {
		t.Fatalf("KindOf(spawn) = %d, want %d", r.KindOf("spawn"), k)
	}
	if n, ok := r.NameOf(k); !ok || n != "spawn" {
		t.Fatalf("NameOf(%d) = %q,%v, want spawn", k, n, ok)
	}
	// Unregistered name → 0; built-in/oob kind → not a name.
	if r.KindOf("never") != 0 {
		t.Fatal("KindOf(unregistered) != 0")
	}
	if _, ok := r.NameOf(EvUnitDeath); ok {
		t.Fatal("NameOf(built-in kind) resolved")
	}
	if !r.IsCustomKind(k) || r.IsCustomKind(EvUnitDeath) || r.IsCustomKind(k+1) {
		t.Fatal("IsCustomKind wrong")
	}
}

func TestCustomEventExhaustion(t *testing.T) {
	const cap = 3
	r := NewCustomEventRegistry(cap)
	for i := 0; i < cap; i++ {
		if r.RegisterEventKind(string(rune('a'+i))) == 0 {
			t.Fatalf("register %d failed below cap", i)
		}
	}
	// Over cap: new name → 0 + Dropped++.
	if r.RegisterEventKind("overflow") != 0 {
		t.Fatal("register past cap returned a kind")
	}
	if r.Dropped != 1 {
		t.Fatalf("Dropped = %d, want 1", r.Dropped)
	}
	// But an already-registered name still resolves at full cap (idempotent).
	if r.RegisterEventKind("a") == 0 {
		t.Fatal("idempotent re-register failed at full cap")
	}
}

func TestNewWorldWiresCustomEvents(t *testing.T) {
	w := NewWorld(Caps{})
	if w.CustomEvents == nil || w.CustomEvents.Cap() != uint16(EngineCaps.CustomEventKinds) {
		t.Fatalf("CustomEvents not wired: %v", w.CustomEvents)
	}
}
