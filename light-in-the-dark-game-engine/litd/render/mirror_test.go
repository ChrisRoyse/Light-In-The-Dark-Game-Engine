package render

import (
	"sort"
	"testing"
)

// SoT for the mirror is the MirrorDelta returned each Sync plus the cumulative
// MirrorStats — both pure bookkeeping, independently checkable without GL.

func spawnedSlots(d MirrorDelta) []int {
	out := make([]int, len(d.Spawned))
	for i, s := range d.Spawned {
		out[i] = s.Slot
	}
	sort.Ints(out)
	return out
}

// TestModelMirrorSpawnAndSteadyStateFSV: first Sync of N entities spawns N
// instances; an identical re-Sync produces an empty delta (the steady-state path).
func TestModelMirrorSpawnAndSteadyStateFSV(t *testing.T) {
	m := NewModelMirror(8)
	live := []MirrorEntry{{Key: 10, Model: 1}, {Key: 11, Model: 1}, {Key: 12, Model: 2}}

	d := m.Sync(live)
	if len(d.Spawned) != 3 || len(d.Despawned) != 0 {
		t.Fatalf("first Sync: spawned=%d despawned=%d, want 3/0", len(d.Spawned), len(d.Despawned))
	}
	st := m.Stats()
	if st.Created != 3 || st.Reused != 0 || st.Live != 3 || st.Capacity != 3 {
		t.Fatalf("after spawn: %+v, want created=3 reused=0 live=3 cap=3", st)
	}
	// Every live key resolves to a distinct slot.
	seen := map[int]bool{}
	for _, e := range live {
		s, ok := m.Slot(e.Key)
		if !ok || seen[s] {
			t.Fatalf("key %d slot=%d ok=%v (dup=%v)", e.Key, s, ok, seen[s])
		}
		seen[s] = true
	}
	t.Logf("FSV spawn: 3 entities → 3 instances, slots %v", spawnedSlots(d))

	// Steady state: same set again → no work, no growth.
	d2 := m.Sync(live)
	if len(d2.Spawned) != 0 || len(d2.Despawned) != 0 {
		t.Fatalf("steady re-Sync: spawned=%d despawned=%d, want 0/0", len(d2.Spawned), len(d2.Despawned))
	}
	if st2 := m.Stats(); st2.Created != 3 || st2.Live != 3 {
		t.Fatalf("steady stats grew: %+v", st2)
	}
	t.Logf("FSV steady: identical re-Sync → empty delta, capacity still 3")
}

// TestModelMirrorDeathReusesSlotFSV: a dying entity's slot returns to the pool and
// is reused by the next spawn — created stays flat, reused climbs, capacity holds.
func TestModelMirrorDeathReusesSlotFSV(t *testing.T) {
	m := NewModelMirror(8)
	m.Sync([]MirrorEntry{{Key: 1, Model: 5}, {Key: 2, Model: 5}, {Key: 3, Model: 5}})
	deadSlot, _ := m.Slot(2)

	// Entity 2 dies; entity 4 spawns. One despawn + one spawn; slot reused.
	d := m.Sync([]MirrorEntry{{Key: 1, Model: 5}, {Key: 3, Model: 5}, {Key: 4, Model: 7}})
	if len(d.Despawned) != 1 || d.Despawned[0] != deadSlot {
		t.Fatalf("despawned=%v, want exactly the dead slot %d", d.Despawned, deadSlot)
	}
	if len(d.Spawned) != 1 || d.Spawned[0].Slot != deadSlot || d.Spawned[0].Key != 4 || d.Spawned[0].Model != 7 {
		t.Fatalf("spawned=%+v, want key4/model7 reusing slot %d", d.Spawned, deadSlot)
	}
	st := m.Stats()
	if st.Created != 3 || st.Reused != 1 || st.Destroyed != 1 || st.Capacity != 3 || st.Live != 3 {
		t.Fatalf("after recycle: %+v, want created=3 reused=1 destroyed=1 cap=3 live=3", st)
	}
	if _, ok := m.Slot(2); ok {
		t.Fatal("dead key 2 still resolves")
	}
	t.Logf("FSV recycle: key2 died, key4 reused slot %d — created flat at 3, reused=1, cap=3", deadSlot)
}

// TestModelMirrorModelSwapFSV: a surviving entity that changes model is a
// despawn+spawn on its SAME slot (rebuild the mesh), counted as a swap.
func TestModelMirrorModelSwapFSV(t *testing.T) {
	m := NewModelMirror(4)
	m.Sync([]MirrorEntry{{Key: 99, Model: 1}})
	slot, _ := m.Slot(99)

	d := m.Sync([]MirrorEntry{{Key: 99, Model: 2}}) // morph 1 → 2
	if len(d.Despawned) != 1 || d.Despawned[0] != slot {
		t.Fatalf("model swap despawned=%v, want same slot %d", d.Despawned, slot)
	}
	if len(d.Spawned) != 1 || d.Spawned[0].Slot != slot || d.Spawned[0].Model != 2 {
		t.Fatalf("model swap spawned=%+v, want model 2 on slot %d", d.Spawned, slot)
	}
	st := m.Stats()
	if st.Swapped != 1 || st.Live != 1 || st.Created != 1 || st.Reused != 0 {
		t.Fatalf("after swap: %+v, want swapped=1 live=1 created=1 reused=0", st)
	}
	t.Logf("FSV swap: key99 model 1→2 → despawn+spawn on slot %d (mesh rebuilt), swapped=1", slot)
}

// TestModelMirrorIgnoresKeyZeroFSV: the reserved entity key is never mirrored.
func TestModelMirrorIgnoresKeyZeroFSV(t *testing.T) {
	m := NewModelMirror(4)
	d := m.Sync([]MirrorEntry{{Key: 0, Model: 3}, {Key: 7, Model: 3}})
	if len(d.Spawned) != 1 || d.Spawned[0].Key != 7 {
		t.Fatalf("spawned=%+v, want only key 7", d.Spawned)
	}
	if m.Live() != 1 {
		t.Fatalf("live=%d, want 1 (key 0 ignored)", m.Live())
	}
	t.Logf("FSV key0: reserved key skipped, only key7 mirrored")
}

// TestModelMirrorChurnNoGrowthFSV: 1-death-1-spawn churn for many ticks keeps the
// instance table at the live count (pool fully reused) — the eviction-free,
// bounded-memory property the draw-call budget relies on.
func TestModelMirrorChurnNoGrowthFSV(t *testing.T) {
	const pop = 50
	m := NewModelMirror(pop)
	live := make([]MirrorEntry, pop)
	next := uint32(1)
	for i := range live {
		live[i] = MirrorEntry{Key: next, Model: 1}
		next++
	}
	m.Sync(live)
	if m.Stats().Capacity != pop {
		t.Fatalf("warm capacity=%d, want %d", m.Stats().Capacity, pop)
	}

	// Each tick: oldest entity dies, a fresh one spawns (rolling population).
	for tick := 0; tick < 500; tick++ {
		copy(live, live[1:])
		live[pop-1] = MirrorEntry{Key: next, Model: 1}
		next++
		d := m.Sync(live)
		if len(d.Spawned) != 1 || len(d.Despawned) != 1 {
			t.Fatalf("tick %d: spawned=%d despawned=%d, want 1/1", tick, len(d.Spawned), len(d.Despawned))
		}
	}
	st := m.Stats()
	if st.Capacity != pop || st.Live != pop {
		t.Fatalf("after 500-tick churn: capacity=%d live=%d, want %d/%d (no growth)", st.Capacity, st.Live, pop, pop)
	}
	if st.Created != pop || st.Reused != 500 {
		t.Fatalf("churn counts: created=%d reused=%d, want created=%d reused=500", st.Created, st.Reused, pop)
	}
	t.Logf("FSV churn: 500 ticks of 1-in-1-out at pop %d → capacity flat at %d, created=%d, reused=%d", pop, pop, st.Created, st.Reused)
}

// TestModelMirrorSteadyStateZeroAllocFSV: a stable-population Sync allocates
// nothing (R-GC-3) — the per-frame mirror is on the zero-alloc render path.
func TestModelMirrorSteadyStateZeroAllocFSV(t *testing.T) {
	m := NewModelMirror(32)
	live := make([]MirrorEntry, 32)
	for i := range live {
		live[i] = MirrorEntry{Key: uint32(i + 1), Model: ModelID(i%3 + 1)}
	}
	m.Sync(live) // warm
	m.Sync(live)

	allocs := testing.AllocsPerRun(200, func() { m.Sync(live) })
	if allocs != 0 {
		t.Fatalf("steady-state Sync allocated %v/op, want 0", allocs)
	}
	t.Logf("FSV zero-alloc: steady-state Sync = %v allocs/op", allocs)
}
