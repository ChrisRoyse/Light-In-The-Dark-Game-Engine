package sim

import "testing"

// Edge: destroy -> create reuses the same index with generation+1.
func TestEntityReuseBumpsGeneration(t *testing.T) {
	e := NewEntities(8)
	a, ok := e.Create()
	if !ok {
		t.Fatal("create failed")
	}
	t.Logf("created:   idx=%d gen=%d alive=%v", a.Index(), a.Generation(), e.Alive(a))
	if !e.Destroy(a) {
		t.Fatal("destroy failed")
	}
	b, ok := e.Create()
	if !ok {
		t.Fatal("re-create failed")
	}
	t.Logf("recreated: idx=%d gen=%d (old handle alive=%v)", b.Index(), b.Generation(), e.Alive(a))
	if b.Index() != a.Index() {
		t.Fatalf("LIFO free list should reuse index %d, got %d", a.Index(), b.Index())
	}
	if b.Generation() != a.Generation()+1 {
		t.Fatalf("generation: got %d want %d", b.Generation(), a.Generation()+1)
	}
}

// Edge: a stale handle resolves to dead after slot reuse, trips the
// debug assert hook, and every operation is a zero-value no-op.
func TestStaleHandleIsDeadNoOp(t *testing.T) {
	e := NewEntities(8)
	a, _ := e.Create()
	t.Logf("before reuse: Alive(a)=%v", e.Alive(a))
	e.Destroy(a)
	b, _ := e.Create() // same slot, new generation
	staleSeen := 0
	e.DebugStaleHandle = func(id EntityID) { staleSeen++ }
	t.Logf("after reuse:  Alive(stale a)=%v Alive(b)=%v", e.Alive(a), e.Alive(b))
	if e.Alive(a) {
		t.Fatal("stale handle reported alive")
	}
	if !e.Alive(b) {
		t.Fatal("fresh handle reported dead")
	}
	if e.Destroy(a) {
		t.Fatal("Destroy via stale handle must be a no-op")
	}
	if !e.Alive(b) || e.Count() != 1 {
		t.Fatalf("stale Destroy damaged live entity: alive=%v count=%d", e.Alive(b), e.Count())
	}
	if staleSeen != 3 { // Logf Alive(a) + if Alive(a) + Destroy(a); Alive(b) never trips
		t.Fatalf("debug assert hook fired %d times, want 3", staleSeen)
	}
	t.Logf("debug stale-handle hook fired %d times; live entity untouched", staleSeen)
}

// Edge: 256 reuses of one slot wrap the generation 255 -> 0 without
// panicking; stale detection still behaves (1-in-256 false positive
// accepted per ecs §3).
func TestGenerationWrapAt256(t *testing.T) {
	e := NewEntities(1) // single slot: every create reuses it
	gens := make([]uint8, 0, 258)
	var last EntityID
	for i := 0; i < 258; i++ {
		id, ok := e.Create()
		if !ok {
			t.Fatalf("create %d failed", i)
		}
		gens = append(gens, id.Generation())
		last = id
		e.Destroy(id)
	}
	t.Logf("gen sequence head: %v ... tail: %v", gens[:4], gens[254:258])
	if gens[255] != 255 || gens[256] != 0 || gens[257] != 1 {
		t.Fatalf("wrap wrong: gens[255..257] = %d,%d,%d want 255,0,1", gens[255], gens[256], gens[257])
	}
	if e.Alive(last) {
		t.Fatal("destroyed handle alive after wrap")
	}
}

// Edge: creation at pool cap fails as a gameplay outcome — count
// stays at cap, nothing reallocates.
func TestCreateAtCapFails(t *testing.T) {
	const capN = 100
	e := NewEntities(capN)
	for i := 0; i < capN; i++ {
		if _, ok := e.Create(); !ok {
			t.Fatalf("create %d failed below cap", i)
		}
	}
	id, ok := e.Create()
	t.Logf("cap=%d count=%d; create #%d -> ok=%v id=%d", e.Cap(), e.Count(), capN+1, ok, id)
	if ok {
		t.Fatal("creation past cap succeeded")
	}
	if e.Count() != capN || e.Cap() != capN {
		t.Fatalf("count=%d cap=%d, want both %d", e.Count(), e.Cap(), capN)
	}
}

// Free-list pop order is LIFO and deterministic: destroy A then B,
// next creates hand back B's slot then A's.
func TestFreeListLIFODeterministic(t *testing.T) {
	e := NewEntities(8)
	a, _ := e.Create()
	b, _ := e.Create()
	c, _ := e.Create()
	e.Destroy(a)
	e.Destroy(b)
	x, _ := e.Create()
	y, _ := e.Create()
	t.Logf("destroyed idx %d then %d; recreated idx %d then %d (LIFO)", a.Index(), b.Index(), x.Index(), y.Index())
	if x.Index() != b.Index() || y.Index() != a.Index() {
		t.Fatalf("free list not LIFO: got %d,%d want %d,%d", x.Index(), y.Index(), b.Index(), a.Index())
	}
	if !e.Alive(c) {
		t.Fatal("unrelated entity died")
	}
}

func TestZeroAllocCreateDestroy(t *testing.T) {
	e := NewEntities(1024)
	if n := testing.AllocsPerRun(10000, func() {
		id, _ := e.Create()
		e.Destroy(id)
	}); n != 0 {
		t.Fatalf("create/destroy allocates %v/op; R-GC-1 requires 0", n)
	}
	t.Log("AllocsPerRun = 0 for Create+Destroy")
}

func BenchmarkEntityCreateDestroy(b *testing.B) {
	e := NewEntities(4096)
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		id, _ := e.Create()
		e.Destroy(id)
	}
}
