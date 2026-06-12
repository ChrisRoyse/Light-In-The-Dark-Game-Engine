package sim

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// Edge 1: touching the same doodad twice promotes once and returns
// the same EntityID.
func TestDoodadPromotionIdempotent(t *testing.T) {
	w := NewWorld(Caps{})
	id1, ok1 := w.PromoteDoodad(7)
	id2, ok2 := w.PromoteDoodad(7)
	t.Logf("first touch: id=%08x ok=%v; second touch: id=%08x ok=%v; promoted count=%d",
		uint32(id1), ok1, uint32(id2), ok2, w.Doodads.Count())
	if !ok1 || !ok2 || id1 != id2 || w.Doodads.Count() != 1 {
		t.Fatalf("idempotence broken: %v %v %v count=%d", id1, id2, ok1, w.Doodads.Count())
	}
}

// Edge 2: 1,024 promotions fill the pool; the 1,025th fails with the
// assert, count pinned at cap.
func TestDoodadPromotionExhaustion(t *testing.T) {
	w := NewWorld(Caps{})
	asserts := 0
	w.Doodads.DebugAssert = func(msg string, placement int32) { asserts++ }
	for p := int32(0); p < 1024; p++ {
		if _, ok := w.PromoteDoodad(p); !ok {
			t.Fatalf("promotion %d failed below cap", p)
		}
	}
	id, ok := w.PromoteDoodad(9999)
	t.Logf("1,025th promotion: id=%d ok=%v; count=%d/%d; assert fired %d time(s)",
		id, ok, w.Doodads.Count(), 1024, asserts)
	if ok || w.Doodads.Count() != 1024 || asserts != 1 {
		t.Fatalf("exhaustion mishandled: ok=%v count=%d asserts=%d", ok, w.Doodads.Count(), asserts)
	}
}

// Edge 3: hiding an unpromoted doodad promotes it with the visibility
// applied — row absent before, present with Visible=false after.
func TestDoodadPromotionHideUnpromoted(t *testing.T) {
	w := NewWorld(Caps{})
	before := w.Doodads.PromotedRow(33)
	id, ok := w.ShowDoodad(33, false)
	r := w.Doodads.Row(id)
	t.Logf("row before: %d (absent); after hide: row=%d placement=%d visible=%v overrides=%02x entity=%08x",
		before, r, w.Doodads.Placement[r], w.Doodads.Visible[r], w.Doodads.Overrides[r], uint32(id))
	if before != -1 || !ok || r == -1 || w.Doodads.Visible[r] {
		t.Fatalf("hide-unpromoted wrong: before=%d ok=%v visible=%v", before, ok, w.Doodads.Visible[r])
	}
}

// Edge 4: promotion order is state — promoting A,B vs B,A assigns
// different EntityIDs (and different hashes), proving order matters
// and is therefore deterministic by construction (script order).
func TestDoodadPromotionOrderIsState(t *testing.T) {
	hashOf := func(w *World) uint64 {
		h := statehash.New()
		w.Doodads.HashInto(h)
		return h.Sum64()
	}
	w1 := NewWorld(Caps{})
	a1, _ := w1.PromoteDoodad(5)
	b1, _ := w1.PromoteDoodad(9)
	w2 := NewWorld(Caps{})
	b2, _ := w2.PromoteDoodad(9)
	a2, _ := w2.PromoteDoodad(5)
	t.Logf("order A,B: placement5=%08x placement9=%08x hash=%016x", uint32(a1), uint32(b1), hashOf(w1))
	t.Logf("order B,A: placement5=%08x placement9=%08x hash=%016x", uint32(a2), uint32(b2), hashOf(w2))
	if a1 == a2 || b1 == b2 {
		t.Fatalf("promotion order must change ID assignment: %v/%v vs %v/%v", a1, b1, a2, b2)
	}
	if hashOf(w1) == hashOf(w2) {
		t.Fatalf("promotion order must change the state hash")
	}
}

// Anim override sets the flag bit; hash covers row mutations.
func TestDoodadAnimOverrideAndHash(t *testing.T) {
	w := NewWorld(Caps{})
	id, _ := w.SetDoodadAnim(12, 3)
	r := w.Doodads.Row(id)
	if w.Doodads.Anim[r] != 3 || w.Doodads.Overrides[r]&DoodadOverrideAnim == 0 {
		t.Fatalf("anim override not applied: anim=%d ov=%02x", w.Doodads.Anim[r], w.Doodads.Overrides[r])
	}
	h1 := statehash.New()
	w.Doodads.HashInto(h1)
	pre := h1.Sum64()
	w.Doodads.Visible[r] = false
	h2 := statehash.New()
	w.Doodads.HashInto(h2)
	post := h2.Sum64()
	t.Logf("hash with anim override %016x -> after visibility flip %016x", pre, post)
	if pre == post {
		t.Fatalf("row mutation must change the doodad hash")
	}
}

// Zero-alloc steady state: repeated touches of promoted doodads.
func TestDoodadTouchZeroAlloc(t *testing.T) {
	w := NewWorld(Caps{})
	for p := int32(0); p < 64; p++ {
		w.PromoteDoodad(p)
	}
	allocs := testing.AllocsPerRun(200, func() {
		for p := int32(0); p < 64; p++ {
			id, _ := w.PromoteDoodad(p) // all idempotent re-touches
			w.Doodads.Visible[w.Doodads.Row(id)] = true
		}
	})
	t.Logf("AllocsPerRun(64 re-touches) = %v", allocs)
	if allocs != 0 {
		t.Fatalf("re-touch allocated: %v", allocs)
	}
}
