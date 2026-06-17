package render

import (
	"testing"

	"github.com/g3n/engine/math32"
)

func req(pri VFXPriority, life int32, dist float32) VFXRequest {
	return VFXRequest{Priority: pri, Lifetime: life, Radius: 100, ScreenDist: dist,
		Color: math32.Color{R: 1, G: 1, B: 1}, Intensity: 1}
}

func TestVFXPoolFillAndCapFSV(t *testing.T) {
	p := NewVFXLightPool(nil, false)
	for i := 0; i < MaxVFXLights; i++ {
		_, d := p.Acquire(req(VFXStandardSpell, 100, 0))
		if !d.Granted || d.Reason != "free-slot" || d.Victim != -1 {
			t.Fatalf("acquire %d: %+v", i, d)
		}
	}
	t.Logf("FSV pool filled active=%d", p.ActiveCount())
	if p.ActiveCount() != MaxVFXLights {
		t.Fatalf("active=%d want %d", p.ActiveCount(), MaxVFXLights)
	}
	// 100 rapid same-priority requests: pool never exceeds the cap.
	maxActive := p.ActiveCount()
	for i := 0; i < 100; i++ {
		p.Acquire(req(VFXStandardSpell, 100, float32(i)))
		if a := p.ActiveCount(); a > maxActive {
			maxActive = a
		}
	}
	t.Logf("FSV after 100 rapid requests maxActive=%d", maxActive)
	if maxActive != MaxVFXLights {
		t.Fatalf("pool exceeded cap: maxActive=%d", maxActive)
	}
}

func TestVFXEvictLowerPriorityFSV(t *testing.T) {
	p := NewVFXLightPool(nil, false)
	for i := 0; i < MaxVFXLights; i++ {
		p.Acquire(req(VFXStandardSpell, 100, 0))
	}
	// 9th request, higher priority → evicts a standard.
	_, d := p.Acquire(req(VFXUltimate, 100, 0))
	t.Logf("FSV ultimate over full standard pool: %+v active=%d", d, p.ActiveCount())
	if !d.Granted || d.Victim < 0 || d.Reason != "evict:lower-priority" {
		t.Fatalf("ultimate should evict a standard: %+v", d)
	}
	if p.ActiveCount() != MaxVFXLights {
		t.Fatalf("active=%d want %d", p.ActiveCount(), MaxVFXLights)
	}

	// Fill with all ultimates, then a lower-priority request → denied, unchanged.
	p2 := NewVFXLightPool(nil, false)
	for i := 0; i < MaxVFXLights; i++ {
		p2.Acquire(req(VFXUltimate, 100, 0))
	}
	before := p2.ActiveCount()
	_, d2 := p2.Acquire(req(VFXStandardSpell, 100, 0))
	t.Logf("FSV standard over full ultimate pool: %+v active=%d", d2, p2.ActiveCount())
	if d2.Granted || d2.Reason != "denied:lower-priority" {
		t.Fatalf("lower-priority request should be denied: %+v", d2)
	}
	if p2.ActiveCount() != before {
		t.Fatalf("denied request changed pool: %d -> %d", before, p2.ActiveCount())
	}
}

func TestVFXEvictTieShortestLifetimeFSV(t *testing.T) {
	p := NewVFXLightPool(nil, false)
	// 8 same-priority lights with descending remaining lifetime; slot 5 shortest.
	lifetimes := []int32{100, 90, 80, 70, 60, 5, 40, 30}
	handles := make([]uint64, MaxVFXLights)
	for i, lf := range lifetimes {
		h, d := p.Acquire(req(VFXStandardSpell, lf, 0))
		handles[i] = h
		if !d.Granted || d.Slot != i {
			t.Fatalf("setup acquire %d: %+v", i, d)
		}
	}
	// 9th same priority → victim = slot 5 (remaining 5, the shortest).
	_, d := p.Acquire(req(VFXStandardSpell, 100, 0))
	t.Logf("FSV tie→shortest-lifetime victim=%d reason=%s (lifetimes=%v)", d.Victim, d.Reason, lifetimes)
	if !d.Granted || d.Victim != 5 || d.Reason != "evict:tie-lifetime-or-distance" {
		t.Fatalf("expected slot 5 (shortest lifetime) evicted: %+v", d)
	}
}

func TestVFXEvictTieFarthestDistanceFSV(t *testing.T) {
	p := NewVFXLightPool(nil, false)
	// 8 same priority, same lifetime, varying screen distance; slot 3 farthest.
	dists := []float32{10, 20, 30, 999, 40, 50, 60, 70}
	for i, ds := range dists {
		_, d := p.Acquire(req(VFXStandardSpell, 100, ds))
		if !d.Granted || d.Slot != i {
			t.Fatalf("setup acquire %d: %+v", i, d)
		}
	}
	_, d := p.Acquire(req(VFXStandardSpell, 100, 0))
	t.Logf("FSV tie→farthest-distance victim=%d reason=%s (dists=%v)", d.Victim, d.Reason, dists)
	if !d.Granted || d.Victim != 3 {
		t.Fatalf("expected slot 3 (farthest from centre) evicted: %+v", d)
	}
}

func TestVFXLifetimeExpiryFSV(t *testing.T) {
	p := NewVFXLightPool(nil, false)
	p.Acquire(req(VFXStandardSpell, 3, 0))
	p.Acquire(req(VFXUltimate, 10, 0))
	t.Logf("FSV before ticks active=%d", p.ActiveCount())
	if p.ActiveCount() != 2 {
		t.Fatalf("active=%d want 2", p.ActiveCount())
	}
	rel := 0
	for tick := 1; tick <= 3; tick++ {
		r := p.Tick()
		rel += r
		t.Logf("FSV tick %d released=%d active=%d", tick, r, p.ActiveCount())
	}
	// The lifetime-3 light expired; the lifetime-10 light remains.
	if rel != 1 || p.ActiveCount() != 1 {
		t.Fatalf("after 3 ticks released=%d active=%d, want 1 released / 1 active", rel, p.ActiveCount())
	}
}

func TestVFXInvalidRequestFSV(t *testing.T) {
	p := NewVFXLightPool(nil, false)
	_, d1 := p.Acquire(VFXRequest{Priority: VFXStandardSpell, Lifetime: 0, Radius: 100})
	_, d2 := p.Acquire(VFXRequest{Priority: VFXStandardSpell, Lifetime: 10, Radius: 0})
	t.Logf("FSV invalid lifetime=%+v radius=%+v active=%d", d1, d2, p.ActiveCount())
	if d1.Granted || d1.Reason != "denied:invalid-lifetime" {
		t.Fatalf("zero lifetime must be denied: %+v", d1)
	}
	if d2.Granted || d2.Reason != "denied:invalid-radius" {
		t.Fatalf("zero radius must be denied: %+v", d2)
	}
	if p.ActiveCount() != 0 {
		t.Fatalf("invalid requests bound a light: active=%d", p.ActiveCount())
	}
}

func TestVFXLowPresetFSV(t *testing.T) {
	p := NewVFXLightPool(nil, true)
	_, d := p.Acquire(req(VFXUltimate, 100, 0))
	t.Logf("FSV low-preset acquire=%+v active=%d", d, p.ActiveCount())
	if d.Granted || d.Reason != "denied:low-preset" {
		t.Fatalf("low preset must account but not bind: %+v", d)
	}
	if p.ActiveCount() != 0 {
		t.Fatalf("low preset bound a light: active=%d", p.ActiveCount())
	}
}

func TestVFXReleaseFSV(t *testing.T) {
	p := NewVFXLightPool(nil, false)
	h, _ := p.Acquire(req(VFXStandardSpell, 100, 0))
	if !p.Release(h) || p.ActiveCount() != 0 {
		t.Fatalf("release failed: active=%d", p.ActiveCount())
	}
	if p.Release(h) {
		t.Fatalf("stale handle release must return false")
	}
	if p.Release(99999) {
		t.Fatalf("unknown handle release must return false")
	}
}

func TestVFXZeroAllocFSV(t *testing.T) {
	p := NewVFXLightPool(nil, false)
	// Steady churn: acquire then release the same slot, repeatedly.
	r := req(VFXStandardSpell, 100, 0)
	allocs := testing.AllocsPerRun(1000, func() {
		h, _ := p.Acquire(r)
		p.Release(h)
		p.Tick()
	})
	t.Logf("FSV acquire/release/tick allocs/op = %v", allocs)
	if allocs != 0 {
		t.Fatalf("VFX light pool steady-state allocates %v/op, want 0", allocs)
	}
}
