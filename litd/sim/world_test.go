package sim

import (
	"testing"
	"unsafe"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// pointers captures the backing-array addresses of every World pool.
func worldPointers(w *World) map[string]uintptr {
	return map[string]uintptr{
		"ents.slots":  uintptr(unsafe.Pointer(unsafe.SliceData(w.Ents.slots))),
		"trans.pos":   uintptr(unsafe.Pointer(unsafe.SliceData(w.Transforms.Pos))),
		"trans.rowOf": uintptr(unsafe.Pointer(unsafe.SliceData(w.Transforms.rowOf))),
		"projectiles": uintptr(unsafe.Pointer(unsafe.SliceData(w.projectiles))),
		"buffs":       uintptr(unsafe.Pointer(unsafe.SliceData(w.buffs))),
		"orderPool":   uintptr(unsafe.Pointer(unsafe.SliceData(w.orderPool))),
		"events":      uintptr(unsafe.Pointer(unsafe.SliceData(w.events))),
		"pathReqs":    uintptr(unsafe.Pointer(unsafe.SliceData(w.pathReqs))),
		"doodads":     uintptr(unsafe.Pointer(unsafe.SliceData(w.doodads))),
	}
}

// Backing arrays must be byte-stable across heavy churn: capture
// SliceData pointers, churn 10k create/destroy, compare.
func TestWorldCapacityPointerStability(t *testing.T) {
	w := NewWorld(Caps{})
	before := worldPointers(w)
	for k, v := range before {
		t.Logf("before churn: %-12s %#x", k, v)
	}

	// churn: fill to cap, drain to 0, repeat until 10k+ ops
	ops := 0
	ids := make([]EntityID, 0, w.Caps().Units)
	for ops < 10_000 {
		ids = ids[:0]
		for {
			id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(int32(ops))}, 0)
			if !ok {
				break
			}
			ids = append(ids, id)
			ops++
		}
		if w.UnitCount() != w.Caps().Units {
			t.Fatalf("fill stopped at %d, cap %d", w.UnitCount(), w.Caps().Units)
		}
		for _, id := range ids {
			w.DestroyUnit(id)
			ops++
		}
		if w.UnitCount() != 0 {
			t.Fatalf("drain left %d units", w.UnitCount())
		}
	}

	after := worldPointers(w)
	for k, v := range after {
		t.Logf("after %d ops:  %-12s %#x", ops, k, v)
	}
	for k := range before {
		if before[k] != after[k] {
			t.Fatalf("pool %s reallocated: %#x -> %#x", k, before[k], after[k])
		}
	}
	t.Logf("all %d backing arrays byte-stable across %d create/destroy ops", len(before), ops)
}

// Edge: unit 4,001 fails as a gameplay outcome; count stays 4,000.
func TestWorldUnitCapExhaustion(t *testing.T) {
	w := NewWorld(Caps{})
	for i := 0; i < 4000; i++ {
		if _, ok := w.CreateUnit(fixed.Vec2{}, 0); !ok {
			t.Fatalf("create %d failed below cap", i)
		}
	}
	id, ok := w.CreateUnit(fixed.Vec2{}, 0)
	t.Logf("unit 4001: ok=%v id=%d; UnitCount=%d (cap %d)", ok, id, w.UnitCount(), w.Caps().Units)
	if ok || w.UnitCount() != 4000 {
		t.Fatalf("cap not enforced: ok=%v count=%d", ok, w.UnitCount())
	}
}

// Edge: map header lowers caps — creation 101 fails at 100, not 4,000.
func TestWorldMapHeaderLowersCaps(t *testing.T) {
	w := NewWorld(Caps{Units: 100})
	t.Logf("requested Units=100 -> effective %d (engine ceiling %d)", w.Caps().Units, EngineCaps.Units)
	if w.Caps().Units != 100 {
		t.Fatalf("effective cap %d, want 100", w.Caps().Units)
	}
	n := 0
	for {
		if _, ok := w.CreateUnit(fixed.Vec2{}, 0); !ok {
			break
		}
		n++
	}
	t.Logf("creation stopped at %d units", n)
	if n != 100 {
		t.Fatalf("stopped at %d, want 100", n)
	}
}

// Edge: a map header may not RAISE caps past the engine ceiling.
func TestWorldCapsCannotExceedCeiling(t *testing.T) {
	w := NewWorld(Caps{Units: 999_999, Projectiles: 999_999})
	t.Logf("requested Units=999999 -> effective %d; Projectiles=999999 -> %d",
		w.Caps().Units, w.Caps().Projectiles)
	if w.Caps().Units != EngineCaps.Units || w.Caps().Projectiles != EngineCaps.Projectiles {
		t.Fatalf("ceiling not enforced: %+v", w.Caps())
	}
}

// ecs §5.1: total preallocated sim state is single-digit megabytes.
func TestWorldPreallocatedBytesEnvelope(t *testing.T) {
	w := NewWorld(Caps{})
	bytes := w.PreallocatedBytes()
	t.Logf("preallocated pools at default caps: %d bytes (%.2f MB)", bytes, float64(bytes)/(1<<20))
	if bytes >= 10<<20 {
		t.Fatalf("pools take %d bytes — ecs §5.1 promises single-digit MB", bytes)
	}
	if bytes < 100<<10 {
		t.Fatalf("pools take only %d bytes — suspiciously small, check the math", bytes)
	}
}

func TestWorldCreateDestroyZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{Units: 1000})
	if n := testing.AllocsPerRun(5000, func() {
		id, _ := w.CreateUnit(fixed.Vec2{X: fixed.One}, 0)
		w.DestroyUnit(id)
	}); n != 0 {
		t.Fatalf("CreateUnit/DestroyUnit allocates %v/op; R-GC-1 requires 0", n)
	}
	t.Log("AllocsPerRun = 0 for CreateUnit+DestroyUnit")
}
