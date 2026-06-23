package render

// #309 buff aura attachment FSV. SoT = the pool's active set + each slot's
// unitKey / remaining / resolved follow-position, read via SnapshotInto, across
// Acquire→Update→Tick→Release. X+X=Y: a temporary aura with lifetime 3 lives
// exactly 3 ticks; a permanent aura (lifetime 0) never expires; an aura at
// offset {0,2,0} on a unit at {10,0,5} resolves to {10,2,5}. Fail-closed edges
// (negative lifetime, zero size, full pool) and zero steady-state alloc. Headless.

import (
	"testing"

	"github.com/g3n/engine/math32"
)

func auraReq(unit uint32, lifetime int32) BuffAuraRequest {
	return BuffAuraRequest{
		UnitKey:  unit,
		Offset:   math32.Vector3{X: 0, Y: 2, Z: 0},
		Size:     0.4,
		Color:    math32.Color{R: 0.3, G: 0.6, B: 1},
		UV:       math32.Vector4{X: 0, Y: 0, Z: 1, W: 1},
		Lifetime: lifetime,
	}
}

func auraByHandle(p *BuffAuraPool, h uint64) (BuffAuraSlotInfo, bool) {
	for _, s := range p.SnapshotInto(make([]BuffAuraSlotInfo, 0, MaxBuffAuras)) {
		if s.Active && s.Handle == h {
			return s, true
		}
	}
	return BuffAuraSlotInfo{}, false
}

func TestBuffAuraLifecycleFSV(t *testing.T) {
	p := NewBuffAuraPool()

	// Permanent aura (lifetime 0): remaining -1, never auto-expires.
	hPerm, dPerm := p.Acquire(auraReq(7, 0))
	t.Logf("FSV permanent attach: handle=%d decision=%+v", hPerm, dPerm)
	sp, ok := auraByHandle(p, hPerm)
	if !ok || !sp.Permanent || sp.Remaining != -1 || sp.UnitKey != 7 {
		t.Fatalf("permanent aura slot = %+v, want permanent/-1/unit7", sp)
	}

	// Temporary aura (lifetime 3) on the same unit.
	hTmp, _ := p.Acquire(auraReq(7, 3))
	if p.ActiveCount() != 2 {
		t.Fatalf("active=%d after 2 attach, want 2", p.ActiveCount())
	}

	// Tick 3 times: temporary expires exactly on tick 3, permanent untouched.
	if r := p.Tick(); r != 0 || p.ActiveCount() != 2 {
		t.Fatalf("tick1 released=%d active=%d, want 0/2", r, p.ActiveCount())
	}
	st, _ := auraByHandle(p, hTmp)
	t.Logf("FSV tick1: temp remaining=%d", st.Remaining)
	if st.Remaining != 2 {
		t.Fatalf("temp remaining after tick1 = %d, want 2", st.Remaining)
	}
	p.Tick()
	if r := p.Tick(); r != 1 || p.ActiveCount() != 1 {
		t.Fatalf("tick3 released=%d active=%d, want 1/1 (temp expired, permanent stays)", r, p.ActiveCount())
	}
	if _, ok := auraByHandle(p, hTmp); ok {
		t.Fatal("temporary aura still active past its lifetime")
	}
	if _, ok := auraByHandle(p, hPerm); !ok {
		t.Fatal("permanent aura wrongly expired on Tick")
	}
	t.Logf("FSV lifecycle: temp expired tick 3, permanent survived, active=%d", p.ActiveCount())
}

func TestBuffAuraFollowFSV(t *testing.T) {
	p := NewBuffAuraPool()
	h, _ := p.Acquire(auraReq(7, 0)) // offset {0,2,0} on unit 7

	// Unit 7 is at {10,0,5}; the aura must resolve to {10,2,5} and be visible.
	unitPos := map[uint32]math32.Vector3{7: {X: 10, Y: 0, Z: 5}}
	p.Update(func(k uint32) (math32.Vector3, bool) { v, ok := unitPos[k]; return v, ok })
	s, _ := auraByHandle(p, h)
	t.Logf("FSV follow: unit{10,0,5}+offset{0,2,0} -> pos=%+v visible=%v", s.Pos, s.Visible)
	if !s.Visible || s.Pos.X != 10 || s.Pos.Y != 2 || s.Pos.Z != 5 {
		t.Fatalf("followed pos = %+v visible=%v, want {10,2,5}/true", s.Pos, s.Visible)
	}

	// Unit gone (lookup miss): aura marked not-visible but NOT released.
	p.Update(func(k uint32) (math32.Vector3, bool) { return math32.Vector3{}, false })
	s, ok := auraByHandle(p, h)
	t.Logf("FSV follow-miss: visible=%v active=%v", s.Visible, ok)
	if !ok {
		t.Fatal("aura released on a transient lookup miss (should only mark invisible)")
	}
	if s.Visible {
		t.Fatal("aura still visible after its unit disappeared")
	}
}

func TestBuffAuraReleaseByUnitFSV(t *testing.T) {
	p := NewBuffAuraPool()
	p.Acquire(auraReq(7, 0))
	p.Acquire(auraReq(7, 50))
	h9, _ := p.Acquire(auraReq(9, 0))
	if p.ActiveCount() != 3 {
		t.Fatalf("active=%d, want 3", p.ActiveCount())
	}

	// Unit 7 despawns: both of its auras go, unit 9's stays.
	n := p.ReleaseByUnit(7)
	t.Logf("FSV ReleaseByUnit(7): released=%d active=%d", n, p.ActiveCount())
	if n != 2 || p.ActiveCount() != 1 {
		t.Fatalf("ReleaseByUnit(7)=%d active=%d, want 2/1", n, p.ActiveCount())
	}
	if _, ok := auraByHandle(p, h9); !ok {
		t.Fatal("unit-9 aura wrongly released by ReleaseByUnit(7)")
	}
}

func TestBuffAuraRefuseEdgesFSV(t *testing.T) {
	p := NewBuffAuraPool()

	if h, d := p.Acquire(auraReq(1, -1)); h != 0 || d.Granted {
		t.Fatalf("negative lifetime accepted: %+v", d)
	}
	bad := auraReq(1, 0)
	bad.Size = 0
	if h, d := p.Acquire(bad); h != 0 || d.Granted {
		t.Fatalf("zero size accepted: %+v", d)
	}
	if p.ActiveCount() != 0 {
		t.Fatalf("refused requests still bound: active=%d", p.ActiveCount())
	}

	// Fill the pool, then the next attach is refused (auras don't evict buffs).
	for i := 0; i < MaxBuffAuras; i++ {
		if _, d := p.Acquire(auraReq(uint32(i), 0)); !d.Granted {
			t.Fatalf("fill %d refused early: %+v", i, d)
		}
	}
	h, d := p.Acquire(auraReq(999, 0))
	t.Logf("FSV full-pool: decision=%+v active=%d", d, p.ActiveCount())
	if h != 0 || d.Granted || d.Reason != "refused:pool-full" {
		t.Fatalf("full-pool attach not refused: handle=%d %+v", h, d)
	}
	if p.ActiveCount() != MaxBuffAuras {
		t.Fatalf("active=%d after full-pool refusal, want %d", p.ActiveCount(), MaxBuffAuras)
	}
	if p.Release(123456) {
		t.Fatal("Release(stale) returned true")
	}
}

func TestBuffAuraZeroAllocFSV(t *testing.T) {
	p := NewBuffAuraPool()
	dst := make([]BuffAuraSlotInfo, 0, MaxBuffAuras)
	for i := 0; i < MaxBuffAuras/2; i++ {
		p.Acquire(auraReq(uint32(i), 100))
	}
	lookup := func(k uint32) (math32.Vector3, bool) { return math32.Vector3{X: 1, Y: 1, Z: 1}, true }
	allocs := testing.AllocsPerRun(200, func() {
		p.Update(lookup)
		p.Tick()
		dst = p.SnapshotInto(dst)
	})
	t.Logf("FSV zero-alloc: allocs/op=%.2f over Update+Tick+SnapshotInto", allocs)
	if allocs != 0 {
		t.Fatalf("aura pool allocated %.2f/op at steady state, want 0 (R-GC-2)", allocs)
	}
}
