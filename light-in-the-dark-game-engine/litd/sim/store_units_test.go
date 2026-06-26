package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Edge 1: Movement requires Transform — adding to an entity without
// one fails closed and fires the debug assert.
func TestStoreMovementRequiresTransform(t *testing.T) {
	e := NewEntities(16)
	tr := NewTransformStore(16, 16)
	mv := NewMovementStore(16, 16)
	var asserts []string
	mv.DebugAssert = func(msg string, id EntityID) { asserts = append(asserts, msg) }

	bare, _ := e.Create() // no Transform
	if mv.Add(e, tr, bare, fixed.One, 1024) {
		t.Fatalf("Movement add without Transform must fail")
	}
	t.Logf("assert fired: %q (add returned false, store count %d)", asserts, mv.Count())
	if len(asserts) != 1 || asserts[0] != "Movement requires Transform" {
		t.Fatalf("expected the requires-Transform assert, got %v", asserts)
	}

	good, _ := e.Create()
	tr.Add(e, good, fixed.Vec2{}, 0)
	if !mv.Add(e, tr, good, fixed.One, 1024) {
		t.Fatalf("Movement add with Transform must succeed")
	}
}

// Edge 2: swap-remove in each store keeps every cross-store rowOf
// probe intact.
func TestStoreSwapRemoveProbesIntact(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	var ids []EntityID
	for i := 0; i < 4; i++ {
		id, _ := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(int32(i))}, 0)
		w.Movements.Add(w.Ents, w.Transforms, id, fixed.FromInt(int32(i+1)), 100)
		w.Collisions.Add(w.Ents, id, uint8(i), PathGround)
		w.Healths.Add(w.Ents, id, fixed.FromInt(int32(100+i)), 0, int16(i), 0)
		w.Owners.Add(w.Ents, id, uint8(i), uint8(i%2), uint8(i))
		w.UnitTypes.Add(w.Ents, id, uint16(1000+i))
		ids = append(ids, id)
	}
	// remove the FIRST entity from every store: last row swaps into row 0
	w.Movements.Remove(ids[0])
	w.Collisions.Remove(ids[0])
	w.Healths.Remove(ids[0])
	w.Owners.Remove(ids[0])
	w.UnitTypes.Remove(ids[0])

	for i, id := range ids[1:] {
		t.Logf("entity %d rows after swap-remove: mv=%d col=%d hp=%d own=%d ut=%d",
			i+1, w.Movements.Row(id), w.Collisions.Row(id), w.Healths.Row(id), w.Owners.Row(id), w.UnitTypes.Row(id))
	}
	// the swapped-in entity (ids[3]) must read back its own values
	r := w.Movements.Row(ids[3])
	if w.Movements.Speed[r] != fixed.FromInt(4) || w.Movements.Entity[r] != ids[3] {
		t.Fatalf("moved Movement row corrupted: speed=%v entity=%v", w.Movements.Speed[r], w.Movements.Entity[r])
	}
	if r2 := w.Healths.Row(ids[3]); w.Healths.MaxLife[r2] != fixed.FromInt(103) {
		t.Fatalf("moved Health row corrupted")
	}
	if r3 := w.UnitTypes.Row(ids[3]); w.UnitTypes.TypeID[r3] != 1003 {
		t.Fatalf("moved UnitType row corrupted")
	}
	if w.Movements.Row(ids[0]) != -1 || w.Healths.Row(ids[0]) != -1 {
		t.Fatalf("removed entity still has rows")
	}
}

// Edge 3: regen as a per-tick fixed increment — the data-table value
// 0.25 life/s becomes 0.0125 life/tick (20 ticks/s) in 32.32.
func TestStoreHealthRegenPerTickFixed(t *testing.T) {
	e := NewEntities(4)
	hp := NewHealthStore(4, 4)
	id, _ := e.Create()

	// 0.25/s ÷ 20 ticks/s, all in fixed point: One/4/20
	regenPerTick := fixed.One / 4 / 20
	hp.Add(e, id, fixed.FromInt(100), regenPerTick, 0, 0)

	r := hp.Row(id)
	raw := int64(hp.Regen[r])
	t.Logf("regen 0.25/s -> per-tick raw 32.32 value: %d (0x%016x)", raw, uint64(raw))
	drift := int64(fixed.One)/4 - raw*20
	t.Logf("raw/2^32 = %v (0.0125 target); 20 ticks accumulate raw %d vs exact 0.25 raw %d — truncation drift %d ulp",
		float64(raw)/(1<<32), raw*20, int64(fixed.One)/4, drift)
	// 1/20 is not dyadic, so 0.0125 truncates in binary fixed point.
	// The contract is DETERMINISM, not round-tripping: the per-tick
	// value is the exact same truncated constant on every platform.
	if raw != 53687091 {
		t.Fatalf("0.0125 in 32.32 must truncate to exactly 53687091, got %d", raw)
	}
	if drift < 0 || drift > 20 {
		t.Fatalf("accumulated truncation drift out of bounds: %d ulp over 20 ticks", drift)
	}
}

// Edge 4: destroying a unit removes all five component rows (plus
// Transform).
func TestStoreDestroyRemovesAllRows(t *testing.T) {
	w := NewWorld(Caps{Units: 16})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	w.Movements.Add(w.Ents, w.Transforms, id, fixed.One, 100)
	w.Collisions.Add(w.Ents, id, 1, PathGround)
	w.Healths.Add(w.Ents, id, fixed.FromInt(100), 0, 0, 0)
	w.Owners.Add(w.Ents, id, 0, 0, 0)
	w.UnitTypes.Add(w.Ents, id, 7)

	before := []int32{w.Transforms.Count(), w.Movements.Count(), w.Collisions.Count(), w.Healths.Count(), w.Owners.Count(), w.UnitTypes.Count()}
	w.DestroyUnit(id)
	after := []int32{w.Transforms.Count(), w.Movements.Count(), w.Collisions.Count(), w.Healths.Count(), w.Owners.Count(), w.UnitTypes.Count()}
	t.Logf("store counts (transform,mv,col,hp,own,ut) before=%v after=%v", before, after)
	for i, c := range after {
		if c != 0 {
			t.Fatalf("store %d not emptied: %d rows", i, c)
		}
	}
	_ = before
}

// Zero-alloc add/remove cycles per store (R-GC-1).
func TestStoreUnitsZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{Units: 64})
	id, _ := w.CreateUnit(fixed.Vec2{}, 0)
	allocs := testing.AllocsPerRun(200, func() {
		w.Movements.Add(w.Ents, w.Transforms, id, fixed.One, 100)
		w.Collisions.Add(w.Ents, id, 1, PathGround)
		w.Healths.Add(w.Ents, id, fixed.FromInt(100), 0, 0, 0)
		w.Owners.Add(w.Ents, id, 0, 0, 0)
		w.UnitTypes.Add(w.Ents, id, 7)
		w.Movements.Remove(id)
		w.Collisions.Remove(id)
		w.Healths.Remove(id)
		w.Owners.Remove(id)
		w.UnitTypes.Remove(id)
	})
	t.Logf("AllocsPerRun(add+remove x5 stores) = %v", allocs)
	if allocs != 0 {
		t.Fatalf("R-GC-1 violated: %v allocs/op", allocs)
	}
}

// Dense iteration over each store with rowOf joins, zero alloc.
func BenchmarkStoreIter(b *testing.B) {
	w := NewWorld(Caps{Units: 1000})
	for i := 0; i < 1000; i++ {
		id, _ := w.CreateUnit(fixed.Vec2{}, 0)
		w.Movements.Add(w.Ents, w.Transforms, id, fixed.One, 100)
		w.Healths.Add(w.Ents, id, fixed.FromInt(100), 1, 0, 0)
	}
	b.ReportAllocs()
	b.ResetTimer()
	var sink fixed.F64
	for i := 0; i < b.N; i++ {
		// movement walks its dense rows and probes Transform by rowOf
		for r := int32(0); r < w.Movements.Count(); r++ {
			tr := w.Transforms.Row(w.Movements.Entity[r])
			sink += w.Transforms.Pos[tr].X + w.Movements.Speed[r]
		}
		// health regen sweep
		for r := int32(0); r < w.Healths.Count(); r++ {
			w.Healths.Life[r] = w.Healths.Life[r].Add(w.Healths.Regen[r])
		}
	}
	_ = sink
}
