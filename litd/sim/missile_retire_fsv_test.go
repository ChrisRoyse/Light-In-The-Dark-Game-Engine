package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// Manual FSV (#590 missiles-retire): the SoT is (1) the HashSystems vocabulary —
// "missiles" must be absent — and (2) a live projectile surviving a v48 save/load
// round-trip with a byte-identical state hash. Synthetic: spawn a homing missile,
// step it into flight, snapshot the hash, save, load into a fresh world, re-hash.
func TestMissileRetireFSV(t *testing.T) {
	// (1) SoT: the hash vocabulary no longer contains "missiles".
	if contains(HashSystems, "missiles") {
		t.Fatalf(`"missiles" still present in HashSystems (retire #590 incomplete)`)
	}
	t.Logf("FSV vocab: %d systems, 'missiles' absent, 'movers' present=%v", len(HashSystems), contains(HashSystems, "movers"))

	// (2) Synthetic projectile + save/load round-trip.
	w := NewWorld(Caps{})
	w.BindDamageMatrix([][]int32{{1000}})
	mk := func(team uint8, x, y int32) EntityID {
		id, _ := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)}, 0)
		w.Owners.Add(w.Ents, id, team, team, team)
		w.Healths.Add(w.Ents, id, 100*fixed.One, 0, 0, 0)
		return id
	}
	src := mk(0, 1000, 1000)
	tgt := mk(1, 4000, 1000)
	id, ok := w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: fixed.FromInt(1000), Y: fixed.FromInt(1000)},
		Source: src, Target: tgt, Speed: 50 * fixed.One,
	})
	if !ok {
		t.Fatal("SpawnMissile failed")
	}
	for i := 0; i < 3; i++ {
		w.Step()
	}
	mr, isProj := w.ProjMover(id)
	t.Logf("BEFORE save: projectiles=%d, projMover ok=%v row=%d, body pos=%v",
		w.ProjRender.Count(), isProj, mr, projPos(w, id))

	reg := NewHashRegistry()
	var snapA statehash.Snapshot
	w.HashState(reg, &snapA)
	h1 := snapA.Top

	var buf bytes.Buffer
	if err := w.SaveState(&buf, 0); err != nil {
		t.Fatalf("save: %v", err)
	}
	t.Logf("saved %d bytes at format v%d", buf.Len(), SaveFormatVersion)

	w2 := NewWorld(Caps{})
	w2.BindDamageMatrix([][]int32{{1000}})
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), 0); err != nil {
		t.Fatalf("load: %v", err)
	}
	reg2 := NewHashRegistry()
	var snapB statehash.Snapshot
	w2.HashState(reg2, &snapB)
	h2 := snapB.Top

	t.Logf("AFTER load:  projectiles=%d, hash before=%#x after=%#x", w2.ProjRender.Count(), h1, h2)
	if h1 != h2 {
		t.Fatalf("save/load hash mismatch: before %#x != after %#x (v48 round-trip broken)", h1, h2)
	}
	if w2.ProjRender.Count() != w.ProjRender.Count() {
		t.Fatalf("projectile count not preserved: before %d != after %d", w.ProjRender.Count(), w2.ProjRender.Count())
	}
}

func contains(ss []string, s string) bool {
	for _, x := range ss {
		if x == s {
			return true
		}
	}
	return false
}

func projPos(w *World, id EntityID) fixed.Vec2 {
	if tr := w.Transforms.Row(id); tr != -1 {
		return w.Transforms.Pos[tr]
	}
	return fixed.Vec2{}
}
