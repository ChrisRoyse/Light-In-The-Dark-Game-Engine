package sim

// Spatial bucket grid (combat-and-orders.md §3.1): coarse candidate
// filtering for acquisition scans. World space is divided into
// 512-wu square cells — at least half the largest shipped acquisition
// range (archer 700), so a range-R scan touches the buckets of a
// bounding square only. Buckets are intrusive doubly-linked lists
// keyed by entity index: insert/remove/move are O(1), preallocated at
// NewWorld, zero alloc forever after (R-GC-1/2).
//
// The grid is DERIVED state — rebuildable from Transform positions —
// so it is excluded from the state hash. It is maintained
// incrementally: CreateUnit inserts, DestroyUnit removes, and the
// movement phase ends with a reconcile pass that re-files every
// transform whose bucket changed (covering waypoint integration,
// avoidance shoves and sidesteps alike — anything that wrote a
// position this tick).

import (
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

const (
	// bucketShift: 2^9 = 512 world units per bucket cell.
	bucketShift = 9
	// BucketGridSize is the bucket grid side: the 16,384-wu world
	// (path.GridSize pathing cells × 32 wu) at 512-wu buckets.
	BucketGridSize = (path.GridSize * 32) >> bucketShift
	bucketCount    = BucketGridSize * BucketGridSize
)

// bucketCoord clamps one world coordinate to a bucket axis index.
// Off-map positions file into the border bucket — they stay findable
// and the clamp is deterministic.
func bucketCoord(v fixed.F64) int32 {
	c := int32(v.Floor() >> bucketShift)
	if c < 0 {
		return 0
	}
	if c >= BucketGridSize {
		return BucketGridSize - 1
	}
	return c
}

func bucketOfPos(p fixed.Vec2) int32 {
	return bucketCoord(p.Y)*BucketGridSize + bucketCoord(p.X)
}

// bucketInsert files id into the bucket of pos. Double inserts are
// debug-assert violations (the membership contract is one node per
// live transform).
func (w *World) bucketInsert(id EntityID, pos fixed.Vec2) {
	idx := id.Index()
	if w.bucketCell[idx] != -1 {
		if w.Ents.DebugStaleHandle != nil {
			w.Ents.DebugStaleHandle(id)
		}
		return
	}
	b := bucketOfPos(pos)
	w.bucketCell[idx] = b
	w.bucketID[idx] = id
	head := w.bucketHead[b]
	w.bucketNext[idx] = head
	w.bucketPrev[idx] = -1
	if head != -1 {
		w.bucketPrev[head] = int32(idx)
	}
	w.bucketHead[b] = int32(idx)
}

// bucketRemove unlinks id; absent membership is a no-op (DestroyUnit
// runs on entities that may predate grid maintenance in tests).
func (w *World) bucketRemove(id EntityID) {
	idx := id.Index()
	b := w.bucketCell[idx]
	if b == -1 {
		return
	}
	next, prev := w.bucketNext[idx], w.bucketPrev[idx]
	if prev != -1 {
		w.bucketNext[prev] = next
	} else {
		w.bucketHead[b] = next
	}
	if next != -1 {
		w.bucketPrev[next] = prev
	}
	w.bucketCell[idx] = -1
	w.bucketID[idx] = 0
}

// bucketReconcile re-files every transform whose position left its
// bucket — the §3.1 "rebuilt incrementally in the movement phase"
// pass. O(transforms), zero alloc.
func (w *World) bucketReconcile() {
	t := w.Transforms
	for r := int32(0); r < t.count; r++ {
		id := t.Entity[r]
		idx := id.Index()
		b := bucketOfPos(t.Pos[r])
		if w.bucketCell[idx] == b {
			continue
		}
		w.bucketRemove(id)
		w.bucketInsert(id, t.Pos[r])
	}
}
