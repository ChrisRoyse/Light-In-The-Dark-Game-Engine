package luabind

// #200 path-a edge 1 (hold-timer RESET): the beacon-control victory timer must
// reset to zero the instant a player drops below the beacon threshold — it does
// not pause-and-resume. The existing victory test only ever holds below threshold
// (timer never leaves 0) or holds to victory; neither exercises the
// reset-from-nonzero branch (`else holdSteps[s] = 0` in worlds/firstflame). This
// drives a real contest: P1 captures both required beacons and accrues hold, then
// loses one to a P2 capture BEFORE reaching HOLD_STEPS — the timer must snap back
// to 0 and no victory may latch. SoT = the hold timer + beacon owners the world
// publishes to Storage, and the sim PlayerResult via the Go api.

import (
	"os"
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func cellPos(cx, cy int) api.Vec2 { return api.Vec2{X: float64(cx*32 + 16), Y: float64(cy*32 + 16)} }

func TestFirstFlameVictoryHoldResetFSV(t *testing.T) {
	m, err := mapdata.Load(os.DirFS("../.."), "data/maps/firstflame")
	if err != nil {
		t.Fatal(err)
	}
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 5})
	if err != nil {
		t.Fatal(err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatal(err)
	}
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatal(err)
	}
	RegisterMap(L, m)
	reg := NewChunkRegistry()
	defer reg.Close()

	// P1 (slot 1) holds both required beacons: id1 (128,128) and id2 (88,88).
	g.CreateUnit(g.Player(1), g.UnitType("hfoo"), cellPos(128, 128), api.Deg(0))
	uB2 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), cellPos(88, 88), api.Deg(0))
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "firstflame")); err != nil {
		t.Fatal(err)
	}

	hold := func() int { v, _ := g.Storage().GetInt("hold", "p1"); return v }
	owner := func(i string) int { v, _ := g.Storage().GetInt("beacon"+i, "owner"); return v }
	decided := func() int { v, _ := g.Storage().GetInt("match", "decided"); return v }

	// Each step is one Game_Every(0.25s) tick group = 5 sim ticks. P1 captures both
	// beacons by step 8 and the hold timer starts climbing. At step 9 P1 abandons
	// beacon 2 (unit killed) and a P2 (slot 0) claimant lands on it; P2 needs 8
	// uncontested steps to capture, completing at step 17 — before P1's hold can
	// reach HOLD_STEPS(12) — at which point P1's hold must reset.
	var peakHold, holdAtReset int
	for step := 1; step <= 22; step++ {
		g.Advance(5)
		if step == 9 {
			uB2.Kill()
			g.CreateUnit(g.Player(0), g.UnitType("hfoo"), cellPos(88, 88), api.Deg(0))
		}
		if step == 16 { // peak: P1 still holds both, timer well above zero
			peakHold = hold()
		}
		if step == 17 { // the tick P2's capture lands and P1 drops below threshold
			holdAtReset = hold()
		}
	}

	// (1) The timer genuinely accrued above zero before the loss — otherwise this
	// would be the trivial below-threshold case, not a reset.
	if peakHold <= 0 {
		t.Fatalf("hold timer never accrued (peak=%d) — not a reset-from-nonzero scenario", peakHold)
	}
	// (2) The instant P2 captured beacon 2 (owner → slot 0), P1's timer snapped to 0.
	if owner("2") != 0 {
		t.Fatalf("setup: P2 did not capture beacon 2 (owner=%d)", owner("2"))
	}
	if holdAtReset != 0 {
		t.Fatalf("hold timer did NOT reset on dropping below threshold: peak=%d, at-reset=%d (want 0)", peakHold, holdAtReset)
	}
	// (3) No victory latched — neither prematurely for P1 nor at all.
	if decided() != 0 {
		t.Fatalf("a victory latched despite the hold reset (decided=%d)", decided())
	}
	if r := g.Player(1).Result(); r != api.ResultPlaying {
		t.Fatalf("P1 result = %d after losing a beacon mid-hold, want Playing(%d)", int(r), int(api.ResultPlaying))
	}
	if h := hold(); h != 0 {
		t.Fatalf("hold timer must stay 0 after the reset, got %d", h)
	}
	t.Logf("FSV #200 hold-reset: P1 hold climbed to %d then SNAPPED to 0 when P2 captured beacon 2 (owner→slot0); no victory latched, P1 result=Playing", peakHold)
}
