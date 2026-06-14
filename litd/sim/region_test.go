package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func fv(x, y float64) fixed.Vec2 {
	return fixed.Vec2{X: fixed.F64(int64(x) << 32), Y: fixed.F64(int64(y) << 32)}
}
func ff(v float64) fixed.F64 { return fixed.F64(int64(v) << 32) }

// TestRegionContainmentFSV — happy path + edge audit on the cell store.
// SoT: ContainsPoint after known adds, plus the popcount of set cells.
func TestRegionContainmentFSV(t *testing.T) {
	rs := NewRegionStore(64, 256)
	id, gen := rs.NewRegion()
	t.Logf("FSV region created id=%d gen=%d alive=%v popcount(before)=%d",
		id, gen, rs.Alive(id, gen), rs.live(id, gen).popcount())

	// Add a rect [0,0]-[64,64]: covers cells (0,0)(1,0)(0,1)(1,1) at 32-wu
	// cells = 4 cells (0..64 inclusive spans cell 0 and cell 2? cell of 64
	// is 64>>5=2). [0,64] -> cell x in {0,1,2}; 3x3 = 9 cells.
	rs.AddRect(id, gen, ff(0), ff(0), ff(64), ff(64))
	pc := rs.live(id, gen).popcount()
	t.Logf("FSV AddRect [0,0]-[64,64]: popcount=%d (cells x,y in 0..2 -> 9)", pc)
	if pc != 9 {
		t.Fatalf("rect cell count = %d, want 9", pc)
	}

	// Edge audit on containment:
	cases := []struct {
		name string
		p    fixed.Vec2
		want bool
	}{
		{"origin", fv(0, 0), true},
		{"inside", fv(40, 40), true},
		{"on rect cell (64,64)", fv(64, 64), true},
		{"just past (96,0) cell 3", fv(96, 0), false},
		{"negative clamps to cell 0 (inside)", fv(-10, -10), true},
		{"far outside", fv(5000, 5000), false},
	}
	for _, c := range cases {
		got := rs.ContainsPoint(id, gen, c.p)
		t.Logf("FSV Contains %-32s p=(%d,%d) -> %v (want %v)",
			c.name, int64(c.p.X)>>32, int64(c.p.Y)>>32, got, c.want)
		if got != c.want {
			t.Fatalf("Contains[%s] = %v, want %v", c.name, got, c.want)
		}
	}

	// AddCell + ClearCell single-cell ops.
	rs.AddCell(id, gen, fv(200, 200))
	if !rs.ContainsPoint(id, gen, fv(200, 200)) {
		t.Fatalf("AddCell(200,200) not contained")
	}
	rs.ClearCell(id, gen, fv(200, 200))
	if rs.ContainsPoint(id, gen, fv(200, 200)) {
		t.Fatalf("ClearCell(200,200) still contained")
	}

	// ClearRect removes the original block.
	rs.ClearRect(id, gen, ff(0), ff(0), ff(64), ff(64))
	if pc := rs.live(id, gen).popcount(); pc != 0 {
		t.Fatalf("after ClearRect popcount=%d, want 0", pc)
	}
}

// TestRegionStaleHandleFSV — Remove frees the slot under a bumped
// generation; a stale handle is invalid and its ops are no-ops, never
// aliasing the live region that reuses the slot. SoT: Alive + Contains.
func TestRegionStaleHandleFSV(t *testing.T) {
	rs := NewRegionStore(64, 256)
	id1, gen1 := rs.NewRegion()
	rs.AddRect(id1, gen1, ff(0), ff(0), ff(32), ff(32))
	rs.Remove(id1, gen1)
	t.Logf("FSV after Remove: alive(id1,gen1)=%v", rs.Alive(id1, gen1))
	if rs.Alive(id1, gen1) {
		t.Fatalf("removed region still alive")
	}

	id2, gen2 := rs.NewRegion() // reuses slot id1
	t.Logf("FSV recycle: id1=%d gen1=%d  id2=%d gen2=%d", id1, gen1, id2, gen2)
	if id2 != id1 {
		t.Fatalf("expected slot reuse: id1=%d id2=%d", id1, id2)
	}
	if gen2 == gen1 {
		t.Fatalf("generation not bumped on reuse: both %d", gen1)
	}
	// Fresh region starts empty (cells reset), not inheriting id1's rect.
	if rs.ContainsPoint(id2, gen2, fv(0, 0)) {
		t.Fatalf("recycled region inherited old cells (aliasing)")
	}
	// Stale ops are no-ops on the live region.
	rs.AddRect(id1, gen1, ff(0), ff(0), ff(1000), ff(1000)) // stale: ignored
	if rs.ContainsPoint(id2, gen2, fv(100, 100)) {
		t.Fatalf("stale AddRect wrote into the live recycled region")
	}
}

// TestRegionSaveLoadFSV — regions survive a save/load round-trip and the
// state hash is identical before and after (regions are hashed gameplay
// state). SoT: the post-load containment + the two hashes.
func TestRegionSaveLoadFSV(t *testing.T) {
	w := NewWorld(Caps{})
	r1, g1 := w.Regions.NewRegion()
	r2, g2 := w.Regions.NewRegion()
	w.Regions.AddRect(r1, g1, ff(64), ff(64), ff(192), ff(192))
	w.Regions.AddCell(r2, g2, fv(1000, 1000))
	// Remove a third to exercise the free list across save.
	r3, g3 := w.Regions.NewRegion()
	w.Regions.Remove(r3, g3)

	reg := NewHashRegistry()
	var snapA, snapB statehash.Snapshot
	hA := w.HashState(reg, &snapA).Top

	var buf bytes.Buffer
	if err := w.SaveState(&buf, 0); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := NewWorld(Caps{})
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("load: %v", err)
	}
	hB := w2.HashState(reg, &snapB).Top

	t.Logf("FSV region save/load: r1 contains(100,100) pre=%v post=%v ; r2 contains(1000,1000) post=%v",
		w.Regions.ContainsPoint(r1, g1, fv(100, 100)),
		w2.Regions.ContainsPoint(r1, g1, fv(100, 100)),
		w2.Regions.ContainsPoint(r2, g2, fv(1000, 1000)))
	t.Logf("FSV state hash before=%016x after=%016x freelist(post)=%d", hA, hB, len(w2.Regions.free))

	if !w2.Regions.ContainsPoint(r1, g1, fv(100, 100)) {
		t.Fatalf("r1 rect lost across save/load")
	}
	if !w2.Regions.ContainsPoint(r2, g2, fv(1000, 1000)) {
		t.Fatalf("r2 cell lost across save/load")
	}
	if w2.Regions.Alive(r3, g3) {
		t.Fatalf("removed r3 came back alive after load")
	}
	if hA != hB {
		t.Fatalf("state hash diverged across save/load: %016x != %016x", hA, hB)
	}
	// Determinism: the free list must reproduce so the next NewRegion picks
	// the same slot on both worlds.
	idA, _ := w.Regions.NewRegion()
	idB, _ := w2.Regions.NewRegion()
	if idA != idB {
		t.Fatalf("post-load region allocation diverged: %d vs %d", idA, idB)
	}
}
