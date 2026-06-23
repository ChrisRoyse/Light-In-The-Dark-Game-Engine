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
	"bytes"
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
// setupMatch builds and arms an asymmetric match but does NOT step it. High life
// so the battle is fought out over many attack cycles, not a one-shot stomp; the
// size edge makes the larger force the deterministic winner (Lanchester's square
// law: concentrated fire compounds the numbers edge).
func setupMatch(t *testing.T, naSize, nbSize int) (*sim.World, *litd.Game, litd.Player, litd.Player) {
	t.Helper()
	w, g := matchWorld(t)
	p1, p2 := g.Player(1), g.Player(2)
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
	return w, g, p1, p2
}

func playMatch(t *testing.T, capTicks, naSize, nbSize int) (tick int, r1, r2 litd.MatchResult, counts [2]int) {
	t.Helper()
	w, g, p1, p2 := setupMatch(t, naSize, nbSize)
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

// Mid-match save/load (issue edge 1): saving the sim state mid-battle and
// restoring it into a fresh world resumes to the SAME victor in the SAME total
// number of ticks — the match is just inputs+state, so a save is a faithful
// snapshot. SoT = the resumed Player.Result and the terminal tick vs the
// unbroken run.
func TestAcceptanceMatchSaveLoadMidMatchFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("acceptance match runs combat to elimination; -short skips")
	}
	const fp = 0xACCE55ED

	// Unbroken reference.
	refTick, refR1, refR2, _ := playMatch(t, 20000, 5, 3)

	// Run an identical match to a mid-battle tick, then save the sim state.
	w, _, p1, p2 := setupMatch(t, 5, 3)
	const saveAt = 40
	for i := 0; i < saveAt; i++ {
		w.Step()
	}
	if p1.Result() != litd.ResultPlaying || p2.Result() != litd.ResultPlaying {
		t.Fatalf("match already decided by tick %d; choose an earlier save point", saveAt)
	}
	var buf bytes.Buffer
	if err := w.SaveState(&buf, fp); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	saved := buf.Len()

	// Fresh world (same world config). The save/load contract requires the
	// loading world to RE-REGISTER the same handler set (same order) BEFORE
	// LoadState, so the death-driven win condition is installed first; its
	// immediate zero-unit eval stages spurious defeats on the still-empty world,
	// but LoadState then overwrites the entire result store with the saved
	// mid-battle state (both players still Playing).
	w2, _ := matchWorld(t)
	g2 := litd.NewGameForTest(w2)
	q1, q2 := g2.Player(1), g2.Player(2)
	melee.VictoryDefeatConditions(g2, []litd.Player{q1, q2})
	if err := w2.LoadState(&buf, fp); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if q1.Result() != litd.ResultPlaying || q2.Result() != litd.ResultPlaying {
		t.Fatalf("LoadState did not restore mid-battle results: p1=%v p2=%v", q1.Result(), q2.Result())
	}

	tick := saveAt
	for tick < 20000 {
		w2.Step()
		tick++
		if q1.Result() != litd.ResultPlaying || q2.Result() != litd.ResultPlaying {
			break
		}
	}
	t.Logf("FSV #212 save/load: unbroken terminal tick=%d (p1=%v p2=%v); saved %d bytes @tick %d; resumed terminal tick=%d (p1=%v p2=%v)",
		refTick, refR1, refR2, saved, saveAt, tick, q1.Result(), q2.Result())

	if tick != refTick {
		t.Fatalf("save/load changed match length: resumed=%d vs unbroken=%d", tick, refTick)
	}
	if q1.Result() != refR1 || q2.Result() != refR2 {
		t.Fatalf("save/load changed outcome: resumed %v/%v vs unbroken %v/%v", q1.Result(), q2.Result(), refR1, refR2)
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
