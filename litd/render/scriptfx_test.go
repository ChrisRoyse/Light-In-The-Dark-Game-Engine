package render

// #351 script-effect pool FSV. SoT = the pool's slot counts
// (active/persistent/one-shot/light) and per-slot state via SnapshotInto, across
// Spawn(persistent)→Update→End and OneShot→Tick→evict. The #309 priority rule is
// the spotlight: one-shots evict oldest-first, persistents are NEVER evicted, and
// a one-shot is denied when only persistents remain. Plus follow-position from
// the snapshot, fail-closed edges, and zero steady-state alloc (R-GC-2). Headless.

import (
	"testing"

	"github.com/g3n/engine/math32"
)

func fxDesc(model uint16, light bool) ScriptFXDesc {
	return ScriptFXDesc{Model: model, Scale: 1.5, Color: math32.Color{R: 0.6, G: 0.4, B: 1}, HasLight: light}
}

func fxByHandle(p *ScriptFXPool, h uint64) (ScriptFXSlotInfo, bool) {
	for _, s := range p.SnapshotInto(make([]ScriptFXSlotInfo, 0, MaxScriptFX)) {
		if s.Active && s.Handle == h {
			return s, true
		}
	}
	return ScriptFXSlotInfo{}, false
}

func TestScriptFXLifecycleFSV(t *testing.T) {
	p := NewScriptFXPool()

	// Three persistent effects (entity keys 10,11,12) + two one-shots.
	for _, k := range []uint32{10, 11, 12} {
		if _, d := p.Spawn(k, fxDesc(uint16(k), k == 11)); !d.Granted {
			t.Fatalf("spawn persistent %d refused: %+v", k, d)
		}
	}
	p.OneShot(math32.Vector3{X: 5}, fxDesc(99, false), 3)
	p.OneShot(math32.Vector3{X: 6}, fxDesc(98, true), 5)

	t.Logf("FSV counts: active=%d persistent=%d oneshot=%d light=%d",
		p.ActiveCount(), p.PersistentCount(), p.OneShotCount(), p.LightCount())
	if p.ActiveCount() != 5 || p.PersistentCount() != 3 || p.OneShotCount() != 2 {
		t.Fatalf("counts active=%d persistent=%d oneshot=%d, want 5/3/2", p.ActiveCount(), p.PersistentCount(), p.OneShotCount())
	}
	// LightCount: key 11 persistent + the second one-shot = 2.
	if p.LightCount() != 2 {
		t.Fatalf("light count = %d, want 2", p.LightCount())
	}

	// Persistents resolve position from the snapshot by key; one of them despawns.
	pos := map[uint32]math32.Vector3{10: {X: 100, Z: 200}, 12: {X: -50, Z: 0}} // 11 missing
	p.Update(func(k uint32) (math32.Vector3, bool) { v, ok := pos[k]; return v, ok })
	for _, s := range p.SnapshotInto(make([]ScriptFXSlotInfo, 0, MaxScriptFX)) {
		if !s.Active || !s.Persistent {
			continue
		}
		switch s.Key {
		case 10:
			if !s.Visible || s.Pos.X != 100 || s.Pos.Z != 200 {
				t.Fatalf("effect 10 pos=%+v visible=%v, want (100,_,200)/true", s.Pos, s.Visible)
			}
		case 11:
			if s.Visible {
				t.Fatal("effect 11 visible despite missing from snapshot")
			}
		}
	}
	t.Logf("FSV follow: effect 10 at snapshot pos, effect 11 (despawned) invisible but still active")

	// X+X=Y: the lifetime-3 one-shot expires on exactly the 3rd Tick; persistents
	// untouched.
	p.Tick()
	p.Tick()
	if rel := p.Tick(); rel != 1 {
		t.Fatalf("3rd tick released=%d, want 1 (lifetime-3 one-shot)", rel)
	}
	if p.PersistentCount() != 3 {
		t.Fatalf("persistent count changed under Tick: %d, want 3", p.PersistentCount())
	}
	if p.OneShotCount() != 1 {
		t.Fatalf("one-shot count = %d after expiry, want 1", p.OneShotCount())
	}

	// End a persistent (RenderEffectEnd) → freed; End of an unknown key → false.
	if !p.End(11) || p.PersistentCount() != 2 {
		t.Fatalf("End(11) failed: persistent=%d", p.PersistentCount())
	}
	if p.End(11) {
		t.Fatal("End(11) twice returned true")
	}
}

func TestScriptFXPriorityRuleFSV(t *testing.T) {
	p := NewScriptFXPool()
	// Fill the pool with persistents except for a few one-shot slots.
	const persistents = MaxScriptFX - 3
	for k := uint32(1); k <= persistents; k++ {
		if _, d := p.Spawn(k, fxDesc(1, false)); !d.Granted {
			t.Fatalf("spawn persistent %d refused: %+v", k, d)
		}
	}
	// Three one-shots fill the last slots; remember the oldest's handle.
	hOldest, _ := p.OneShot(math32.Vector3{}, fxDesc(2, false), 100)
	p.OneShot(math32.Vector3{}, fxDesc(2, false), 100)
	p.OneShot(math32.Vector3{}, fxDesc(2, false), 100)
	if p.ActiveCount() != MaxScriptFX {
		t.Fatalf("pool not full: active=%d", p.ActiveCount())
	}

	// A new one-shot into the full pool evicts the OLDEST one-shot, never a
	// persistent. Persistent count must stay put.
	_, d := p.OneShot(math32.Vector3{}, fxDesc(3, false), 100)
	t.Logf("FSV evict: decision=%+v persistent=%d oneshot=%d", d, p.PersistentCount(), p.OneShotCount())
	if !d.Granted || d.Reason != "evict:oldest-oneshot" || d.Victim < 0 {
		t.Fatalf("full-pool one-shot should evict oldest one-shot: %+v", d)
	}
	if p.PersistentCount() != persistents {
		t.Fatalf("a persistent was evicted! count=%d, want %d", p.PersistentCount(), persistents)
	}
	if _, ok := fxByHandle(p, hOldest); ok {
		t.Fatal("the oldest one-shot was not the one evicted")
	}

	// Now evict the remaining one-shots by spawning persistents, until the pool is
	// ALL persistents; then a one-shot must be DENIED (never evict a persistent).
	for k := uint32(1000); p.OneShotCount() > 0; k++ {
		p.Spawn(k, fxDesc(1, false)) // each new persistent evicts an oldest one-shot
	}
	if p.PersistentCount() != MaxScriptFX {
		t.Fatalf("pool not all-persistent: persistent=%d", p.PersistentCount())
	}
	_, d2 := p.OneShot(math32.Vector3{}, fxDesc(4, false), 100)
	t.Logf("FSV deny: all-persistent pool, one-shot decision=%+v", d2)
	if d2.Granted || d2.Reason != "refused:all-persistent" {
		t.Fatalf("one-shot into all-persistent pool should be denied: %+v", d2)
	}
	if p.PersistentCount() != MaxScriptFX {
		t.Fatalf("denied one-shot still disturbed persistents: %d", p.PersistentCount())
	}
}

func TestScriptFXRefuseEdgesFSV(t *testing.T) {
	p := NewScriptFXPool()
	if _, d := p.Spawn(0, fxDesc(1, false)); d.Granted || d.Reason != "refused:zero-key" {
		t.Fatalf("zero-key spawn accepted: %+v", d)
	}
	if _, d := p.OneShot(math32.Vector3{}, fxDesc(1, false), 0); d.Granted {
		t.Fatalf("zero-lifetime one-shot accepted: %+v", d)
	}
	bad := fxDesc(1, false)
	bad.Scale = 0
	if _, d := p.OneShot(math32.Vector3{}, bad, 5); d.Granted {
		t.Fatalf("zero-scale one-shot accepted: %+v", d)
	}
	if p.ActiveCount() != 0 {
		t.Fatalf("refused requests bound a slot: active=%d", p.ActiveCount())
	}
	// Re-spawn of a live key updates in place (idempotent), no new slot.
	p.Spawn(7, fxDesc(1, false))
	_, d := p.Spawn(7, fxDesc(42, true))
	if d.Reason != "update" || p.ActiveCount() != 1 {
		t.Fatalf("re-spawn should update in place: %+v active=%d", d, p.ActiveCount())
	}
	for _, si := range p.SnapshotInto(make([]ScriptFXSlotInfo, 0, MaxScriptFX)) {
		if si.Active && si.Key == 7 && si.Model != 42 {
			t.Fatalf("re-spawn did not update model: %d", si.Model)
		}
	}
}

func TestScriptFXZeroAllocFSV(t *testing.T) {
	p := NewScriptFXPool()
	dst := make([]ScriptFXSlotInfo, 0, MaxScriptFX)
	for k := uint32(1); k <= 20; k++ {
		p.Spawn(k, fxDesc(1, k%2 == 0))
	}
	for i := 0; i < 10; i++ {
		p.OneShot(math32.Vector3{X: float32(i)}, fxDesc(2, false), 50)
	}
	lookup := func(k uint32) (math32.Vector3, bool) { return math32.Vector3{X: 1, Y: 2, Z: 3}, true }
	allocs := testing.AllocsPerRun(200, func() {
		p.Update(lookup)
		p.Tick()
		dst = p.SnapshotInto(dst)
	})
	t.Logf("FSV zero-alloc: allocs/op=%.2f over Update+Tick+SnapshotInto", allocs)
	if allocs != 0 {
		t.Fatalf("scriptfx pool allocated %.2f/op, want 0 (R-GC-2)", allocs)
	}
}
