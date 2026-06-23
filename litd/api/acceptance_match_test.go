package litd_test

// #212 acceptance-match SIM CORE (headless). The milestone wants a full match vs
// the M5.5 AI reaching a real victory/defeat, with phase evidence. The screenshot
// + firstflame-world-load + economy-AI halves are render/asset-gated; this proves
// the part that is honestly headless: a REAL match driven by the actual combat
// system reaches a NATURAL terminal result via the public last-standing rule
// (melee.VictoryDefeatConditions) — no forced Victory()/Defeat() call, no mock —
// and it is deterministic.
//
// Trigger -> Process -> Outcome (all observed against the sim SoT, not returns):
//   units engage and die in combat  ->  the death-driven condition re-evaluates
//   -> the emptied player is staged Lost and the lone survivor Won (Player.Result,
//   read straight from the result store).

import (
	"testing"

	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api/helpers/melee"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// matchWorld builds a headless combat-ready world: walkable grid, a 1x1 100%
// damage matrix, and an every-tick acquire scan so armies engage promptly.
func matchWorld(t *testing.T) (*sim.World, *litd.Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 64})
	grid := path.NewGrid()
	for y := int32(0); y < 100; y++ {
		for x := int32(0); x < 100; x++ {
			grid.SetFlags(x, y, path.Walkable)
		}
	}
	w.SetGrid(grid)
	if err := w.BindDamageMatrix([][]int32{{1000}}); err != nil {
		t.Fatalf("BindDamageMatrix: %v", err)
	}
	w.SetAcquireInterval(1)
	return w, litd.NewGameForTest(w)
}

// soldier spawns an armed, mobile, damageable unit owned by player `team` at
// world cell (cx,cy): weapon dmg 30, range 120, cooldown 20 — fast enough that
// combat resolves in a bounded number of ticks. life is low so kills are quick.
func soldier(t *testing.T, w *sim.World, team uint8, cx, cy int32, life float64) sim.EntityID {
	t.Helper()
	pos := fixed.Vec2{X: fixed.FromInt(cx*32 + 16), Y: fixed.FromInt(cy*32 + 16)}
	id, ok := w.CreateUnit(pos, 0)
	if !ok ||
		!w.Owners.Add(w.Ents, id, team, team, team) ||
		!w.Healths.Add(w.Ents, id, fixed.F64(life*float64(fixed.One)), 0, 0, 0) ||
		!w.Combats.Add(w.Ents, id) ||
		!w.Orders.Add(w.Ents, id) ||
		!w.Movements.Add(w.Ents, w.Transforms, id, 5*fixed.One, 65535) {
		t.Fatalf("soldier spawn failed (team %d at %d,%d)", team, cx, cy)
	}
	atk := &data.Attack{AttackType: 0, Range: 120 * fixed.One, DamageBase: 30, CooldownTicks: 20, DamagePointTicks: 5, BackswingTicks: 5}
	if !w.SetWeapon(id, 0, atk, 0, data.EffectList{}) {
		t.Fatal("SetWeapon failed")
	}
	// Untyped units have acquisition range 0 and would never find a target; give
	// them a real scan radius so they engage enemies in range (as the sim's own
	// attack-move tests do).
	w.Combats.AcquisitionRange[w.Combats.Row(id)] = fixed.FromInt(600)
	return id
}

func worldPt(cx, cy int32) fixed.Vec2 {
	return fixed.Vec2{X: fixed.FromInt(cx*32 + 16), Y: fixed.FromInt(cy*32 + 16)}
}

// playMatch sets up an asymmetric combat (army A larger than B), installs the
// last-standing rule, drives combat, and returns the final tick reached, the two
// results, and the world top-hash. seed governs nothing here except as a label;
// the setup is fully deterministic so two calls return identical values.
func playMatch(t *testing.T, capTicks, naSize, nbSize int) (tick int, r1, r2 litd.MatchResult, counts [2]int) {
	t.Helper()
	w, g := matchWorld(t)
	p1, p2 := g.Player(1), g.Player(2)

	// Army A (player 1) vs Army B (player 2), facing. High life so the battle is
	// fought out over many attack cycles, not a one-shot stomp; the size edge
	// makes the larger force the deterministic winner (Lanchester's square law:
	// concentrated fire compounds the numbers edge).
	var armyA, armyB []sim.EntityID
	for i := 0; i < naSize; i++ {
		armyA = append(armyA, soldier(t, w, 1, 45, int32(47+i), 150))
	}
	for i := 0; i < nbSize; i++ {
		armyB = append(armyB, soldier(t, w, 2, 48, int32(48+i), 150))
	}
	// Each army attack-moves toward the other's center, engaging en route.
	for _, id := range armyA {
		w.IssueOrder(id, sim.Order{Kind: sim.OrderAttack, Point: worldPt(48, 49), Target: 0}, false)
	}
	for _, id := range armyB {
		w.IssueOrder(id, sim.Order{Kind: sim.OrderAttack, Point: worldPt(45, 49), Target: 0}, false)
	}
	melee.VictoryDefeatConditions(g, []litd.Player{p1, p2})

	if p1.Result() != litd.ResultPlaying || p2.Result() != litd.ResultPlaying {
		t.Fatalf("decided at setup: p1=%v p2=%v", p1.Result(), p2.Result())
	}

	for tick = 1; tick <= capTicks; tick++ {
		w.Step()
		if p1.Result() != litd.ResultPlaying || p2.Result() != litd.ResultPlaying {
			break
		}
	}
	counts = [2]int{melee.PlayerUnitCount(g, p1), melee.PlayerUnitCount(g, p2)}
	return tick, p1.Result(), p2.Result(), counts
}

func TestAcceptanceMatchNaturalVictoryFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("acceptance match runs combat to elimination; -short skips")
	}
	tick, r1, r2, counts := playMatch(t, 20000, 5, 3)
	t.Logf("FSV #212: match reached terminal at tick %d — p1=%v (units=%d) p2=%v (units=%d)",
		tick, r1, counts[0], r2, counts[1])

	// SoT (Player.Result store): the larger army won by ELIMINATION, naturally.
	if r1 != litd.ResultWon {
		t.Fatalf("player 1 (4-unit army) result=%v, want Won", r1)
	}
	if r2 != litd.ResultLost {
		t.Fatalf("player 2 (2-unit army) result=%v, want Lost", r2)
	}
	if counts[1] != 0 {
		t.Fatalf("loser still has %d units — defeat was not by elimination", counts[1])
	}
	if counts[0] == 0 {
		t.Fatal("winner has 0 units — both sides wiped, not a lone-survivor win")
	}
	if tick <= 1 || tick >= 20000 {
		t.Fatalf("terminal tick %d implausible (want a real fought-out match, not instant/timeout)", tick)
	}
}

func TestAcceptanceMatchDeterministicFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("acceptance match runs combat to elimination; -short skips")
	}
	t1, a1, b1, c1 := playMatch(t, 20000, 5, 3)
	t2, a2, b2, c2 := playMatch(t, 20000, 5, 3)
	t.Logf("FSV #212 determinism: runA=(tick %d,%v/%v,%v) runB=(tick %d,%v/%v,%v)", t1, a1, b1, c1, t2, a2, b2, c2)
	if t1 != t2 || a1 != a2 || b1 != b2 || c1 != c2 {
		t.Fatalf("non-deterministic match: runA tick=%d %v/%v %v vs runB tick=%d %v/%v %v", t1, a1, b1, c1, t2, a2, b2, c2)
	}
}

// The win condition is not rigged to player 1: a mirror setup where player 2 has
// the larger army resolves to player 2's victory, and still terminates naturally.
func TestAcceptanceMatchEitherSideCanWinFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("acceptance match runs combat to elimination; -short skips")
	}
	tick, r1, r2, counts := playMatch(t, 20000, 3, 5) // player 2 is now larger
	t.Logf("FSV #212 mirror: tick %d — p1=%v (units=%d) p2=%v (units=%d)", tick, r1, counts[0], r2, counts[1])
	if r2 != litd.ResultWon || r1 != litd.ResultLost {
		t.Fatalf("larger army (player 2) did not win: p1=%v p2=%v", r1, r2)
	}
	if counts[0] != 0 {
		t.Fatalf("loser (player 1) still has %d units", counts[0])
	}
	if tick <= 1 || tick >= 20000 {
		t.Fatalf("mirror terminal tick %d implausible", tick)
	}
}
