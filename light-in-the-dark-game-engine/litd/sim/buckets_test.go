package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// bucketMembers walks one bucket's intrusive list.
func bucketMembers(w *World, b int32) []EntityID {
	var out []EntityID
	for e := w.bucketHead[b]; e != -1; e = w.bucketNext[e] {
		out = append(out, w.bucketID[e])
	}
	return out
}

// Issue edge 4: a unit at a bucket boundary moves one cell over — the
// reconcile pass re-files it, membership verified before and after,
// and the unit stays findable by a scan.
func TestBucketBoundaryMove(t *testing.T) {
	w := NewWorld(Caps{})
	// 511.0 → bucket x=0; 513.0 → bucket x=1 (512-wu cells)
	id, ok := w.CreateUnit(fixed.Vec2{X: 511 * fixed.One, Y: 100 * fixed.One}, 0)
	if !ok {
		t.Fatal("create")
	}
	b0 := bucketOfPos(fixed.Vec2{X: 511 * fixed.One, Y: 100 * fixed.One})
	b1 := bucketOfPos(fixed.Vec2{X: 513 * fixed.One, Y: 100 * fixed.One})
	if b0 == b1 {
		t.Fatalf("test positions must straddle a boundary (got bucket %d twice)", b0)
	}
	t.Logf("BEFORE: bucketCell=%d members(b0=%d)=%v members(b1=%d)=%v",
		w.bucketCell[id.Index()], b0, bucketMembers(w, b0), b1, bucketMembers(w, b1))
	if w.bucketCell[id.Index()] != b0 {
		t.Fatalf("initial bucket = %d, want %d", w.bucketCell[id.Index()], b0)
	}

	// cross the boundary the way the sim does: position write + the
	// movement-phase reconcile
	w.Transforms.Pos[w.Transforms.Row(id)] = fixed.Vec2{X: 513 * fixed.One, Y: 100 * fixed.One}
	w.bucketReconcile()

	t.Logf("AFTER:  bucketCell=%d members(b0)=%v members(b1)=%v",
		w.bucketCell[id.Index()], bucketMembers(w, b0), bucketMembers(w, b1))
	if w.bucketCell[id.Index()] != b1 {
		t.Fatalf("post-move bucket = %d, want %d", w.bucketCell[id.Index()], b1)
	}
	if len(bucketMembers(w, b0)) != 0 || len(bucketMembers(w, b1)) != 1 {
		t.Fatal("stale membership after boundary move")
	}
}

// Destroy unlinks from the middle of a bucket list without breaking it.
func TestBucketDestroyUnlinks(t *testing.T) {
	w := NewWorld(Caps{})
	pos := fixed.Vec2{X: 100 * fixed.One, Y: 100 * fixed.One}
	var ids []EntityID
	for i := 0; i < 3; i++ {
		id, ok := w.CreateUnit(pos, 0)
		if !ok {
			t.Fatal("create")
		}
		ids = append(ids, id)
	}
	b := bucketOfPos(pos)
	t.Logf("before destroy: %v", bucketMembers(w, b))
	w.DestroyUnit(ids[1]) // middle of the LIFO list
	after := bucketMembers(w, b)
	t.Logf("after destroying %d: %v", ids[1], after)
	if len(after) != 2 || after[0] != ids[2] || after[1] != ids[0] {
		t.Fatalf("list broken after middle unlink: %v", after)
	}
}

// Off-map positions clamp into border buckets — still findable, never
// out of bounds.
func TestBucketOffMapClamp(t *testing.T) {
	w := NewWorld(Caps{})
	id, ok := w.CreateUnit(fixed.Vec2{X: -50 * fixed.One, Y: 99999 * fixed.One}, 0)
	if !ok {
		t.Fatal("create")
	}
	b := w.bucketCell[id.Index()]
	bx, by := b%BucketGridSize, b/BucketGridSize
	t.Logf("off-map (-50, 99999) filed in bucket %d (x=%d y=%d)", b, bx, by)
	if bx != 0 || by != BucketGridSize-1 {
		t.Fatalf("expected border bucket (0, %d), got (%d, %d)", BucketGridSize-1, bx, by)
	}
}
