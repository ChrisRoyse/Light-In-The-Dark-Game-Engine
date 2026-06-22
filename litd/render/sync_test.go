package render

import (
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// snap1 builds a single-slot snapshot at the given integer sim coordinates.
func snap1(tick uint64, x, y, z, facing int32, present, snapFlag bool) *Snapshot {
	return &Snapshot{
		Tick:    tick,
		X:       []fixed.F64{fixed.FromInt(x)},
		Y:       []fixed.F64{fixed.FromInt(y)},
		Z:       []fixed.F64{fixed.FromInt(z)},
		Facing:  []fixed.F64{fixed.FromInt(facing)},
		Present: []bool{present},
		Snap:    []bool{snapFlag},
	}
}

func requireMirrorEntries(t *testing.T, got []MirrorEntry, want ...MirrorEntry) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("mirror entries len=%d got=%+v want=%+v", len(got), got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("mirror entry %d=%+v want %+v (all got=%+v)", i, got[i], want[i], got)
		}
	}
}

func requireMirrorErr(t *testing.T, label string, s *Snapshot, contains string) {
	t.Helper()
	before := make([]MirrorEntry, 1, 5)
	before[0] = MirrorEntry{Key: 999, Model: 9}
	after, err := s.MirrorEntries(before)
	t.Logf("FSV mirror contract edge=%s before=%+v after=%+v err=%v", label, before, after, err)
	if err == nil || !strings.Contains(err.Error(), contains) {
		t.Fatalf("%s err=%v, want substring %q", label, err, contains)
	}
	if len(after) != 0 {
		t.Fatalf("%s after len=%d, want 0 on failed contract", label, len(after))
	}
}

func TestSnapshotMirrorEntriesFSV(t *testing.T) {
	s := &Snapshot{
		Present:   []bool{true, true, false, true, true},
		EntityKey: []uint32{101, 0, 202, 303, 404},
		Model:     []ModelID{7, 8, 9, ModelNone, 11},
	}
	before := make([]MirrorEntry, 1, 5)
	before[0] = MirrorEntry{Key: 999, Model: 9}
	beforeSentinel, beforeCap := before[0], cap(before)
	got, err := s.MirrorEntries(before)
	if err != nil {
		t.Fatalf("MirrorEntries returned error: %v", err)
	}
	t.Logf("FSV mirror entries beforeSentinel=%+v beforeCap=%d snapshot.present=%v keys=%v models=%v after=%+v afterCap=%d", beforeSentinel, beforeCap, s.Present, s.EntityKey, s.Model, got, cap(got))
	requireMirrorEntries(t, got,
		MirrorEntry{Key: 101, Model: 7},
		MirrorEntry{Key: 303, Model: ModelNone},
		MirrorEntry{Key: 404, Model: 11},
	)
	if cap(got) != beforeCap {
		t.Fatalf("destination buffer not reused: cap after=%d before=%d", cap(got), beforeCap)
	}
}

func TestSnapshotMirrorEntriesContractEdgesFSV(t *testing.T) {
	empty := &Snapshot{}
	emptyEntries, err := empty.MirrorEntries(nil)
	t.Logf("FSV mirror contract edge=empty before=nil after=%+v err=%v", emptyEntries, err)
	if err != nil || len(emptyEntries) != 0 {
		t.Fatalf("empty snapshot entries=%+v err=%v, want empty/nil", emptyEntries, err)
	}

	requireMirrorErr(t, "nil snapshot", nil, "nil snapshot")
	requireMirrorErr(t, "short entity key column", &Snapshot{
		Present:   []bool{true, true},
		EntityKey: []uint32{1},
		Model:     []ModelID{1, 2},
	}, "entity key column length 1 below present length 2")
	requireMirrorErr(t, "short model column", &Snapshot{
		Present:   []bool{true, true},
		EntityKey: []uint32{1, 2},
		Model:     []ModelID{1},
	}, "model column length 1 below present length 2")
}

func TestSnapshotMirrorEntriesDriveModelMirrorFSV(t *testing.T) {
	s1 := &Snapshot{
		Present:   []bool{true, true},
		EntityKey: []uint32{50, 51},
		Model:     []ModelID{1, 2},
	}
	entries, err := s1.MirrorEntries(make([]MirrorEntry, 0, 2))
	if err != nil {
		t.Fatalf("s1 entries: %v", err)
	}
	m := NewModelMirror(2)
	d1 := m.Sync(entries)
	if len(d1.Spawned) != 2 || len(d1.Despawned) != 0 {
		t.Fatalf("initial mirror delta spawned=%+v despawned=%v, want 2/0", d1.Spawned, d1.Despawned)
	}
	slot50, _ := m.Slot(50)
	slot51, _ := m.Slot(51)
	t.Logf("FSV mirror trigger=s1 entries=%+v after delta=%+v stats=%+v", entries, d1, m.Stats())

	s2 := &Snapshot{
		Present:   []bool{true, true},
		EntityKey: []uint32{51, 52},
		Model:     []ModelID{3, 4},
	}
	entries, err = s2.MirrorEntries(entries)
	if err != nil {
		t.Fatalf("s2 entries: %v", err)
	}
	d2 := m.Sync(entries)
	st := m.Stats()
	t.Logf("FSV mirror trigger=s2 entries=%+v beforeSlots=(50:%d 51:%d) after delta=%+v stats=%+v", entries, slot50, slot51, d2, st)

	if len(d2.Despawned) != 2 || len(d2.Spawned) != 2 {
		t.Fatalf("second mirror delta spawned=%+v despawned=%v, want 2/2", d2.Spawned, d2.Despawned)
	}
	if d2.Despawned[0] != slot51 || d2.Despawned[1] != slot50 {
		t.Fatalf("second despawned=%v, want swap slot %d then dead slot %d", d2.Despawned, slot51, slot50)
	}
	if d2.Spawned[0] != (MirrorSpawn{Key: 51, Model: 3, Slot: slot51}) {
		t.Fatalf("swap spawn=%+v, want key51 model3 slot%d", d2.Spawned[0], slot51)
	}
	if d2.Spawned[1] != (MirrorSpawn{Key: 52, Model: 4, Slot: slot50}) {
		t.Fatalf("reused spawn=%+v, want key52 model4 slot%d", d2.Spawned[1], slot50)
	}
	if st.Created != 2 || st.Reused != 1 || st.Destroyed != 1 || st.Swapped != 1 || st.Live != 2 || st.Capacity != 2 {
		t.Fatalf("stats=%+v, want created=2 reused=1 destroyed=1 swapped=1 live=2 capacity=2", st)
	}
}

func TestSnapshotMirrorEntriesZeroAllocFSV(t *testing.T) {
	const n = 500
	s := &Snapshot{
		Present:   make([]bool, n),
		EntityKey: make([]uint32, n),
		Model:     make([]ModelID, n),
	}
	for i := 0; i < n; i++ {
		s.Present[i] = true
		s.EntityKey[i] = uint32(i + 1)
		s.Model[i] = ModelID(i%5 + 1)
	}
	dst := make([]MirrorEntry, 0, n)
	var err error
	dst, err = s.MirrorEntries(dst)
	if err != nil {
		t.Fatalf("warm MirrorEntries: %v", err)
	}
	allocs := testing.AllocsPerRun(500, func() {
		dst, err = s.MirrorEntries(dst)
	})
	if err != nil {
		t.Fatalf("MirrorEntries after alloc run: %v", err)
	}
	t.Logf("FSV mirror entries zero-alloc: %d live slots -> %d entries, allocs/op=%v first=%+v last=%+v", n, len(dst), allocs, dst[0], dst[len(dst)-1])
	if allocs != 0 {
		t.Fatalf("MirrorEntries allocates %v/op, want 0", allocs)
	}
	if len(dst) != n || dst[0] != (MirrorEntry{Key: 1, Model: 1}) || dst[n-1] != (MirrorEntry{Key: n, Model: 5}) {
		t.Fatalf("entries after alloc run wrong: len=%d first=%+v last=%+v", len(dst), dst[0], dst[n-1])
	}
}

func TestInterpolateIgnoresSnapshotIdentityColumnsFSV(t *testing.T) {
	prev := snap1(10, 0, 0, 0, 0, true, false)
	prev.EntityKey = []uint32{77}
	prev.Model = []ModelID{1}
	curr := snap1(11, 100, 200, 40, 8, true, false)
	curr.EntityKey = []uint32{77}
	curr.Model = []ModelID{2}

	var buf InterpBuffer
	Interpolate(&buf, prev, curr, 0.5)
	t.Logf("FSV interpolate identity isolation: prevModel=%d currModel=%d after=(%.0f,%.0f,%.0f,%.0f) active=%v", prev.Model[0], curr.Model[0], buf.X[0], buf.Y[0], buf.Z[0], buf.Facing[0], buf.Active[0])
	if buf.X[0] != 50 || buf.Y[0] != 100 || buf.Z[0] != 20 || buf.Facing[0] != 4 || !buf.Active[0] {
		t.Fatalf("interpolation changed under identity/model columns: buf=%+v", buf)
	}
	if prev.Model[0] != 1 || curr.Model[0] != 2 {
		t.Fatalf("interpolation mutated model columns: prev=%d curr=%d", prev.Model[0], curr.Model[0])
	}
}

func TestInterpolateMidpointFSV(t *testing.T) {
	prev := snap1(10, 0, 0, 0, 0, true, false)
	curr := snap1(11, 100, 200, 40, 8, true, false)
	var buf InterpBuffer
	for _, tc := range []struct {
		alpha          float32
		wx, wy, wz, wf float32
	}{
		{0, 0, 0, 0, 0},
		{0.5, 50, 100, 20, 4},
		{1, 100, 200, 40, 8},
	} {
		Interpolate(&buf, prev, curr, tc.alpha)
		t.Logf("FSV lerp alpha=%.1f -> (%.1f,%.1f,%.1f) facing=%.1f", tc.alpha, buf.X[0], buf.Y[0], buf.Z[0], buf.Facing[0])
		if buf.X[0] != tc.wx || buf.Y[0] != tc.wy || buf.Z[0] != tc.wz || buf.Facing[0] != tc.wf {
			t.Fatalf("alpha %.1f got (%v,%v,%v,%v) want (%v,%v,%v,%v)", tc.alpha, buf.X[0], buf.Y[0], buf.Z[0], buf.Facing[0], tc.wx, tc.wy, tc.wz, tc.wf)
		}
	}
}

func TestInterpolateSnapTeleportFSV(t *testing.T) {
	prev := snap1(10, 0, 0, 0, 0, true, false)
	curr := snap1(11, 5000, 6000, 0, 0, true, true) // Snap=true (teleport)
	var buf InterpBuffer
	for _, alpha := range []float32{0, 0.25, 0.5, 0.99, 1} {
		Interpolate(&buf, prev, curr, alpha)
		t.Logf("FSV teleport alpha=%.2f -> (%.0f,%.0f)", alpha, buf.X[0], buf.Y[0])
		if buf.X[0] != 5000 || buf.Y[0] != 6000 {
			t.Fatalf("snap should jump to curr at alpha %.2f, got (%v,%v)", alpha, buf.X[0], buf.Y[0])
		}
	}
}

func TestInterpolateSpawnFSV(t *testing.T) {
	// prev has the slot absent (a spawn this tick): no lerp from origin.
	prev := snap1(10, 0, 0, 0, 0, false, false)
	curr := snap1(11, 300, 400, 0, 0, true, true)
	var buf InterpBuffer
	Interpolate(&buf, prev, curr, 0.5)
	t.Logf("FSV spawn alpha=0.5 -> (%.0f,%.0f) active=%v", buf.X[0], buf.Y[0], buf.Active[0])
	if !buf.Active[0] || buf.X[0] != 300 || buf.Y[0] != 400 {
		t.Fatalf("spawn must render at spawn point, got (%v,%v) active=%v", buf.X[0], buf.Y[0], buf.Active[0])
	}
}

func TestInterpolatePausedFSV(t *testing.T) {
	// Same snapshot twice (sim paused): position frozen, alpha irrelevant.
	s := snap1(10, 123, 456, 0, 0, true, false)
	var buf InterpBuffer
	for _, alpha := range []float32{0, 0.5, 1} {
		Interpolate(&buf, s, s, alpha)
		if buf.X[0] != 123 || buf.Y[0] != 456 {
			t.Fatalf("paused frozen failed at alpha %.1f: (%v,%v)", alpha, buf.X[0], buf.Y[0])
		}
	}
	t.Logf("FSV paused frozen at (123,456) across all alpha")
}

func TestInterpolateDeathFSV(t *testing.T) {
	prev := snap1(10, 10, 10, 0, 0, true, false)
	curr := snap1(11, 10, 10, 0, 0, false, true) // absent in curr => dead
	var buf InterpBuffer
	Interpolate(&buf, prev, curr, 0.5)
	t.Logf("FSV death -> active=%v", buf.Active[0])
	if buf.Active[0] {
		t.Fatalf("dead entity must be inactive in render buffer")
	}
}

func TestInterpolateClampFSV(t *testing.T) {
	prev := snap1(10, 0, 0, 0, 0, true, false)
	curr := snap1(11, 100, 0, 0, 0, true, false)
	var buf InterpBuffer
	Interpolate(&buf, prev, curr, -0.5) // clamps to 0 => prev
	got0 := buf.X[0]
	Interpolate(&buf, prev, curr, 2.0) // clamps to 1 => curr
	got1 := buf.X[0]
	t.Logf("FSV clamp alpha=-0.5->%.0f alpha=2.0->%.0f", got0, got1)
	if got0 != 0 || got1 != 100 {
		t.Fatalf("alpha clamp wrong: -0.5->%v (want 0), 2.0->%v (want 100)", got0, got1)
	}
}

func TestInterpolateZeroAllocFSV(t *testing.T) {
	const n = 500
	mk := func(base int32) *Snapshot {
		s := &Snapshot{
			X: make([]fixed.F64, n), Y: make([]fixed.F64, n), Z: make([]fixed.F64, n),
			Facing: make([]fixed.F64, n), Present: make([]bool, n), Snap: make([]bool, n),
		}
		for i := 0; i < n; i++ {
			s.X[i] = fixed.FromInt(base + int32(i))
			s.Y[i] = fixed.FromInt(base + int32(i*2))
			s.Present[i] = true
		}
		return s
	}
	prev, curr := mk(0), mk(100)
	var buf InterpBuffer
	Interpolate(&buf, prev, curr, 0.5) // warm the buffers
	allocs := testing.AllocsPerRun(500, func() {
		Interpolate(&buf, prev, curr, 0.5)
	})
	t.Logf("FSV 500-mover interpolate allocs/op = %v (lastX0=%.1f)", allocs, buf.X[0])
	if allocs != 0 {
		t.Fatalf("sync path allocates %v/op for 500 movers, want 0", allocs)
	}
	// Spot-check a couple of interpolated slots against the fixed-point truth.
	if buf.X[0] != 50 || buf.X[10] != 60 {
		t.Fatalf("interpolated positions wrong: X0=%v X10=%v", buf.X[0], buf.X[10])
	}
}
