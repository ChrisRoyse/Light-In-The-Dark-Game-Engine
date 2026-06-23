package sim

// #309 clause 1 FSV — missile render publication. SoT = the published Snapshot
// read directly off w.Snaps after a Step: a spawned missile must appear in
// Snapshot.Missiles (with Pos matching its live Transforms row, the spec Arc, and
// the resolved guidance kind) and must NOT appear in Snapshot.Entries (it renders
// as an arced billboard, never a unit model), while real units stay in Entries.
// X+X=Y: a missile spawned with Arc=64 reads Arc=64 in the snapshot. Plus the
// determinism-inert proof: publishSnapshot (which now also fills Missiles) does
// not perturb the state hash — the render surface is never hashed.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func TestMissileSnapshotPublishFSV(t *testing.T) {
	w, a, v := msWorld(t) // 2 units: attacker team0, victim team1
	const arc = 64 * fixed.One
	mid, ok := w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
		Source: a, Target: v,
		Speed:      50 * fixed.One,
		Arc:        arc,
		GuidanceID: MissileGuidanceHoming,
		Packet:     DamagePacket{Source: a, Target: v, Amount: 10 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn missile failed")
	}

	w.Step() // advances + publishes (phase 7)
	snap := w.Snaps.Curr()

	// SoT 1: exactly one missile in the render snapshot, and it is our missile.
	t.Logf("FSV snapshot: %d entries, %d missiles", len(snap.Entries), len(snap.Missiles))
	if len(snap.Missiles) != 1 {
		t.Fatalf("snapshot has %d missiles, want 1", len(snap.Missiles))
	}
	me := snap.Missiles[0]
	if me.ID != mid {
		t.Fatalf("missile entry ID = %d, want %d", me.ID, mid)
	}
	// SoT 2: snapshot Pos cross-checks against the live Transforms row (same tick),
	// Arc is the spec value, guidance is homing.
	tr := w.Transforms.Row(mid)
	live := w.Transforms.Pos[tr]
	t.Logf("FSV missile entry: pos=(%d,%d) arc=%d guid=%d (live pos=(%d,%d))",
		me.Pos.X.Floor(), me.Pos.Y.Floor(), me.Arc.Floor(), me.GuidanceID,
		live.X.Floor(), live.Y.Floor())
	if me.Pos != live {
		t.Fatalf("snapshot missile pos %v != live transform pos %v", me.Pos, live)
	}
	if me.Arc != arc {
		t.Fatalf("snapshot missile Arc = %d, want %d", me.Arc.Floor(), arc.Floor())
	}
	if me.GuidanceID != MissileGuidanceHoming {
		t.Fatalf("snapshot missile guidance = %d, want homing %d", me.GuidanceID, MissileGuidanceHoming)
	}

	// SoT 3: the missile is NOT a unit entry; the two real units ARE.
	for _, e := range snap.Entries {
		if e.ID == mid {
			t.Fatal("missile leaked into Snapshot.Entries — would render as a unit model")
		}
	}
	if len(snap.Entries) != 2 {
		t.Fatalf("Entries = %d, want 2 (the two units, missile excluded)", len(snap.Entries))
	}
	var sawA, sawV bool
	for _, e := range snap.Entries {
		sawA = sawA || e.ID == a
		sawV = sawV || e.ID == v
	}
	if !sawA || !sawV {
		t.Fatalf("real units missing from Entries: sawA=%v sawV=%v", sawA, sawV)
	}
}

func TestMissileImpactCueFSV(t *testing.T) {
	w, a, v := msWorld(t) // victim at x=1400, attacker at x=1000
	mid, _ := w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
		Source: a, Target: v, Speed: 100 * fixed.One,
		Packet: DamagePacket{Source: a, Target: v, Amount: 10 * fixed.One},
	})
	// Capture the authoritative impact point from the sim callback (SoT to
	// cross-check against the render cue's Pos).
	var impactAt fixed.Vec2
	var impacted bool
	w.OnMissileImpact = func(_ uint32, _ EntityID, at fixed.Vec2, _ EntityID) {
		impactAt, impacted = at, true
	}

	var cue RenderEvent
	var found bool
	for i := 0; i < 10 && !impacted; i++ {
		w.Step()
		// The impact tick's snapshot carries the staged cue.
		for _, ev := range w.Snaps.Curr().Events {
			if ev.Kind == RenderMissileImpact {
				cue, found = ev, true
			}
		}
	}
	if !impacted {
		t.Fatal("missile never impacted")
	}
	t.Logf("FSV impact cue: found=%v kind=%d ent=%d pos=(%d,%d) impactAt=(%d,%d)",
		found, cue.Kind, cue.Ent, cue.Pos.X.Floor(), cue.Pos.Y.Floor(),
		impactAt.X.Floor(), impactAt.Y.Floor())
	if !found {
		t.Fatal("no RenderMissileImpact cue published for the impact")
	}
	// SoT: the cue identifies the missile and carries the exact impact point —
	// the position the render ImpactFXPool burst will spawn at.
	if cue.Ent != mid {
		t.Fatalf("cue Ent = %d, want missile %d", cue.Ent, mid)
	}
	if cue.Pos != impactAt {
		t.Fatalf("cue Pos %v != sim impact point %v", cue.Pos, impactAt)
	}
}

func TestMissileSnapshotDeterminismInertFSV(t *testing.T) {
	w, a, v := msWorld(t)
	w.SpawnMissile(MissileSpec{
		Pos:    fixed.Vec2{X: 1000 * fixed.One, Y: 1000 * fixed.One},
		Source: a, Target: v, Speed: 50 * fixed.One, Arc: 32 * fixed.One,
		Packet: DamagePacket{Source: a, Target: v, Amount: 10 * fixed.One},
	})
	w.Step()

	// The render snapshot is not part of the hash: re-publishing (which now also
	// fills Missiles) must not perturb the state hash. Hash, publish, hash again.
	reg := NewHashRegistry()
	var s1, s2 statehash.Snapshot
	h1 := w.HashState(reg, &s1).Top
	w.publishSnapshot()
	h2 := w.HashState(reg, &s2).Top
	t.Logf("FSV determinism-inert: hash before publish=%#x after=%#x", h1, h2)
	if h1 != h2 {
		t.Fatalf("publishSnapshot perturbed the state hash: %#x -> %#x (missile publish must be render-only)", h1, h2)
	}

	// And a missile is genuinely present, so the inertness is meaningful (not a
	// vacuous empty-pool pass).
	if len(w.Snaps.Curr().Missiles) != 1 {
		t.Fatalf("expected 1 published missile for a meaningful inertness check, got %d", len(w.Snaps.Curr().Missiles))
	}
}
