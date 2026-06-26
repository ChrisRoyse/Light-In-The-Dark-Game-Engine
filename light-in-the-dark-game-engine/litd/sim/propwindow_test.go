package sim

// #376 prop-window FSV. SoT = a unit's Transforms.Pos across ticks. A
// narrow propulsion window makes a unit turn in place (no translation)
// until its facing is within the window of the desired heading; the
// no-gate default lets it translate immediately (golden-stable).

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

const tMover uint16 = 0

// propWorld binds one mobile unit def: speed 4/tick, turn rate 45°/tick
// (0x2000 BAM), and the given default prop window.
func propWorld(t *testing.T, defWindow fixed.Angle) *World {
	t.Helper()
	w := NewWorld(Caps{Units: 16})
	defs := []data.Unit{{ID: "rider", Life: 100,
		MoveSpeedPerTick: 4 * fixed.One, TurnRatePerTick: 0x2000,
		CollisionSize: 16, PropWindow: defWindow}}
	if !w.BindUnitDefs(defs) {
		t.Fatal("BindUnitDefs failed")
	}
	return w
}

// TestPropWindowGateFSV — a unit facing east, ordered due west, with a
// narrow window turns in place (X unchanged) until aligned, then
// translates. SoT: position X each tick.
func TestPropWindowGateFSV(t *testing.T) {
	w := propWorld(t, propWindowNoGate) // default no-gate; override per unit below
	id, ok := w.SpawnFromTable(tMover, 0, 0, pt2(1000, 1000))
	if !ok {
		t.Fatal("spawn failed")
	}
	tr := w.Transforms.Row(id)
	w.Transforms.Facing[tr] = 0          // facing east (+X)
	w.SetPropWindow(id, fixed.Angle(0x1000)) // 22.5° window
	w.StartMoveTo(id, pt2(0, 1000))      // target due west → want = halfTurn

	startX := w.Transforms.Pos[tr].X
	// 45°/tick: facing 0 -> 0x2000 -> 0x4000 -> 0x6000 -> 0x8000(west). Arc to
	// west <= 22.5° only at tick 4, so X holds for ticks 1-3 then drops.
	for tick := 1; tick <= 3; tick++ {
		w.Step()
		x := w.Transforms.Pos[tr].X
		t.Logf("FSV gated t+%d: facing=%#x X=%d (want unchanged %d)", tick, w.Transforms.Facing[tr], x.Floor(), startX.Floor())
		if x != startX {
			t.Fatalf("tick %d: unit translated while gated (X %d -> %d)", tick, startX.Floor(), x.Floor())
		}
	}
	w.Step() // tick 4: now aligned → translates west
	x4 := w.Transforms.Pos[tr].X
	t.Logf("FSV released t+4: facing=%#x X=%d (want < %d)", w.Transforms.Facing[tr], x4.Floor(), startX.Floor())
	if x4 >= startX {
		t.Fatalf("unit did not translate after aligning: X %d -> %d", startX.Floor(), x4.Floor())
	}
}

// TestPropWindowNoGateFSV — the no-gate default lets a unit translate on
// the first tick even when facing the wrong way (preserves existing
// behavior). SoT: X drops on tick 1.
func TestPropWindowNoGateFSV(t *testing.T) {
	w := propWorld(t, propWindowNoGate)
	id, _ := w.SpawnFromTable(tMover, 0, 0, pt2(1000, 1000))
	tr := w.Transforms.Row(id)
	w.Transforms.Facing[tr] = 0     // facing east, ordered west
	w.StartMoveTo(id, pt2(0, 1000))
	startX := w.Transforms.Pos[tr].X
	w.Step()
	t.Logf("FSV no-gate t+1: X=%d (want < %d) effectiveWindow=%#x", w.Transforms.Pos[tr].X.Floor(), startX.Floor(), w.PropWindow(id))
	if w.Transforms.Pos[tr].X >= startX {
		t.Fatalf("no-gate unit failed to translate immediately: X %d -> %d", startX.Floor(), w.Transforms.Pos[tr].X.Floor())
	}
}

// TestPropWindowAccessorsFSV — default reads the type value; override and
// recycle behave. SoT: PropWindow / DefaultPropWindow.
func TestPropWindowAccessorsFSV(t *testing.T) {
	w := propWorld(t, fixed.Angle(0x2000)) // type default 45°
	id, _ := w.SpawnFromTable(tMover, 0, 0, pt2(100, 100))
	t.Logf("FSV default: PropWindow=%#x DefaultPropWindow=%#x (want 0x2000/0x2000)", w.PropWindow(id), w.DefaultPropWindow(id))
	if w.PropWindow(id) != 0x2000 || w.DefaultPropWindow(id) != 0x2000 {
		t.Fatalf("default prop window wrong: %#x", w.PropWindow(id))
	}
	w.SetPropWindow(id, fixed.Angle(0x0800))
	t.Logf("FSV override: PropWindow=%#x (want 0x0800) default still %#x", w.PropWindow(id), w.DefaultPropWindow(id))
	if w.PropWindow(id) != 0x0800 || w.DefaultPropWindow(id) != 0x2000 {
		t.Fatalf("override wrong: %#x", w.PropWindow(id))
	}
	// recycle: killed slot reverts to the type default.
	w.KillUnit(id)
	w.Step()
	id2, _ := w.SpawnFromTable(tMover, 0, 0, pt2(120, 120))
	t.Logf("FSV recycle: id1=%#x id2=%#x sameSlot=%v id2.PropWindow=%#x (want default 0x2000)",
		uint32(id), uint32(id2), id.Index() == id2.Index(), w.PropWindow(id2))
	if w.PropWindow(id2) != 0x2000 {
		t.Fatalf("recycled unit inherited stale prop window: %#x", w.PropWindow(id2))
	}
}

// TestPropWindowSaveRoundTripFSV — an override survives save(v24)→load and
// the full-World hash matches. SoT: PropWindow on reload + hash.
func TestPropWindowSaveRoundTripFSV(t *testing.T) {
	w := propWorld(t, propWindowNoGate)
	id, _ := w.SpawnFromTable(tMover, 0, 0, pt2(100, 100))
	w.SetPropWindow(id, fixed.Angle(0x0C00))

	reg := NewHashRegistry()
	var before statehash.Snapshot
	w.HashState(reg, &before)

	var buf bytes.Buffer
	const fp = 0x99
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("save: %v", err)
	}
	w2 := propWorld(t, propWindowNoGate)
	if err := w2.LoadState(bytes.NewReader(buf.Bytes()), fp); err != nil {
		t.Fatalf("load: %v", err)
	}
	var after statehash.Snapshot
	w2.HashState(reg, &after)
	t.Logf("FSV reload: PropWindow=%#x (want 0x0C00) ; hash orig=%016x reload=%016x", w2.PropWindow(id), before.Top, after.Top)
	if w2.PropWindow(id) != 0x0C00 {
		t.Fatalf("reloaded prop window = %#x, want 0x0C00", w2.PropWindow(id))
	}
	if before.Top != after.Top {
		t.Fatalf("post-load hash mismatch: %016x vs %016x", before.Top, after.Top)
	}
}
