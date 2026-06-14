package sim

// #367 fly-height FSV. SoT = World.FlyHeight after BindUnitDefs +
// SetFlyHeight + Step ticks, plus the post-save hash. Known-input/
// known-output: a 50→100 climb at rate 10/tick reaches 60,70,80,90,100
// over five ticks and then parks (no overshoot).

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const tFlyer uint16 = 0

// flyWorld binds one unit def with a default fly height of 50.
func flyWorld(t *testing.T) *World {
	t.Helper()
	w := NewWorld(Caps{Units: 16})
	defs := []data.Unit{{ID: "gryphon", Life: 100, FlyHeight: fixed.FromInt(50),
		MoveSpeedPerTick: 2 * fixed.One, CollisionSize: 16}}
	if !w.BindUnitDefs(defs) {
		t.Fatal("BindUnitDefs failed")
	}
	return w
}

func fu(v fixed.F64) int64 { return int64(v) / int64(fixed.One) }

// TestFlyHeightDefaultFSV — an un-set unit reads its type default; an
// untyped/dead handle reads 0.
func TestFlyHeightDefaultFSV(t *testing.T) {
	w := flyWorld(t)
	id, ok := w.SpawnFromTable(tFlyer, 0, 0, pt2(100, 100))
	if !ok {
		t.Fatal("spawn failed")
	}
	t.Logf("FSV default: FlyHeight=%d DefaultFlyHeight=%d (want 50/50)", fu(w.FlyHeight(id)), fu(w.DefaultFlyHeight(id)))
	if w.FlyHeight(id) != fixed.FromInt(50) || w.DefaultFlyHeight(id) != fixed.FromInt(50) {
		t.Fatalf("default fly height wrong: %d", fu(w.FlyHeight(id)))
	}
	var dead EntityID
	if w.FlyHeight(dead) != 0 {
		t.Fatalf("dead handle fly height = %d, want 0", fu(w.FlyHeight(dead)))
	}
}

// TestFlyHeightClimbFSV — SetFlyHeight animates toward the target at the
// climb rate, never overshoots, then parks. SoT: FlyHeight after each Step.
func TestFlyHeightClimbFSV(t *testing.T) {
	w := flyWorld(t)
	id, _ := w.SpawnFromTable(tFlyer, 0, 0, pt2(100, 100))
	w.SetFlyHeight(id, fixed.FromInt(100), fixed.FromInt(10)) // 50 -> 100 at 10/tick
	want := []int64{60, 70, 80, 90, 100, 100} // 6th tick: parked, no overshoot
	for i, exp := range want {
		w.Step()
		got := fu(w.FlyHeight(id))
		t.Logf("FSV climb t+%d: FlyHeight=%d (want %d)", i+1, got, exp)
		if got != exp {
			t.Fatalf("tick %d: fly height %d, want %d", i+1, got, exp)
		}
	}

	// descend 100 -> 0 at 25/tick.
	w.SetFlyHeight(id, 0, fixed.FromInt(25))
	down := []int64{75, 50, 25, 0, 0}
	for i, exp := range down {
		w.Step()
		got := fu(w.FlyHeight(id))
		t.Logf("FSV descend t+%d: FlyHeight=%d (want %d)", i+1, got, exp)
		if got != exp {
			t.Fatalf("descend tick %d: %d, want %d", i+1, got, exp)
		}
	}
}

// TestFlyHeightSnapFSV — edge: rate 0 snaps instantly (no Step needed).
func TestFlyHeightSnapFSV(t *testing.T) {
	w := flyWorld(t)
	id, _ := w.SpawnFromTable(tFlyer, 0, 0, pt2(100, 100))
	before := w.FlyHeight(id)
	w.SetFlyHeight(id, fixed.FromInt(30), 0)
	t.Logf("FSV snap: %d -> %d (rate 0, want 30 immediately)", fu(before), fu(w.FlyHeight(id)))
	if w.FlyHeight(id) != fixed.FromInt(30) {
		t.Fatalf("snap fly height = %d, want 30", fu(w.FlyHeight(id)))
	}
}

// TestFlyHeightRecycleSafeFSV — edge: a slot recycled after death does not
// inherit the prior occupant's fly height. SoT: the new unit reads the
// default, not the stale set value.
func TestFlyHeightRecycleSafeFSV(t *testing.T) {
	w := flyWorld(t)
	id1, _ := w.SpawnFromTable(tFlyer, 0, 0, pt2(100, 100))
	w.SetFlyHeight(id1, fixed.FromInt(200), 0)
	t.Logf("FSV pre-recycle: id1=%#x FlyHeight=%d", uint32(id1), fu(w.FlyHeight(id1)))
	w.KillUnit(id1)
	w.Step() // phase-7 removal frees the slot (and drops the fly row)
	id2, _ := w.SpawnFromTable(tFlyer, 0, 0, pt2(120, 120))
	t.Logf("FSV recycle: id1=%#x id2=%#x sameSlot=%v id2.FlyHeight=%d (want 50 default)",
		uint32(id1), uint32(id2), id1.Index() == id2.Index(), fu(w.FlyHeight(id2)))
	if w.FlyHeight(id2) != fixed.FromInt(50) {
		t.Fatalf("recycled unit inherited stale fly height: %d", fu(w.FlyHeight(id2)))
	}
}

// TestFlyHeightSaveRoundTripFSV — a mid-climb fly row survives save(v23)→
// load and the full-World hash matches. SoT: FlyHeight on reload + hash.
func TestFlyHeightSaveRoundTripFSV(t *testing.T) {
	w := flyWorld(t)
	id, _ := w.SpawnFromTable(tFlyer, 0, 0, pt2(100, 100))
	w.SetFlyHeight(id, fixed.FromInt(100), fixed.FromInt(10))
	w.Step() // 60, mid-climb (row has Height=60,Target=100,Rate=10)

	reg := NewHashRegistry()
	var before statehash.Snapshot
	w.HashState(reg, &before)

	var buf bytes.Buffer
	const fp = 0x77
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := flyWorld(t)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}
	// hash immediately after load — must equal the pre-save hash.
	var after statehash.Snapshot
	w2.HashState(reg, &after)
	t.Logf("FSV reload: FlyHeight=%d (want 60) ; hash orig=%016x reload=%016x", fu(w2.FlyHeight(id)), before.Top, after.Top)
	if w2.FlyHeight(id) != fixed.FromInt(60) {
		t.Fatalf("reloaded fly height = %d, want 60", fu(w2.FlyHeight(id)))
	}
	if before.Top != after.Top {
		t.Fatalf("post-load hash mismatch: %016x vs %016x", before.Top, after.Top)
	}
	// climb resumes deterministically after load.
	w2.Step()
	if w2.FlyHeight(id) != fixed.FromInt(70) {
		t.Fatalf("post-load climb = %d, want 70", fu(w2.FlyHeight(id)))
	}
}
