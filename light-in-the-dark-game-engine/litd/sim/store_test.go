package sim

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func storeTables(s *TransformStore, entityCap int) string {
	var sb strings.Builder
	sb.WriteString("rows: ")
	for r := int32(0); r < s.Count(); r++ {
		fmt.Fprintf(&sb, "[r%d e%d pos=%d] ", r, s.Entity[r].Index(), s.Pos[r].X.Floor())
	}
	sb.WriteString("| rowOf: ")
	for i := 0; i < entityCap; i++ {
		fmt.Fprintf(&sb, "%d:%d ", i, s.Row(makeEntityID(uint32(i), 0)))
	}
	return sb.String()
}

func newStoreWorld(t *testing.T, n int) (*Entities, *TransformStore, []EntityID) {
	t.Helper()
	e := NewEntities(8)
	s := NewTransformStore(8, 8)
	ids := make([]EntityID, n)
	for i := range ids {
		id, ok := e.Create()
		if !ok {
			t.Fatal("create failed")
		}
		ids[i] = id
		if !s.Add(e, id, fixed.Vec2{X: fixed.FromInt(int32(10 + i)), Y: 0}, 0) {
			t.Fatal("add failed")
		}
	}
	return e, s, ids
}

// Edge: remove a MIDDLE row — last row swaps in, moved entity's rowOf
// is fixed; full tables printed before/after.
func TestTransformStoreRemoveMiddle(t *testing.T) {
	_, s, ids := newStoreWorld(t, 4)
	t.Logf("before: %s", storeTables(s, 4))
	if !s.Remove(ids[1]) {
		t.Fatal("remove failed")
	}
	t.Logf("after:  %s", storeTables(s, 4))
	if s.Count() != 3 {
		t.Fatalf("count %d want 3", s.Count())
	}
	// last entity (ids[3], pos 13) must now sit in row 1 with rowOf fixed
	if s.Entity[1] != ids[3] || s.Pos[1].X != fixed.FromInt(13) {
		t.Fatalf("swap-with-last wrong: row1 entity=%d pos=%d", s.Entity[1].Index(), s.Pos[1].X.Floor())
	}
	if s.Row(ids[3]) != 1 {
		t.Fatalf("moved entity rowOf not fixed: %d", s.Row(ids[3]))
	}
	if s.Row(ids[1]) != -1 {
		t.Fatalf("removed entity still has row %d", s.Row(ids[1]))
	}
}

// Edge: remove the LAST row — no swap, just count--.
func TestTransformStoreRemoveLast(t *testing.T) {
	_, s, ids := newStoreWorld(t, 3)
	t.Logf("before: %s", storeTables(s, 3))
	if !s.Remove(ids[2]) {
		t.Fatal("remove failed")
	}
	t.Logf("after:  %s", storeTables(s, 3))
	if s.Count() != 2 || s.Row(ids[2]) != -1 {
		t.Fatalf("count=%d rowOf=%d", s.Count(), s.Row(ids[2]))
	}
	if s.Entity[0] != ids[0] || s.Entity[1] != ids[1] {
		t.Fatal("surviving rows disturbed by last-row removal")
	}
}

// Edge: Add for an entity that already has the component fails closed
// and trips the debug assert.
func TestTransformStoreDoubleAddAsserts(t *testing.T) {
	e, s, ids := newStoreWorld(t, 2)
	asserts := []string{}
	s.DebugAssert = func(msg string, id EntityID) {
		asserts = append(asserts, fmt.Sprintf("%s (idx %d)", msg, id.Index()))
	}
	ok := s.Add(e, ids[0], fixed.Vec2{}, 0)
	t.Logf("double Add -> ok=%v asserts=%v; state: %s", ok, asserts, storeTables(s, 2))
	if ok || s.Count() != 2 {
		t.Fatalf("double Add mutated store: ok=%v count=%d", ok, s.Count())
	}
	if len(asserts) != 1 || !strings.Contains(asserts[0], "double Add") {
		t.Fatalf("debug assert wrong: %v", asserts)
	}

	// Add on a dead entity also fails closed
	e.Destroy(ids[1])
	if s.Add(e, ids[1], fixed.Vec2{}, 0) {
		t.Fatal("Add on dead entity succeeded")
	}
}

// Edge: 100 interleaved add/removes — final row order identical
// across two independent runs (deterministic history ⇒ deterministic
// order).
func TestTransformStoreInterleavedDeterministic(t *testing.T) {
	runOnce := func() string {
		e := NewEntities(64)
		s := NewTransformStore(64, 64)
		live := []EntityID{}
		for i := 0; i < 100; i++ {
			if i%3 == 2 && len(live) > 0 {
				victim := live[i%len(live)]
				s.Remove(victim)
				e.Destroy(victim)
				live = append(live[:i%len(live)], live[i%len(live)+1:]...)
			} else {
				id, _ := e.Create()
				s.Add(e, id, fixed.Vec2{X: fixed.FromInt(int32(i))}, 0)
				live = append(live, id)
			}
		}
		var sb strings.Builder
		for r := int32(0); r < s.Count(); r++ {
			fmt.Fprintf(&sb, "%d/%d ", s.Entity[r].Index(), s.Pos[r].X.Floor())
		}
		return sb.String()
	}
	a, b := runOnce(), runOnce()
	t.Logf("run A rows: %s", a)
	t.Logf("run B rows: %s", b)
	if a != b {
		t.Fatal("row order diverged between identical histories")
	}
}

func TestTransformStoreZeroAlloc(t *testing.T) {
	e := NewEntities(1024)
	s := NewTransformStore(1024, 1024)
	ids := make([]EntityID, 512)
	for i := range ids {
		ids[i], _ = e.Create()
		s.Add(e, ids[i], fixed.Vec2{}, 0)
	}
	if n := testing.AllocsPerRun(5000, func() {
		s.Remove(ids[100])
		s.Add(e, ids[100], fixed.Vec2{X: fixed.One}, 0)
		var acc fixed.F64
		for r := int32(0); r < s.Count(); r++ {
			acc = acc.Add(s.Pos[r].X)
		}
		_ = acc
	}); n != 0 {
		t.Fatalf("add/remove/iterate allocates %v/op; R-GC-1 requires 0", n)
	}
	t.Log("AllocsPerRun = 0 for Add+Remove+full iteration (512 rows)")
}

func BenchmarkTransformIter(b *testing.B) {
	e := NewEntities(4096)
	s := NewTransformStore(4000, 4096)
	for i := 0; i < 4000; i++ {
		id, _ := e.Create()
		s.Add(e, id, fixed.Vec2{X: fixed.FromInt(int32(i)), Y: fixed.One}, 0)
	}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		var acc fixed.F64
		for r := int32(0); r < s.Count(); r++ {
			acc = acc.Add(s.Pos[r].X)
		}
		_ = acc
	}
}
