package sim

import (
	"bytes"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// TestPlayerAllianceAsymmetryFSV — alliance is one-directional. SoT: the
// alliance bitset read back via IsAlly/IsEnemy after each set.
func TestPlayerAllianceAsymmetryFSV(t *testing.T) {
	w := NewWorld(Caps{})

	// BEFORE: FFA default — distinct teams, nobody allied.
	t.Logf("FSV default: IsAlly(0,1)=%v IsEnemy(0,1)=%v team0=%d team1=%d",
		w.IsAlly(0, 1), w.IsEnemy(0, 1), w.PlayerTeam(0), w.PlayerTeam(1))
	if w.IsAlly(0, 1) || !w.IsEnemy(0, 1) {
		t.Fatalf("default should be mutual enemies")
	}

	// 0 allies 1 (one-directional). 1 still at war with 0.
	w.SetAlliance(0, 1, AlliancePassive)
	t.Logf("FSV after SetAlliance(0->1,passive): IsAlly(0,1)=%v IsAlly(1,0)=%v IsEnemy(1,0)=%v alliance[0][1]=%#x alliance[1][0]=%#x",
		w.IsAlly(0, 1), w.IsAlly(1, 0), w.IsEnemy(1, 0), w.Alliance(0, 1), w.Alliance(1, 0))
	if !w.IsAlly(0, 1) {
		t.Fatalf("0 should be ally of 1 after SetAlliance")
	}
	if w.IsAlly(1, 0) {
		t.Fatalf("alliance leaked back: 1 became ally of 0 (must be asymmetric)")
	}
	if !w.IsEnemy(1, 0) {
		t.Fatalf("1 should still be enemy of 0")
	}

	// Single-flag toggle leaves the rest intact.
	w.SetAllianceFlag(0, 1, AllianceSharedVision, true)
	if w.Alliance(0, 1) != (AlliancePassive|AllianceSharedVision) {
		t.Fatalf("flag toggle wrong: %#x", w.Alliance(0, 1))
	}
	w.SetAllianceFlag(0, 1, AlliancePassive, false)
	t.Logf("FSV after clear passive: IsAlly(0,1)=%v alliance[0][1]=%#x", w.IsAlly(0, 1), w.Alliance(0, 1))
	if w.IsAlly(0, 1) || w.Alliance(0, 1) != AllianceSharedVision {
		t.Fatalf("clearing passive should leave only shared-vision: %#x", w.Alliance(0, 1))
	}

	// Self: never ally/enemy, and SetAlliance is a no-op on self.
	w.SetAlliance(2, 2, AlliancePassive)
	t.Logf("FSV self: IsAlly(2,2)=%v IsEnemy(2,2)=%v alliance[2][2]=%#x", w.IsAlly(2, 2), w.IsEnemy(2, 2), w.Alliance(2, 2))
	if w.IsAlly(2, 2) || w.IsEnemy(2, 2) || w.Alliance(2, 2) != 0 {
		t.Fatalf("self alliance must be inert")
	}

	// Out-of-range: inert.
	w.SetAlliance(0, 99, AlliancePassive)
	w.SetAlliance(99, 0, AlliancePassive)
	if w.IsAlly(0, 99) || w.IsEnemy(99, 0) {
		t.Fatalf("out-of-range alliance not inert")
	}
}

// TestPlayerResourceSetterFSV — the economy ledger is the SoT. X+X=Y:
// set 2 gold, add 2, expect 4; negative add clamps to 0.
func TestPlayerResourceSetterFSV(t *testing.T) {
	w := NewWorld(Caps{})
	if !w.BindEconomy(2) { // gold=0, lumber=1
		t.Fatalf("BindEconomy failed")
	}
	t.Logf("FSV before: gold=%d lumber=%d", w.Resources(0, 0), w.Resources(0, 1))

	w.SetResource(0, 0, 2)
	w.AddResource(0, 0, 2)
	w.SetResource(0, 1, 750)
	t.Logf("FSV after set 2 + add 2 gold, set 750 lumber: gold=%d lumber=%d", w.Resources(0, 0), w.Resources(0, 1))
	if w.Resources(0, 0) != 4 {
		t.Fatalf("2+2 gold = %d, want 4", w.Resources(0, 0))
	}
	if w.Resources(0, 1) != 750 {
		t.Fatalf("lumber = %d, want 750", w.Resources(0, 1))
	}

	// Negative clamp: subtract more than held.
	w.AddResource(0, 0, -1000)
	t.Logf("FSV after subtract 1000 from 4: gold=%d (clamped)", w.Resources(0, 0))
	if w.Resources(0, 0) != 0 {
		t.Fatalf("clamp failed: gold = %d, want 0", w.Resources(0, 0))
	}

	// SetResource negative also clamps.
	w.SetResource(0, 1, -5)
	if w.Resources(0, 1) != 0 {
		t.Fatalf("SetResource negative clamp failed: %d", w.Resources(0, 1))
	}

	// Out-of-range player / resource index: no-op (no panic).
	w.SetResource(99, 0, 100)
	w.SetResource(0, 9, 100)
	w.AddResource(99, 0, 100)
	t.Logf("FSV out-of-range writes inert; gold[0]=%d", w.Resources(0, 0))

	// Food cap setter clamps; used stays unit-derived.
	w.SetFoodCap(0, 40)
	w.SetFoodCap(1, -3)
	t.Logf("FSV foodCap[0]=%d foodCap[1]=%d (clamped)", w.FoodCap(0), w.FoodCap(1))
	if w.FoodCap(0) != 40 || w.FoodCap(1) != 0 {
		t.Fatalf("food cap set/clamp wrong: %d %d", w.FoodCap(0), w.FoodCap(1))
	}
}

// TestPlayerRosterSaveLoadFSV — roster + alliance survive a save/load
// round-trip and the state hash is identical before and after. SoT: the
// reloaded roster fields + the two hashes.
func TestPlayerRosterSaveLoadFSV(t *testing.T) {
	w := NewWorld(Caps{})
	w.SetPlayerName(0, "synthetic_player_0")
	w.SetController(0, ControllerUser)
	w.SetController(1, ControllerComputer)
	w.SetPlayerRace(0, 1) // human
	w.SetPlayerColor(0, 4) // yellow
	w.SetPlayerTeam(0, 3)
	w.SetPlayerStart(0, ff(512), ff(1024))
	w.SetAlliedVictory(0, true)
	w.SetAlliance(0, 1, AlliancePassive|AllianceSharedVision)

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

	sx, sy := w2.PlayerStart(0)
	t.Logf("FSV reloaded: name=%q controller0=%d controller1=%d race=%d color=%d team=%d start=(%d,%d) alliedVictory=%v alliance[0][1]=%#x",
		w2.PlayerName(0), w2.Controller(0), w2.Controller(1), w2.PlayerRace(0), w2.PlayerColor(0), w2.PlayerTeam(0),
		int64(sx)>>32, int64(sy)>>32, w2.AlliedVictory(0), w2.Alliance(0, 1))
	t.Logf("FSV hash before=%016x after=%016x", hA, hB)

	if w2.PlayerName(0) != "synthetic_player_0" {
		t.Fatalf("name lost: %q", w2.PlayerName(0))
	}
	if w2.Controller(0) != ControllerUser || w2.Controller(1) != ControllerComputer {
		t.Fatalf("controller lost: %d %d", w2.Controller(0), w2.Controller(1))
	}
	if w2.PlayerRace(0) != 1 || w2.PlayerColor(0) != 4 || w2.PlayerTeam(0) != 3 {
		t.Fatalf("race/color/team lost: %d %d %d", w2.PlayerRace(0), w2.PlayerColor(0), w2.PlayerTeam(0))
	}
	if sx != ff(512) || sy != ff(1024) {
		t.Fatalf("start loc lost: (%d,%d)", int64(sx)>>32, int64(sy)>>32)
	}
	if !w2.AlliedVictory(0) {
		t.Fatalf("allied-victory flag lost")
	}
	if w2.Alliance(0, 1) != (AlliancePassive | AllianceSharedVision) {
		t.Fatalf("alliance lost: %#x", w2.Alliance(0, 1))
	}
	if hA != hB {
		t.Fatalf("state hash diverged across save/load: %016x != %016x", hA, hB)
	}
}
