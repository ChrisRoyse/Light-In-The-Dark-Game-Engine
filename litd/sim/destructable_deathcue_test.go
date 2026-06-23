package sim

// #72 destructable death render cue FSV. SoT = the published Snapshot.Events read
// directly off w.Snaps after a KillDestructable: a live destructable's death must
// stage exactly one RenderDestructableDeath cue carrying its handle, type id, and
// world position — the signal render needs to rebuild the owning terrain chunk
// (drop the merged doodad mesh) and play a death burst. Fail-closed: re-killing a
// dead destructable emits nothing. Plus determinism-inert: draining the cue on
// publish never perturbs the state hash (presentation cues are non-hashing).

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func countDeathCues(w *World) int {
	n := 0
	for _, ev := range w.Snaps.Curr().Events {
		if ev.Kind == RenderDestructableDeath {
			n++
		}
	}
	return n
}

func TestDestructableDeathCueFSV(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	w.SetGrid(destGrid())
	const cx, cy = 30, 30
	pos := cellPos(cx, cy)
	const typ = uint16(7)

	id := w.CreateDestructable(typ, pos, 0, 100, true, 1)
	if id == 0 {
		t.Fatal("CreateDestructable failed")
	}

	// No death cue before the kill.
	w.publishSnapshot()
	if n := countDeathCues(w); n != 0 {
		t.Fatalf("death cue present before any kill: %d", n)
	}

	if !w.KillDestructable(id) {
		t.Fatal("Kill should succeed on a live destructable")
	}
	w.publishSnapshot()

	// SoT: exactly one death cue, identifying the destructable, its type, its pos.
	var cue RenderEvent
	found := 0
	for _, ev := range w.Snaps.Curr().Events {
		if ev.Kind == RenderDestructableDeath {
			cue, found = ev, found+1
		}
	}
	t.Logf("FSV death cue: found=%d ent=%d data(type)=%d pos=(%d,%d)",
		found, cue.Ent, cue.Data, cue.Pos.X.Floor(), cue.Pos.Y.Floor())
	if found != 1 {
		t.Fatalf("want exactly 1 death cue, got %d", found)
	}
	if cue.Ent != id {
		t.Fatalf("cue Ent = %d, want destructable %d", cue.Ent, id)
	}
	if cue.Data != typ {
		t.Fatalf("cue Data(type) = %d, want %d", cue.Data, typ)
	}
	if cue.Pos != pos {
		t.Fatalf("cue Pos %v != destructable pos %v", cue.Pos, pos)
	}

	// Fail-closed: re-killing a dead destructable is a no-op and emits no cue.
	if w.KillDestructable(id) {
		t.Fatal("second Kill on a dead destructable returned true")
	}
	w.publishSnapshot()
	if n := countDeathCues(w); n != 0 {
		t.Fatalf("re-kill emitted a cue: %d (death cue must be once-per-death)", n)
	}
}

func TestDestructableDeathCueDeterminismInertFSV(t *testing.T) {
	w := NewWorld(Caps{Units: 8})
	w.SetGrid(destGrid())
	id := w.CreateDestructable(7, cellPos(30, 30), 0, 100, true, 1)
	if !w.KillDestructable(id) {
		t.Fatal("kill failed")
	}

	// The staged death cue is render-only: draining it on publish must not perturb
	// the state hash. Hash, publish (drains the cue into the snapshot), hash again.
	reg := NewHashRegistry()
	var s1, s2 statehash.Snapshot
	h1 := w.HashState(reg, &s1).Top
	w.publishSnapshot()
	h2 := w.HashState(reg, &s2).Top
	t.Logf("FSV determinism-inert: hash before publish=%#x after=%#x", h1, h2)
	if h1 != h2 {
		t.Fatalf("publishSnapshot perturbed the state hash %#x -> %#x (death cue must be render-only)", h1, h2)
	}
	// The cue is genuinely present, so the inertness check is not vacuous.
	if countDeathCues(w) != 1 {
		t.Fatalf("expected 1 published death cue for a meaningful check, got %d", countDeathCues(w))
	}
}
