package sim

// #528 missile flight-progress FSV. SoT = MissileSnapEntry.LifeFrac read off the
// published snapshot (derived from sim missile Span/RangeLeft/Pos), the live store
// Span after save/load, and the state hash. A linear missile of known Range/Speed
// must report exact, monotonically rising progress; Span must round-trip a save
// yet never enter the state hash (render-only); a degenerate span renders as
// arrived. X+X=Y: Range 1000 at Speed 100, after 5 ticks the missile is halfway,
// so LifeFrac == 32767 (½ of 65535).

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

func TestMissileLifeFracProgressFSV(t *testing.T) {
	w, a, _ := msWorld(t) // victim sits at +x; fire -x into empty space so it flies its full range
	const speed = 100 * fixed.One
	const rng = 1000 * fixed.One
	mid, ok := w.SpawnMissile(MissileSpec{
		Pos:        xy(1000, 1000),
		Source:     a,
		Speed:      speed,
		Flags:      MissileLinear,
		GuidanceID: MissileGuidanceLinear,
		ImpactID:   MissileImpactPierce,
		Dir:        xy(-1, 0),
		Range:      rng,
		Pierce:     1,
		Packet:     DamagePacket{Source: a, Amount: 1 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn linear missile failed")
	}
	// SoT: Span captured at spawn = the whole-unit range.
	r := w.Missiles.Row(mid)
	if got := w.Missiles.Span[r]; got != 1000 {
		t.Fatalf("spawn Span = %d, want 1000", got)
	}

	// Walk the flight, reading LifeFrac off the snapshot each tick. Speed 100 over
	// Range 1000 => 10 ticks; collect while the missile is still live.
	var fracs []uint16
	for i := 0; i < 9; i++ {
		w.Step()
		snap := w.Snaps.Curr()
		if len(snap.Missiles) == 0 {
			break // delivered/expired
		}
		fracs = append(fracs, snap.Missiles[0].LifeFrac)
	}
	t.Logf("FSV LifeFrac sequence: %v", fracs)
	if len(fracs) < 5 {
		t.Fatalf("only %d in-flight snapshots, want >=5", len(fracs))
	}
	// Monotonic non-decreasing, strictly rising over the flight.
	for i := 1; i < len(fracs); i++ {
		if fracs[i] < fracs[i-1] {
			t.Fatalf("LifeFrac went backwards at %d: %d < %d", i, fracs[i], fracs[i-1])
		}
	}
	if fracs[len(fracs)-1] <= fracs[0] {
		t.Fatalf("LifeFrac did not rise: first=%d last=%d", fracs[0], fracs[len(fracs)-1])
	}
	// X+X=Y: after 5 ticks the missile has gone 500 of 1000 units — exactly half,
	// LifeFrac == 32767.
	if fracs[4] != 32767 {
		t.Fatalf("halfway LifeFrac = %d, want 32767 (½ flight at tick 5)", fracs[4])
	}
}

func TestMissileSpanUnhashedButSavedFSV(t *testing.T) {
	w, a, v := msWorld(t)
	mid, ok := w.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: a, Target: v,
		Speed:      50 * fixed.One,
		GuidanceID: MissileGuidanceHoming,
		Packet:     DamagePacket{Source: a, Target: v, Amount: 10 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn failed")
	}
	r := w.Missiles.Row(mid)
	spawnSpan := w.Missiles.Span[r]
	t.Logf("FSV homing Span at spawn = %d units", spawnSpan)
	if spawnSpan <= 0 {
		t.Fatalf("homing Span = %d, want > 0 (distance launch->target)", spawnSpan)
	}

	// Contradiction-engine SoT: mutating a HASHED field (Accel) changes the hash;
	// mutating Span does NOT — proving Span is render-only and never folded.
	reg := NewHashRegistry()
	var base, spanMut, accelMut statehash.Snapshot
	w.HashState(reg, &base)
	w.Missiles.Span[r] = spawnSpan + 9999
	w.HashState(reg, &spanMut)
	w.Missiles.Span[r] = spawnSpan // restore
	w.Missiles.Accel[r] += 7 * fixed.One
	w.HashState(reg, &accelMut)
	w.Missiles.Accel[r] -= 7 * fixed.One
	t.Logf("FSV hash: base=%016x spanMutated=%016x accelMutated=%016x", base.Top, spanMut.Top, accelMut.Top)
	if spanMut.Top != base.Top {
		t.Fatal("Span entered the state hash — must be render-only (#528)")
	}
	if accelMut.Top == base.Top {
		t.Fatal("control failed: Accel mutation should change the hash")
	}

	// But Span IS persisted: save -> load preserves it (the store round-trips
	// fully, so a reloaded mid-flight missile keeps correct arc progress).
	var buf bytes.Buffer
	if err := w.SaveState(&buf, 9); err != nil {
		t.Fatal(err)
	}
	loaded, _, _ := msWorld(t)
	if err := loaded.LoadState(bytes.NewReader(buf.Bytes()), 9); err != nil {
		t.Fatal(err)
	}
	lr := loaded.Missiles.Row(mid)
	if lr < 0 {
		t.Fatal("missile missing after load")
	}
	t.Logf("FSV save/load Span: original=%d reloaded=%d", spawnSpan, loaded.Missiles.Span[lr])
	if loaded.Missiles.Span[lr] != spawnSpan {
		t.Fatalf("save/load lost Span: %d != %d", loaded.Missiles.Span[lr], spawnSpan)
	}
}

func TestMissileLifeFracDegenerateSpanFSV(t *testing.T) {
	w, a, v := msWorld(t)
	mid, ok := w.SpawnMissile(MissileSpec{
		Pos: xy(1000, 1000), Source: a, Target: v,
		Speed:      50 * fixed.One,
		GuidanceID: MissileGuidanceHoming,
		Packet:     DamagePacket{Source: a, Target: v, Amount: 10 * fixed.One},
	})
	if !ok {
		t.Fatal("spawn failed")
	}
	// Force the degenerate span (launch-on-goal): the published progress must
	// clamp to "arrived" (65535), never divide by zero.
	r := w.Missiles.Row(mid)
	w.Missiles.Span[r] = 0
	w.publishSnapshot()
	snap := w.Snaps.Curr()
	if len(snap.Missiles) != 1 {
		t.Fatalf("missiles in snapshot = %d, want 1", len(snap.Missiles))
	}
	t.Logf("FSV degenerate span LifeFrac = %d", snap.Missiles[0].LifeFrac)
	if snap.Missiles[0].LifeFrac != 65535 {
		t.Fatalf("degenerate span LifeFrac = %d, want 65535 (arrived)", snap.Missiles[0].LifeFrac)
	}
}
