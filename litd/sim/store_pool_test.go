package sim

import (
	"testing"
)

// Edge 1: the 8,001st buff fails deterministically; count stays at
// cap and the debug assert fires.
func TestStoreBuffPoolExhaustion(t *testing.T) {
	w := NewWorld(Caps{}) // engine default: 8,000 buffs
	asserts := 0
	w.Buffs.DebugAssert = func(string) { asserts++ }
	for i := 0; i < 8000; i++ {
		if _, ok := w.Buffs.Alloc(); !ok {
			t.Fatalf("alloc %d failed below cap", i)
		}
	}
	idx, ok := w.Buffs.Alloc()
	t.Logf("8,001st alloc: idx=%d ok=%v; live=%d cap=%d; assert fired %d time(s)",
		idx, ok, w.Buffs.Live(), w.Buffs.Cap(), asserts)
	if ok || w.Buffs.Live() != 8000 || asserts != 1 {
		t.Fatalf("exhaustion mishandled: ok=%v live=%d asserts=%d", ok, w.Buffs.Live(), asserts)
	}
}

// Edge 2: LIFO recycling — free A,B,C then three allocs return C,B,A.
func TestStorePoolLIFOOrder(t *testing.T) {
	p := NewBuffPool(16)
	a, _ := p.Alloc()
	b, _ := p.Alloc()
	c, _ := p.Alloc()
	t.Logf("allocated slots: A=%d B=%d C=%d", a, b, c)
	p.Free(a)
	p.Free(b)
	p.Free(c)
	x, _ := p.Alloc()
	y, _ := p.Alloc()
	z, _ := p.Alloc()
	t.Logf("freed A,B,C then re-allocated: %d, %d, %d (want %d, %d, %d — LIFO)", x, y, z, c, b, a)
	if x != c || y != b || z != a {
		t.Fatalf("free list is not LIFO: got %d,%d,%d want %d,%d,%d", x, y, z, c, b, a)
	}
}

// Edge 3: 100k alloc/free churn never moves the backing array.
func TestStorePoolChurnPointerStable(t *testing.T) {
	p := NewBuffPool(2000)
	i0, _ := p.Alloc()
	pre := p.Row(i0)
	preAddr := &p.rows[0]
	p.Free(i0)
	for i := 0; i < 100_000; i++ {
		j, ok := p.Alloc()
		if !ok {
			t.Fatalf("churn alloc failed at %d", i)
		}
		p.Row(j).RemainingTicks = 100
		p.Free(j)
	}
	postAddr := &p.rows[0]
	t.Logf("backing array &rows[0] pre=%p post=%p (100,000 alloc/free cycles)", preAddr, postAddr)
	if preAddr != postAddr {
		t.Fatalf("backing array moved during churn")
	}
	_ = pre
}

// Double-free fails closed.
func TestStorePoolDoubleFree(t *testing.T) {
	p := NewBuffPool(4)
	asserts := 0
	p.DebugAssert = func(string) { asserts++ }
	i, _ := p.Alloc()
	if !p.Free(i) || p.Free(i) {
		t.Fatalf("first free must succeed, second must fail")
	}
	if asserts != 1 {
		t.Fatalf("double free must assert once, got %d", asserts)
	}
	if p.Live() != 0 {
		t.Fatalf("double free corrupted live count: %d", p.Live())
	}
}

func BenchmarkPoolBuff(b *testing.B) {
	p := NewBuffPool(8000)
	b.ReportAllocs()
	for i := 0; i < b.N; i++ {
		j, _ := p.Alloc()
		r := p.Row(j)
		r.BuffID = 7
		r.RemainingTicks = 100
		p.Free(j)
	}
}
