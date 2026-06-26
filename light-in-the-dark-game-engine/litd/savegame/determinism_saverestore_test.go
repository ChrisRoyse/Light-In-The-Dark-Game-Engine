package savegame

// #271 edge-3 (G5.7 determinism gate): the Lua determinism scenario, SAVED
// mid-run at tick 5000 and restored into a cold process, must finish to a
// Game.StateHash bit-identical to the uninterrupted 10k-tick run. This is the
// "mid-run save/restore at tick 5,000 == unbroken" edge the #271 FSV requires;
// it became testable once #440 made cross-coroutine shared objects round-trip.
//
// The six walker coroutines all close over ONE shared `us` table (a mutable Lua
// table reachable from six separate coroutines) and are still mid-walk at tick
// 5000 (2500 hops, ~2.9 ticks/hop, finishing ~tick 7250). Before #440 this very
// save was REFUSED outright — the old DetectCrossThreadSharing fails closed on a
// mutable table shared across coroutine graphs. #440's shared intern pool lets it
// save and round-trip as one graph, so this is the regression guard that the
// scenario is even saveable, plus the determinism proof that it restores exactly.
//
// SoT = Game.StateHash after 10k ticks: restored-from-5000 vs unbroken, plus the
// run-to-run determinism of the unbroken scenario itself and a positive check
// that the walkers were genuinely PARKED at the save (mid lead-X < final lead-X).

import (
	"bytes"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

const determinismSrc = `deaths = 0
OnEvent(1, function() deaths = deaths + 1 end)
local us = {}
for i = 0, 5 do us[i] = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), {x = i*10, y = 0}, 0) end
for i = 0, 5 do
  Run(function() for k = 1, 2500 do local p = Unit_Position(us[i]); Unit_SetPosition(us[i], {x = p.x + 1, y = p.y}); PolledWait(0.15) end end)
end
Run(function() PolledWait(1.0); Unit_Kill(us[0]) end)`

// leadX is the largest unit x — a coarse "how far has the walk progressed" probe.
func leadX(g *api.Game) float64 {
	max := 0.0
	for _, u := range g.AllUnits(nil) {
		if x := u.Position().X; x > max {
			max = x
		}
	}
	return max
}

func TestLuaDeterminismSaveRestore5kFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("10k-tick save/restore determinism is slow; run without -short")
	}
	const saveTick, total = 5000, 10000

	// Unbroken reference, run twice to confirm the scenario is deterministic.
	gR, LR := newGame(t)
	regR := runChunk(t, LR, "determ", determinismSrc)
	gR.Advance(total)
	refHash := gR.StateHash()
	refN, _ := liveUnits(gR)
	finalLead := leadX(gR)
	LR.Close()
	regR.Close()

	g2, L2 := newGame(t)
	reg2 := runChunk(t, L2, "determ", determinismSrc)
	g2.Advance(total)
	if h2 := g2.StateHash(); h2 != refHash {
		t.Fatalf("unbroken scenario not deterministic run-to-run: %#x != %#x", h2, refHash)
	}
	L2.Close()
	reg2.Close()

	// Save at tick 5000 (walkers parked mid-stride, sharing `us`), restore cold.
	gA, LA := newGame(t)
	regA := runChunk(t, LA, "determ", determinismSrc)
	gA.Advance(saveTick)
	midN, _ := liveUnits(gA)
	midLead := leadX(gA)
	if midLead >= finalLead {
		t.Fatalf("walkers already finished by save tick %d (mid lead %.1f >= final %.1f) — not a mid-run save of live coroutines", saveTick, midLead, finalLead)
	}
	var buf bytes.Buffer
	if err := Write(&buf, gA, LA, regA, fp); err != nil {
		t.Fatalf("Write@%d (shared `us` table across 6 live coroutines must persist, #440): %v", saveTick, err)
	}
	LA.Close()
	regA.Close()

	gB, LB := newGame(t)
	defer LB.Close()
	regB := luabind.NewChunkRegistry()
	defer regB.Close()
	if _, err := regB.Register("determ", determinismSrc); err != nil {
		t.Fatalf("re-register: %v", err)
	}
	if err := Load(bytes.NewReader(buf.Bytes()), gB, LB, regB, fp); err != nil {
		t.Fatalf("Load@%d: %v", saveTick, err)
	}
	gB.Advance(total - saveTick)
	gotHash := gB.StateHash()
	gotN, _ := liveUnits(gB)
	gotLead := leadX(gB)
	if gotHash != refHash {
		t.Fatalf("save/restore@%d DIVERGED: restored final %#x != unbroken %#x — shared-table walk did not round-trip", saveTick, gotHash, refHash)
	}
	if gotN != refN || gotLead != finalLead {
		t.Fatalf("restored end-state mismatch: units %d/lead %.1f, want %d/%.1f", gotN, gotLead, refN, finalLead)
	}
	t.Logf("FSV #271 edge-3: save@%d parked (%d units, lead %.1f<%.1f) → restored 10k StateHash %#x == unbroken; units %d, lead %.1f",
		saveTick, midN, midLead, finalLead, gotHash, gotN, gotLead)
}
