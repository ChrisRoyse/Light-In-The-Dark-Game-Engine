package render

// #309 impact one-shot pool FSV. SoT = the pool's active set + each slot's
// remaining lifetime, read via SnapshotInto, across Acquire→Tick→expiry. X+X=Y:
// a lifetime-3 impact lives exactly 3 ticks then auto-releases. Plus fail-closed
// refusal (non-positive lifetime/size), full-pool reuse of the shortest-remaining
// slot, and zero steady-state allocation (R-GC-2). Headless — no GL.

import (
	"testing"

	"github.com/g3n/engine/math32"
)

func impactReq(lifetime int32) ImpactRequest {
	return ImpactRequest{
		Pos:      math32.Vector3{X: 1, Y: 2, Z: 3},
		Size:     0.5,
		Color:    math32.Color{R: 1, G: 0.5, B: 0.2},
		UV:       math32.Vector4{X: 0, Y: 0, Z: 1, W: 1},
		Lifetime: lifetime,
	}
}

func slotByHandle(p *ImpactFXPool, h uint64) (ImpactSlotInfo, bool) {
	for _, s := range p.SnapshotInto(make([]ImpactSlotInfo, 0, MaxImpactFX)) {
		if s.Active && s.Handle == h {
			return s, true
		}
	}
	return ImpactSlotInfo{}, false
}

func TestImpactFXLifecycleFSV(t *testing.T) {
	p := NewImpactFXPool()
	if p.ActiveCount() != 0 {
		t.Fatalf("fresh pool active=%d, want 0", p.ActiveCount())
	}

	h, dec := p.Acquire(impactReq(3))
	t.Logf("FSV acquire: handle=%d decision=%+v active=%d", h, dec, p.ActiveCount())
	if !dec.Granted || h == 0 || dec.Slot < 0 || dec.Victim != -1 {
		t.Fatalf("acquire into empty pool wrong: %+v", dec)
	}
	if p.ActiveCount() != 1 {
		t.Fatalf("active=%d after 1 acquire, want 1", p.ActiveCount())
	}
	// Fresh: LifeFrac == 1.0, remaining == 3.
	s, ok := slotByHandle(p, h)
	if !ok || s.Remaining != 3 || s.MaxLife != 3 || s.LifeFrac != 1 {
		t.Fatalf("fresh slot = %+v, want remaining3 maxLife3 frac1", s)
	}

	// X+X=Y: lifetime 3 → exactly 3 ticks to expire. Walk it tick by tick.
	if r := p.Tick(); r != 0 || p.ActiveCount() != 1 {
		t.Fatalf("tick1 released=%d active=%d, want 0/1", r, p.ActiveCount())
	}
	s, _ = slotByHandle(p, h)
	t.Logf("FSV tick1: remaining=%d lifeFrac=%.3f", s.Remaining, s.LifeFrac)
	if s.Remaining != 2 || s.LifeFrac < 0.66 || s.LifeFrac > 0.67 {
		t.Fatalf("after tick1: remaining=%d frac=%.3f, want 2/~0.667", s.Remaining, s.LifeFrac)
	}
	if r := p.Tick(); r != 0 || p.ActiveCount() != 1 {
		t.Fatalf("tick2 released=%d active=%d, want 0/1", r, p.ActiveCount())
	}
	// Third tick drops remaining 1→0 → auto-release.
	if r := p.Tick(); r != 1 || p.ActiveCount() != 0 {
		t.Fatalf("tick3 released=%d active=%d, want 1/0 (auto-expire at lifetime)", r, p.ActiveCount())
	}
	if _, ok := slotByHandle(p, h); ok {
		t.Fatal("expired impact still active after its lifetime")
	}
	t.Logf("FSV lifecycle: lifetime-3 impact auto-released on tick 3, active=%d", p.ActiveCount())
}

func TestImpactFXRefuseEdgesFSV(t *testing.T) {
	p := NewImpactFXPool()

	// Non-positive lifetime → refused, nothing bound.
	if h, dec := p.Acquire(impactReq(0)); h != 0 || dec.Granted || p.ActiveCount() != 0 {
		t.Fatalf("lifetime 0 accepted: handle=%d dec=%+v active=%d", h, dec, p.ActiveCount())
	}
	if h, dec := p.Acquire(impactReq(-5)); h != 0 || dec.Granted {
		t.Fatalf("negative lifetime accepted: handle=%d dec=%+v", h, dec)
	}
	// Non-positive size → refused.
	bad := impactReq(3)
	bad.Size = 0
	if h, dec := p.Acquire(bad); h != 0 || dec.Granted || p.ActiveCount() != 0 {
		t.Fatalf("zero size accepted: handle=%d dec=%+v", h, dec)
	}
	t.Logf("FSV refusals: non-positive lifetime/size refused, active=%d", p.ActiveCount())

	// Release of a stale handle is a no-op false.
	if p.Release(999) {
		t.Fatal("Release(stale) returned true")
	}
}

func TestImpactFXFullReuseFSV(t *testing.T) {
	p := NewImpactFXPool()
	// Fill every slot; slot i gets lifetime i+1, so slot 0 has the least remaining.
	handles := make([]uint64, MaxImpactFX)
	for i := 0; i < MaxImpactFX; i++ {
		h, dec := p.Acquire(impactReq(int32(i + 1)))
		if !dec.Granted || dec.Victim != -1 {
			t.Fatalf("fill %d: expected free-slot grant, got %+v", i, dec)
		}
		handles[i] = h
	}
	if p.ActiveCount() != MaxImpactFX {
		t.Fatalf("full pool active=%d, want %d", p.ActiveCount(), MaxImpactFX)
	}

	// One more: the pool is full → reuse the shortest-remaining slot (the
	// lifetime-1 one, slot 0). Active count stays capped (no growth).
	hNew, dec := p.Acquire(impactReq(99))
	t.Logf("FSV full reuse: decision=%+v active=%d", dec, p.ActiveCount())
	if !dec.Granted || dec.Victim < 0 {
		t.Fatalf("overflow acquire should reuse a slot: %+v", dec)
	}
	if p.ActiveCount() != MaxImpactFX {
		t.Fatalf("active=%d after overflow, want %d (capped, no growth)", p.ActiveCount(), MaxImpactFX)
	}
	// SoT: the reused slot now holds the new handle with the new lifetime, and
	// the original shortest handle is gone.
	s, ok := slotByHandle(p, hNew)
	if !ok || s.Remaining != 99 {
		t.Fatalf("reused slot = %+v (found=%v), want remaining 99", s, ok)
	}
	if _, ok := slotByHandle(p, handles[0]); ok {
		t.Fatal("the shortest-remaining impact was not the one reused")
	}
}

func TestImpactFXZeroAllocFSV(t *testing.T) {
	p := NewImpactFXPool()
	dst := make([]ImpactSlotInfo, 0, MaxImpactFX)
	// Prime the pool to steady state (full), so Acquire takes the reuse path.
	for i := 0; i < MaxImpactFX; i++ {
		p.Acquire(impactReq(10))
	}
	allocs := testing.AllocsPerRun(200, func() {
		p.Acquire(impactReq(5)) // reuse path, no growth
		p.Tick()
		dst = p.SnapshotInto(dst)
	})
	t.Logf("FSV zero-alloc: allocs/op=%.2f over Acquire+Tick+SnapshotInto", allocs)
	if allocs != 0 {
		t.Fatalf("impact pool allocated %.2f/op at steady state, want 0 (R-GC-2)", allocs)
	}
}
