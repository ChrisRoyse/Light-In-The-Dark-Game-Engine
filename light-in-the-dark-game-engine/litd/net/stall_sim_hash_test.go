package net

import (
	"bytes"
	"testing"
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	sim "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// newStallTwin builds a deterministic game with a host unit (player 0) and a
// laggard unit (player 2). Three calls with the same seed produce byte-identical
// games — the survivor clients and the negative control.
func newStallTwin(t *testing.T) (g *api.Game, hostUnit, lagUnit uint32) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 7})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 4 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	u0 := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	u2 := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 300, Y: 300}, api.Deg(0))
	if u0.ID() == 0 || u2.ID() == 0 {
		t.Fatal("CreateUnit returned invalid unit")
	}
	return g, u0.ID(), u2.ID()
}

// stallRec encodes one command record for an arbitrary player/unit — needed
// because moveRec/stopRec hardcode player 0. Point is ignored for OpStop.
func stallRec(t *testing.T, tick uint32, seq uint16, player uint8, op uint8, unit uint32, x, y int) []byte {
	t.Helper()
	r := sim.CommandRecord{
		Version: sim.CommandVersion, Tick: tick, Player: player, Seq: seq,
		Opcode: op, UnitCount: 1,
		Point: fixed.Vec2{X: fixed.FromInt(int32(x)), Y: fixed.FromInt(int32(y))},
	}
	r.Units[0] = sim.EntityID(unit)
	b, ok := sim.AppendEncode(nil, &r)
	if !ok {
		t.Fatalf("encode player=%d op=%d tick=%d failed", player, op, tick)
	}
	return b
}

// TestStallDropSurvivorsHashEqualFSV is #71's FSV edge 2 wired end-to-end: a real
// stall→grace-expiry→drop on the RoundGate must produce a survivor-only broadcast
// that drives the SURVIVING clients' real sims to an identical StateHash at the
// resume tick, and the dropped player's unit must simply stop receiving orders.
//
// The pieces are proven separately — roundgate_test proves the survivor payload is
// byte-exact over the survivors, lockstep_test proves an identical aggregate drives
// twins to equal hashes — but nothing wired the drop's actual output through real
// survivor sims. This does, along the production seam (RoundGate aggregate →
// LockstepGate.Deliver → Pump → api.Game; host.go:180, observer.go:16). So
// "remaining clients' hashes equal post-resume" is direct evidence, not a
// composition argument.
//
// Roles: host (player 0) drives its unit's motion; peer 1 is a live survivor; peer
// 2 owns a unit and STALLS on the final turn — its would-be move is dropped, so its
// unit holds position on every survivor. turnLen=2, so turn N covers ticks
// 2N+1..2N+2 and each turn's records sit at its first tick.
//
// SoT = both survivors' StateHash at the resume tick. Non-vacuity: the host unit
// travels; the drop removes the laggard's real move (the survivor payload differs
// from the all-three aggregate byte-wise AND in resulting sim state); and the
// negative control — a client that kept the laggard's turn — ends on a different
// hash.
func TestStallDropSurvivorsHashEqualFSV(t *testing.T) {
	gA, hostU, lagU := newStallTwin(t)
	gB, hostU2, lagU2 := newStallTwin(t)
	if hostU != hostU2 || lagU != lagU2 {
		t.Fatalf("twins diverged at creation: host %d/%d lag %d/%d", hostU, hostU2, lagU, lagU2)
	}
	gateA, _ := NewLockstepGate(2)
	gateB, _ := NewLockstepGate(2)

	ctrl, err := NewStallController(30 * time.Second)
	if err != nil {
		t.Fatalf("NewStallController: %v", err)
	}
	gate, err := NewRoundGate(2, ctrl, 150*time.Millisecond)
	if err != nil {
		t.Fatalf("NewRoundGate: %v", err)
	}
	defer gate.Close()
	near1, far1 := pipePair()
	near2, far2 := pipePair()
	if err := gate.AddPeer(1, near1); err != nil {
		t.Fatalf("AddPeer 1: %v", err)
	}
	if err := gate.AddPeer(2, near2); err != nil {
		t.Fatalf("AddPeer 2: %v", err)
	}

	deliver := func(turn uint64, payload []byte) {
		t.Helper()
		if err := gateA.Deliver(turn, payload); err != nil {
			t.Fatalf("A deliver turn %d: %v", turn, err)
		}
		if err := gateB.Deliver(turn, payload); err != nil {
			t.Fatalf("B deliver turn %d: %v", turn, err)
		}
		gateA.Pump(gA)
		gateB.Pump(gB)
		if gA.Tick() != gB.Tick() {
			t.Fatalf("turn %d: survivor ticks diverged A=%d B=%d", turn, gA.Tick(), gB.Tick())
		}
		if gA.StateHash() != gB.StateHash() {
			t.Fatalf("turn %d: survivor StateHash diverged A=%#x B=%#x", turn, gA.StateHash(), gB.StateHash())
		}
	}

	// --- turns 0,1: all three present. Host unit moves then halts; laggard unit
	// idles (stop on its own unit). ---
	host0 := [][]byte{moveRec(t, 1, 0, hostU, 5000, 100)}
	sendPeerTurn(t, far1, [][]byte{gateRecord(t, 1, 1, 0)})
	sendPeerTurn(t, far2, [][]byte{stallRec(t, 1, 0, 2, sim.OpStop, lagU, 0, 0)})
	s0, err := gate.Step(0, host0, 0)
	if err != nil || s0.Status != GateAggregated {
		t.Fatalf("turn 0 step: status=%v err=%v, want aggregated", s0.Status, err)
	}
	if !bytes.Equal(asBytes(s0.Roster), []byte{0, 1, 2}) {
		t.Fatalf("turn 0 roster=%v, want [0 1 2]", s0.Roster)
	}
	deliver(0, s0.Payload)

	host1 := [][]byte{stopRec(t, 3, 1, hostU)}
	sendPeerTurn(t, far1, [][]byte{gateRecord(t, 3, 1, 1)})
	sendPeerTurn(t, far2, [][]byte{stallRec(t, 3, 1, 2, sim.OpStop, lagU, 0, 0)})
	s1, err := gate.Step(1, host1, 0)
	if err != nil || s1.Status != GateAggregated {
		t.Fatalf("turn 1 step: status=%v err=%v, want aggregated", s1.Status, err)
	}
	deliver(1, s1.Payload)

	hPre := gA.StateHash()
	if hPre != gB.StateHash() {
		t.Fatalf("pre-stall hashes already differ: A=%#x B=%#x", hPre, gB.StateHash())
	}
	t.Logf("FSV pre-stall: both survivors at tick %d, StateHash %#x (host unit moved — non-trivial)", gA.Tick(), hPre)

	// --- turn 2: peer 2 STALLS. Its would-be order is a real MOVE of its unit;
	// only host + peer 1 actually submit. ---
	host2 := [][]byte{stopRec(t, 5, 2, hostU)}
	lagMove2 := stallRec(t, 5, 2, 2, sim.OpMove, lagU, 9000, 9000) // dropped — never reaches survivors
	sendPeerTurn(t, far1, [][]byte{gateRecord(t, 5, 1, 2)})
	// peer 2 sends nothing.

	w, err := gate.Step(2, host2, 0)
	if err != nil {
		t.Fatalf("turn 2 step@0: %v", err)
	}
	if w.Status != GateWaiting || !bytes.Equal(asBytes(w.Waiting), []byte{2}) {
		t.Fatalf("turn 2 @t=0: status=%v waiting=%v, want waiting [2]", w.Status, w.Waiting)
	}
	if w.Remaining != 30*time.Second {
		t.Fatalf("turn 2 @t=0: remaining=%v, want full 30s grace", w.Remaining)
	}
	t.Logf("FSV stall begin: round 2 blocked, waiting on player %v, grace %v", w.Waiting, w.Remaining)

	d, err := gate.Step(2, host2, 35*time.Second)
	if err != nil {
		t.Fatalf("turn 2 step@35s: %v", err)
	}
	if d.Status != GateAggregated || !bytes.Equal(asBytes(d.Dropped), []byte{2}) {
		t.Fatalf("turn 2 @t=35s: status=%v dropped=%v, want aggregated dropped [2]", d.Status, d.Dropped)
	}
	if !bytes.Equal(asBytes(d.Roster), []byte{0, 1}) {
		t.Fatalf("post-drop roster=%v, want survivors [0 1]", d.Roster)
	}

	// The drop genuinely changed the broadcast: the survivor payload is NOT the
	// payload that would have carried the laggard's move.
	allThree := refAggregate(t, 2, 2, map[uint8][][]byte{
		0: host2,
		1: {gateRecord(t, 5, 1, 2)},
		2: {lagMove2},
	})
	if bytes.Equal(d.Payload, allThree) {
		t.Fatal("survivor payload equals the all-three aggregate — the drop did not remove the laggard")
	}

	deliver(2, d.Payload)

	hA, hB := gA.StateHash(), gB.StateHash()
	t.Logf("FSV resume: dropped=%v roster=%v → survivor A tick=%d hash=%#x | survivor B tick=%d hash=%#x",
		d.Dropped, d.Roster, gA.Tick(), hA, gB.Tick(), hB)
	if gA.Tick() != 6 || gB.Tick() != 6 {
		t.Fatalf("post-resume ticks A=%d B=%d, want both 6", gA.Tick(), gB.Tick())
	}
	if hA != hB {
		t.Fatalf("HASH DIVERGENCE post-drop: survivors disagree A=%#x B=%#x — a dropped-player stall desynced the match", hA, hB)
	}

	// --- negative control: a client that KEPT the laggard's turn (applied the
	// all-three aggregate instead of the survivor broadcast) lets the laggard unit
	// move, so it ends on a different hash. The equality above has teeth. ---
	gC, _, _ := newStallTwin(t)
	gateC, _ := NewLockstepGate(2)
	if err := gateC.Deliver(0, s0.Payload); err != nil {
		t.Fatal(err)
	}
	if err := gateC.Deliver(1, s1.Payload); err != nil {
		t.Fatal(err)
	}
	if err := gateC.Deliver(2, allThree); err != nil { // wrong: kept the dropped laggard move
		t.Fatal(err)
	}
	gateC.Pump(gC)
	if gC.StateHash() == hA {
		t.Fatalf("control kept the laggard's move yet hashes equal to the survivor run %#x — the laggard's drop had no observable effect", hA)
	}
	t.Logf("FSV control: a client that kept the un-dropped laggard move ends on %#x != survivor %#x — drop is observable, equality has teeth", gC.StateHash(), hA)
}
