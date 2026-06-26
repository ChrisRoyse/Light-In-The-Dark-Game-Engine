package sim

import (
	"bytes"
	"io"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// Regression for #630: a corrupt/divergent save with an out-of-range or
// duplicate mover slot must be REJECTED by validateSave (fail-closed), never
// reach applySave (which indexes mv.*[slot] raw → OOB panic or sentinel
// corruption). Mirrors the existing timer/group partition guards.
//
// Strategy: SaveState a real world holding a live mover, re-decode the bytes to
// a staged decodedSave (no apply), corrupt only the mover slot, then call
// validateSave directly — the exact unit under repair, no fragile byte surgery.

// decodeStaged replays LoadState's header read then decodeBody, returning the
// staged save WITHOUT applying it.
func decodeStaged(t *testing.T, w *World, blob []byte) *decodedSave {
	t.Helper()
	br := bytes.NewReader(blob)
	magic := make([]byte, len(SaveMagic))
	if _, err := io.ReadFull(br, magic); err != nil {
		t.Fatalf("read magic: %v", err)
	}
	if string(magic) != SaveMagic {
		t.Fatalf("bad magic %q", magic)
	}
	r := &saveReader{r: br, what: "header"}
	r.u32()      // format version
	r.u64()      // data-table fingerprint
	for i := 0; i < 19; i++ { // the 19 Caps fields, in LoadState order
		r.u32()
	}
	var d decodedSave
	d.tick = r.u32()
	d.unitCount = r.u32()
	r.u64() // rng State
	r.u64() // rng Inc
	if r.err != nil {
		t.Fatalf("header read: %v", r.err)
	}
	if err := decodeBody(r, &d, w); err != nil {
		t.Fatalf("decodeBody: %v", err)
	}
	return &d
}

func saveWorldWithMover(t *testing.T) ([]byte, *World) {
	t.Helper()
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
	if _, ok := w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: fixed.FromInt(1000), Y: fixed.FromInt(1000)},
		Source: src, Target: tgt, Speed: 50 * fixed.One,
	}); !ok {
		t.Fatal("SpawnMissile failed")
	}
	w.Step() // mover now live
	if w.Movers.count == 0 {
		t.Fatal("expected a live mover after spawn+step")
	}
	var buf bytes.Buffer
	if err := w.SaveState(&buf, 0); err != nil {
		t.Fatalf("save: %v", err)
	}
	return buf.Bytes(), w
}

func TestMoverLoadValidatesSlotFSV(t *testing.T) {
	blob, _ := saveWorldWithMover(t)
	fresh := func() *World {
		w := NewWorld(Caps{})
		w.BindDamageMatrix([][]int32{{1000}})
		return w
	}
	moverCap := fresh().Movers.Cap()

	// Baseline: the un-corrupted decode passes validateSave (proves the test
	// scaffold and the new check don't reject a legitimate save).
	if d := decodeStaged(t, fresh(), blob); validateSave(d, fresh()) != nil {
		t.Fatalf("baseline valid save rejected: %v", validateSave(d, fresh()))
	}
	t.Logf("BEFORE: baseline save (moverCap=%d, 1 live mover) validates clean", moverCap)

	cases := []struct {
		name   string
		mutate func(d *decodedSave)
	}{
		{"slot above cap", func(d *decodedSave) { d.moverRows[0].slot = int32(moverCap + 5) }},
		{"slot zero (reserved sentinel)", func(d *decodedSave) { d.moverRows[0].slot = 0 }},
		{"slot negative", func(d *decodedSave) { d.moverRows[0].slot = -1 }},
		{"live slot duplicated into free list", func(d *decodedSave) {
			d.moverFree = append(d.moverFree, uint32(d.moverRows[0].slot))
			d.moverFreeGen = append(d.moverFreeGen, 0)
		}},
	}
	for _, c := range cases {
		d := decodeStaged(t, fresh(), blob)
		c.mutate(d)
		err := validateSave(d, fresh())
		t.Logf("AFTER mutate %q -> validateSave err=%v", c.name, err)
		if err == nil {
			t.Fatalf("corrupt mover save %q passed validateSave (fail-OPEN); applySave would panic/corrupt (#630)", c.name)
		}
	}
}
