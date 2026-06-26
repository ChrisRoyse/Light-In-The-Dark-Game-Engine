package ai_test

import (
	"fmt"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// #277 production FSV. SoT = the sim Produce store (Queue/QCount/Done) and the
// resource/food ledgers (Resources/FoodUsed/FoodCap), read straight from
// sim.World. The litd/ai production layer (intents + ProductionView counts) is
// the unit under test; every count it reports is cross-checked against the sim.

const (
	pFootman uint16 = 0 // 50 gold, 2 food, 40 train-ticks
	pBarrack uint16 = 1 // trains footmen, provides 20 food, no food cost
)

func prodDefs277() []data.Unit {
	return []data.Unit{
		{ID: "footman", Life: 100, CollisionSize: 16, Costs: []int64{50, 0}, TrainTicks: 40, FoodCost: 2},
		{ID: "barracks", Life: 1000, CollisionSize: 64, FoodProvided: 20, Trains: []uint16{pFootman}},
	}
}

func ptAt(x, y int32) fixed.Vec2 {
	return fixed.Vec2{X: fixed.FromInt(x), Y: fixed.FromInt(y)}
}

// prodCtrl adapts *sim.World to ai.ProductionControl using only public sim API.
type prodCtrl struct{ w *sim.World }

func (a prodCtrl) TrainForPlayer(player, typeID int) (int, int) {
	b, reason := a.w.TrainForPlayer(uint8(player), uint16(typeID))
	if reason != sim.TrainOK {
		return -1, int(reason) // refused: deterministic no-op
	}
	return int(b.Index()), int(reason)
}
func (a prodCtrl) TrainInProgress(player, typeID int) int {
	return a.w.PlayerTrainInProgress(uint8(player), uint16(typeID))
}
func (a prodCtrl) TrainQueued(player, typeID int) int {
	return a.w.PlayerTrainQueued(uint8(player), uint16(typeID))
}

// doneFootmen counts completed footman ENTITIES owned by player (the
// GetUnitCountDone source of truth — straight from the sim stores).
func doneFootmen(w *sim.World, player uint8) int {
	var ids []sim.EntityID
	ids = w.AppendAllUnits(ids)
	n := 0
	for _, id := range ids {
		or := w.Owners.Row(id)
		ur := w.UnitTypes.Row(id)
		if or == -1 || ur == -1 {
			continue
		}
		if w.Owners.Player[or] == player && w.UnitTypes.TypeID[ur] == pFootman {
			n++
		}
	}
	return n
}

func prodWorld277(t *testing.T) *sim.World {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 64})
	if !w.BindEconomy(2) || !w.BindUnitDefs(prodDefs277()) {
		t.Fatal("bind failed")
	}
	return w
}

// TestProductionFiveFootmenAcrossTwoBarracksFSV — the headline scenario. A
// scripted AI (player 1) issues 5 train-footman intents; the sim distributes
// them across 2 barracks (least-loaded, ties to lowest id); we drive the sim
// and read the (queued, in-progress, done) table every tick. Two invariants:
// (a) conservation — done+inProgress+queued == 5 at EVERY tick (no unit lost or
// duplicated); (b) determinism — the full per-tick trace is byte-identical
// across two runs.
func TestProductionFiveFootmenAcrossTwoBarracksFSV(t *testing.T) {
	const player = 1

	run := func(logIt bool) (trace string, finalDone int) {
		w := prodWorld277(t)
		b1, ok1 := w.SpawnFromTable(pBarrack, player, player, ptAt(200, 200))
		b2, ok2 := w.SpawnFromTable(pBarrack, player, player, ptAt(600, 600))
		if !ok1 || !ok2 {
			t.Fatal("barracks spawn failed")
		}
		w.SetResource(player, 0, 500) // gold for 10 footmen — not the limiter
		ctrl := prodCtrl{w}
		pv := ai.NewProductionView(ctrl, player)

		// Issue 5 intents into the command stream (the AICommander boundary).
		st := ai.NewCommandStream(16)
		cmdr := st.Commander(player)
		ai.TrainUnits(cmdr, int(pFootman), 5)
		if st.Len() != 5 {
			t.Fatalf("stream recorded %d intents, want 5", st.Len())
		}

		// Apply each intent: the sim selects a producer and admits it.
		var assign []int
		st.Drain(func(_ int, c ai.AICommand) {
			b, reason := ai.ApplyTrain(ctrl, c)
			if reason != ai.TrainOK {
				t.Fatalf("intent refused unexpectedly: reason=%d", reason)
			}
			assign = append(assign, b)
		})
		if logIt {
			t.Logf("FSV producer assignment (intent order → building index): %v", assign)
			t.Logf("FSV barracks indices: b1=%d b2=%d", b1.Index(), b2.Index())
		}
		// b1 lower index: least-loaded+lowest-id gives b1,b2,b1,b2,b1 → b1×3, b2×2.
		if assign[0] != int(b1.Index()) || assign[1] != int(b2.Index()) {
			t.Fatalf("assignment[0,1]=%v want [b1=%d b2=%d]", assign[:2], b1.Index(), b2.Index())
		}

		var sb strings.Builder
		for tk := 0; tk <= 130; tk++ {
			inProg := pv.InProgress(int(pFootman))
			queued := pv.Queued(int(pFootman))
			done := doneFootmen(w, player)
			// Conservation invariant — the heart of the FSV (X+X=Y: 5 in, 5 always accounted).
			if done+inProg+queued != 5 {
				t.Fatalf("tick %d conservation broken: done=%d inProgress=%d queued=%d sum=%d want 5",
					tk, done, inProg, queued, done+inProg+queued)
			}
			// Cross-check the AI-facing pending count against the sim ground truth.
			if pv.Pending(int(pFootman)) != inProg+queued {
				t.Fatalf("tick %d Pending=%d != inProgress+queued=%d", tk, pv.Pending(int(pFootman)), inProg+queued)
			}
			fmt.Fprintf(&sb, "t%d:q%d/p%d/d%d ", tk, queued, inProg, done)
			w.Step()
		}
		return sb.String(), doneFootmen(w, player)
	}

	trace1, done1 := run(true)
	trace2, done2 := run(false)

	// Print the transition milestones from the recorded trace for the evidence log.
	for _, tk := range []string{"t0:", "t40:", "t41:", "t80:", "t81:", "t120:", "t121:", "t130:"} {
		i := strings.Index(trace1, " "+strings.TrimSuffix(tk, ":")+":")
		if i == -1 && strings.HasPrefix(trace1, strings.TrimSuffix(tk, ":")+":") {
			i = 0
		} else {
			i++
		}
		seg := trace1[i:]
		if j := strings.IndexByte(seg, ' '); j != -1 {
			t.Logf("FSV %s", seg[:j])
		}
	}
	t.Logf("FSV final done run1=%d run2=%d (want 5)", done1, done2)

	if done1 != 5 || done2 != 5 {
		t.Fatalf("final done=%d/%d want 5/5", done1, done2)
	}
	if trace1 != trace2 {
		t.Fatalf("per-tick trace not deterministic across 2 runs:\n run1=%s\n run2=%s", trace1, trace2)
	}
}

// TestProductionZeroAllocFSV — the production query + apply path allocates
// nothing at steady state (R-GC-1).
func TestProductionZeroAllocFSV(t *testing.T) {
	const player = 1
	w := prodWorld277(t)
	w.SpawnFromTable(pBarrack, player, player, ptAt(200, 200))
	w.SetResource(player, 0, 1_000_000)
	ctrl := prodCtrl{w}
	pv := ai.NewProductionView(ctrl, player)
	allocs := testing.AllocsPerRun(500, func() {
		_, _ = ctrl.TrainForPlayer(player, int(pFootman))
		_ = pv.InProgress(int(pFootman))
		_ = pv.Queued(int(pFootman))
		_ = pv.Pending(int(pFootman))
	})
	t.Logf("FSV train+count allocs/op=%v", allocs)
	if allocs != 0 {
		t.Fatalf("production path allocates %v/op at steady state, want 0", allocs)
	}
}

// TestProductionInsufficientGoldNoOpFSV — edge 1. A train intent the player
// cannot afford is recorded in the command stream like any other but is a
// deterministic no-op at apply time (sim unchanged), observable only via counts.
func TestProductionInsufficientGoldNoOpFSV(t *testing.T) {
	const player = 1
	w := prodWorld277(t)
	w.SpawnFromTable(pBarrack, player, player, ptAt(200, 200))
	w.SetResource(player, 0, 50) // exactly one footman's worth of gold
	ctrl := prodCtrl{w}
	pv := ai.NewProductionView(ctrl, player)

	st := ai.NewCommandStream(8)
	ai.TrainUnits(st.Commander(player), int(pFootman), 2)

	t.Logf("FSV before apply: gold=%d inProgress=%d queued=%d streamLen=%d",
		w.Resources(player, 0), pv.InProgress(int(pFootman)), pv.Queued(int(pFootman)), st.Len())

	var reasons []int
	st.Drain(func(_ int, c ai.AICommand) {
		_, r := ai.ApplyTrain(ctrl, c)
		reasons = append(reasons, r)
		t.Logf("FSV   apply → reason=%d gold-after=%d inProgress=%d queued=%d",
			r, w.Resources(player, 0), pv.InProgress(int(pFootman)), pv.Queued(int(pFootman)))
	})

	if len(reasons) != 2 || reasons[0] != ai.TrainOK || reasons[1] != ai.TrainNoResources {
		t.Fatalf("reasons=%v want [TrainOK(0) TrainNoResources(7)]", reasons)
	}
	if w.Resources(player, 0) != 0 {
		t.Fatalf("gold=%d want 0 (exactly one deduction; the refused intent deducted nothing)", w.Resources(player, 0))
	}
	if pv.InProgress(int(pFootman)) != 1 || pv.Queued(int(pFootman)) != 0 {
		t.Fatalf("counts inProgress=%d queued=%d want 1/0 (only the affordable intent took)", pv.InProgress(int(pFootman)), pv.Queued(int(pFootman)))
	}
	t.Logf("FSV unaffordable intent recorded in stream (len stayed 2) but a no-op in the sim")
}

// TestProductionProducerDestroyedMidTrainFSV — edge 2. A producer destroyed
// while training loses its queue; food reservations are released, but gold
// stays spent (destruction is NOT a cancel — no refund). Documented disposition,
// state printed before and after.
func TestProductionProducerDestroyedMidTrainFSV(t *testing.T) {
	const player = 1
	w := prodWorld277(t)
	bar, _ := w.SpawnFromTable(pBarrack, player, player, ptAt(200, 200))
	w.SetResource(player, 0, 500)
	ctrl := prodCtrl{w}
	pv := ai.NewProductionView(ctrl, player)

	st := ai.NewCommandStream(8)
	ai.TrainUnits(st.Commander(player), int(pFootman), 2)
	st.Drain(func(_ int, c ai.AICommand) { ai.ApplyTrain(ctrl, c) })

	for i := 0; i < 20; i++ { // train partway: head at elapsed≈20/40
		w.Step()
	}
	goldBefore, foodBefore := w.Resources(player, 0), w.FoodUsed(player)
	t.Logf("FSV before destroy: gold=%d foodUsed=%d inProgress=%d queued=%d",
		goldBefore, foodBefore, pv.InProgress(int(pFootman)), pv.Queued(int(pFootman)))
	if pv.InProgress(int(pFootman)) != 1 || pv.Queued(int(pFootman)) != 1 {
		t.Fatalf("pre-destroy counts %d/%d want 1/1", pv.InProgress(int(pFootman)), pv.Queued(int(pFootman)))
	}
	if goldBefore != 400 || foodBefore != 4 {
		t.Fatalf("pre-destroy ledger gold=%d food=%d want 400/4 (2 footmen reserved)", goldBefore, foodBefore)
	}

	w.DestroyUnit(bar)

	goldAfter, foodAfter := w.Resources(player, 0), w.FoodUsed(player)
	t.Logf("FSV after destroy: gold=%d foodUsed=%d inProgress=%d queued=%d",
		goldAfter, foodAfter, pv.InProgress(int(pFootman)), pv.Queued(int(pFootman)))
	if pv.InProgress(int(pFootman)) != 0 || pv.Queued(int(pFootman)) != 0 {
		t.Fatalf("post-destroy counts %d/%d want 0/0 (queue gone)", pv.InProgress(int(pFootman)), pv.Queued(int(pFootman)))
	}
	if foodAfter != 0 {
		t.Fatalf("food not released on destroy: foodUsed=%d want 0", foodAfter)
	}
	if goldAfter != 400 {
		t.Fatalf("gold=%d want 400 unchanged (destruction is not a cancel — no refund)", goldAfter)
	}
	t.Logf("FSV documented: producer death drops the queue, releases food reservations, keeps gold spent")
}

// TestProductionTwoIdleLowestIdFSV — edge 3. With two idle producers and one
// intent, the sim deterministically picks the lowest entity id.
func TestProductionTwoIdleLowestIdFSV(t *testing.T) {
	const player = 1
	w := prodWorld277(t)
	b1, _ := w.SpawnFromTable(pBarrack, player, player, ptAt(200, 200))
	b2, _ := w.SpawnFromTable(pBarrack, player, player, ptAt(600, 600))
	w.SetResource(player, 0, 500)
	ctrl := prodCtrl{w}

	st := ai.NewCommandStream(4)
	ai.TrainUnits(st.Commander(player), int(pFootman), 1)
	var chosen int
	st.Drain(func(_ int, c ai.AICommand) { chosen, _ = ai.ApplyTrain(ctrl, c) })

	lo := int(b1.Index())
	if int(b2.Index()) < lo {
		lo = int(b2.Index())
	}
	t.Logf("FSV two idle barracks b1=%d b2=%d, single intent chose building=%d (want lowest=%d)",
		b1.Index(), b2.Index(), chosen, lo)
	if chosen != lo {
		t.Fatalf("chose building %d want lowest entity id %d", chosen, lo)
	}
}

// TestProductionFoodCapFSV — edge 4. At the food cap, a further train intent is
// gated (TrainNoFood) and is a no-op.
func TestProductionFoodCapFSV(t *testing.T) {
	const player = 1
	w := prodWorld277(t)
	w.SpawnFromTable(pBarrack, player, player, ptAt(200, 200))
	w.SetResource(player, 0, 500)
	w.SetFoodCap(player, 2) // room for exactly one 2-food footman
	ctrl := prodCtrl{w}
	pv := ai.NewProductionView(ctrl, player)

	t.Logf("FSV foodCap=%d foodUsed=%d before", w.FoodCap(player), w.FoodUsed(player))
	st := ai.NewCommandStream(8)
	ai.TrainUnits(st.Commander(player), int(pFootman), 2)
	var reasons []int
	st.Drain(func(_ int, c ai.AICommand) {
		_, r := ai.ApplyTrain(ctrl, c)
		reasons = append(reasons, r)
		t.Logf("FSV   apply → reason=%d foodUsed=%d/%d inProgress=%d",
			r, w.FoodUsed(player), w.FoodCap(player), pv.InProgress(int(pFootman)))
	})
	if len(reasons) != 2 || reasons[0] != ai.TrainOK || reasons[1] != ai.TrainNoFood {
		t.Fatalf("reasons=%v want [TrainOK(0) TrainNoFood(6)]", reasons)
	}
	if pv.InProgress(int(pFootman)) != 1 || pv.Queued(int(pFootman)) != 0 {
		t.Fatalf("counts %d/%d want 1/0 (food-gated second intent was a no-op)", pv.InProgress(int(pFootman)), pv.Queued(int(pFootman)))
	}
	if w.FoodUsed(player) != 2 {
		t.Fatalf("foodUsed=%d want 2 (only the admitted footman reserved food)", w.FoodUsed(player))
	}
}

// TestProductionProduceTargetFSV — the SetProduce "maintain N" analogue:
// ProduceTarget issues exactly enough intents to reach a population target and
// nothing once already at/over it.
func TestProductionProduceTargetFSV(t *testing.T) {
	const player = 1
	w := prodWorld277(t)
	w.SpawnFromTable(pBarrack, player, player, ptAt(200, 200))
	w.SetResource(player, 0, 500)
	ctrl := prodCtrl{w}
	pv := ai.NewProductionView(ctrl, player)
	st := ai.NewCommandStream(16)
	cmdr := st.Commander(player)

	// Target 3 from scratch (done=0, pending=0) → issues 3.
	n := ai.ProduceTarget(cmdr, pv, int(pFootman), 0, 3)
	t.Logf("FSV ProduceTarget(target=3, done=0) issued %d intents", n)
	if n != 3 {
		t.Fatalf("issued %d want 3", n)
	}
	st.Drain(func(_ int, c ai.AICommand) { ai.ApplyTrain(ctrl, c) })
	pending := pv.Pending(int(pFootman))
	t.Logf("FSV after applying: pending=%d", pending)
	if pending != 3 {
		t.Fatalf("pending=%d want 3", pending)
	}

	// Already at target (done=0, pending=3) → issues 0 (idempotent).
	n2 := ai.ProduceTarget(cmdr, pv, int(pFootman), 0, 3)
	t.Logf("FSV ProduceTarget(target=3) while pending=3 issued %d intents (want 0)", n2)
	if n2 != 0 || st.Len() != 0 {
		t.Fatalf("idempotent top-up issued %d (streamLen=%d) want 0/0", n2, st.Len())
	}
}
