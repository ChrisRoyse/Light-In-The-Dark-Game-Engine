package ai_test

// #398 — AI-disabled replay. The bug: melee-AI decisions are applied as direct
// sim mutations (detBridge → w.TrainForPlayer/HarvestAssign/PlaceBuildingNear/
// IssueOrder), never recorded, so a match cannot be replayed without re-running
// the AI. This proves the recordable path: a thin recording bridge taps every
// state-changing bridge call (with its tick) WITHOUT changing behavior, then a
// fresh world re-applies that command stream at the same AI-phase with NO
// controllers — and reaches a bit-identical sim state.
//
// SoT = statehash .Top of the sim world after the run (the match outcome),
// compared live-with-AI vs replay-without-AI. Recording is also proven
// non-intrusive: the recorded run's hash equals a plain run's hash.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/ai/melee"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/statehash"
)

// AI command kinds recorded at the bridge boundary.
const (
	aiHarvest uint8 = iota
	aiPlace
	aiTrain
	aiMove
	aiAttack
)

// aiCmd is one recorded AI decision: the tick it was issued, the kind, and the
// kind-specific params (self-contained — replay re-issues it verbatim).
type aiCmd struct {
	tick    uint32
	kind    uint8
	p       int
	a, b, c int32
}

// recBridge wraps *detBridge: it records every state-changing call into a shared
// log (stamped with the current tick) and then delegates to the real bridge, so
// the live run's behavior and every return value are byte-identical — the
// recording adds no sim mutation, so the determinism golden is untouched. The
// read methods come through the embedded *detBridge unchanged.
type recBridge struct {
	*detBridge
	log *[]aiCmd
}

func (r *recBridge) AssignHarvest(player, resource, count int) int {
	*r.log = append(*r.log, aiCmd{tick: r.w.Tick(), kind: aiHarvest, p: player, a: int32(resource), b: int32(count)})
	return r.detBridge.AssignHarvest(player, resource, count)
}

func (r *recBridge) PlaceBuilding(player, typeID int, cx, cy int32) bool {
	*r.log = append(*r.log, aiCmd{tick: r.w.Tick(), kind: aiPlace, p: player, a: int32(typeID), b: cx, c: cy})
	return r.detBridge.PlaceBuilding(player, typeID, cx, cy)
}

func (r *recBridge) TrainForPlayer(player, typeID int) (int, int) {
	*r.log = append(*r.log, aiCmd{tick: r.w.Tick(), kind: aiTrain, p: player, a: int32(typeID)})
	return r.detBridge.TrainForPlayer(player, typeID)
}

func (r *recBridge) OrderMoveTo(id, x, y int32) {
	*r.log = append(*r.log, aiCmd{tick: r.w.Tick(), kind: aiMove, a: id, b: x, c: y})
	r.detBridge.OrderMoveTo(id, x, y)
}

func (r *recBridge) OrderAttackTo(id, x, y int32) {
	*r.log = append(*r.log, aiCmd{tick: r.w.Tick(), kind: aiAttack, a: id, b: x, c: y})
	r.detBridge.OrderAttackTo(id, x, y)
}

// dAttachRec wires both controllers with recording bridges; identical to dAttach
// but every AI sim-mutation is logged.
func dAttachRec(t *testing.T, w *sim.World, vigil, unbound *melee.Strategy, log *[]aiCmd) dMatch {
	t.Helper()
	br1 := &recBridge{detBridge: &detBridge{w: w, player: 1}, log: log}
	br2 := &recBridge{detBridge: &detBridge{w: w, player: 2}, log: log}
	c1 := melee.NewController(vigil, dCfg(1, 1500, 1500, 3500, 1500), br1)
	c2 := melee.NewController(unbound, dCfg(2, 3500, 1500, 1500, 1500), br2)
	dom := ai.NewDomain()
	dom.SetDiagnostics(nil)
	dom.AddPlayer(1, br1, br1, ai.NewFuncController(c1.Step))
	dom.AddPlayer(2, br2, br2, ai.NewFuncController(c2.Step))
	w.OnAIPhase = func(uint32) { dom.Tick(0) }
	return dMatch{w: w, dom: dom, c1: c1, c2: c2}
}

// applyAILog re-issues the recorded commands whose tick == the current tick, in
// log (execution) order, through the SAME sim methods detBridge used — no AI.
func applyAILog(w *sim.World, tick uint32, log []aiCmd, next *int) {
	for *next < len(log) && log[*next].tick == tick {
		cmd := &log[*next]
		switch cmd.kind {
		case aiHarvest:
			w.HarvestAssign(uint8(cmd.p), int(cmd.a), int(cmd.b))
		case aiPlace:
			w.PlaceBuildingNear(uint8(cmd.p), uint16(cmd.a), dpt(cmd.b, cmd.c))
		case aiTrain:
			w.TrainForPlayer(uint8(cmd.p), uint16(cmd.a))
		case aiMove, aiAttack:
			w.IssueOrder(sim.EntityID(uint32(cmd.a)), sim.Order{Kind: sim.OrderMove, Point: dpt(cmd.b, cmd.c)}, false)
		}
		*next++
	}
}

func TestAIDisabledReplayFSV(t *testing.T) {
	const ticks = 2000
	reg := sim.NewHashRegistry()
	vigil, unbound := dLoadFactions(t)

	// 1. Record a live AI match.
	var log []aiCmd
	wRec := dWorld(t)
	dSetupUnits(t, wRec, 0)
	mRec := dAttachRec(t, wRec, vigil, unbound, &log)
	dStep(mRec, ticks)
	var snapLive statehash.Snapshot
	wRec.HashState(reg, &snapLive)

	// 1a. Recording is non-intrusive: the recorded run's sim hash equals a plain
	//     (non-recording) run's. If this fails, the tap itself perturbed the sim.
	mPlain := dNewMatch(t, vigil, unbound, 0, false)
	dStep(mPlain, ticks)
	var snapPlain statehash.Snapshot
	mPlain.w.HashState(reg, &snapPlain)
	if snapLive.Top != snapPlain.Top {
		t.Fatalf("recording perturbed the live sim: recorded=%016x plain=%016x", snapLive.Top, snapPlain.Top)
	}

	var k [5]int
	for _, c := range log {
		k[c.kind]++
	}
	t.Logf("FSV #398: recorded %d AI commands over %d ticks (harvest=%d place=%d train=%d move=%d attack=%d)",
		len(log), ticks, k[aiHarvest], k[aiPlace], k[aiTrain], k[aiMove], k[aiAttack])

	// 2. Replay the recorded command stream with NO controllers: the AI phase
	//    re-applies the log at the same w.Tick() it was recorded at.
	wRep := dWorld(t)
	dSetupUnits(t, wRep, 0)
	next := 0
	wRep.OnAIPhase = func(uint32) { applyAILog(wRep, wRep.Tick(), log, &next) }
	for i := 0; i < ticks; i++ {
		wRep.RecomputeVisibility()
		wRep.Step()
	}
	var snapRep statehash.Snapshot
	wRep.HashState(reg, &snapRep)

	// 3. SoT: AI-disabled replay reproduces the live match's sim state exactly.
	t.Logf("FSV #398: live=%016x replay=%016x applied=%d/%d", snapLive.Top, snapRep.Top, next, len(log))
	if next != len(log) {
		t.Fatalf("replay consumed %d of %d commands — tick addressing mismatch", next, len(log))
	}
	if snapRep.Top != snapLive.Top {
		culprit, _ := sim.CompareCheckpoint(&sim.ReplayCheckpoint{Top: snapLive.Top, Subs: snapLive.Subs}, &snapRep)
		t.Fatalf("AI-disabled replay diverged: replay=%016x live=%016x (first culprit system=%s)", snapRep.Top, snapLive.Top, culprit)
	}
	t.Log("AI-disabled replay OK: the recorded AI command stream reproduces the live match with NO controllers running (#398)")
}
