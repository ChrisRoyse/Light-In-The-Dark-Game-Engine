package net

// #68 FSV: lockstep sim gating with REAL api.Game twins. SoT = the tick-advance
// log (Game.Tick vs the highest delivered turn), both twins' StateHash, and the
// OnCommandRecord consumption trace (records reach the sim at their explicit
// ticks). No mock driver — the gate drives a real deterministic sim.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	sim "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

type consumed struct {
	tick   uint32
	player uint8
	seq    uint16
	op     uint8
}

// newTwin makes a deterministic game with one player-0 unit and records every
// consumed command into *log.
func newTwin(t *testing.T) (*api.Game, uint32, *[]consumed) {
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
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	if u.ID() == 0 {
		t.Fatal("CreateUnit returned invalid unit")
	}
	log := &[]consumed{}
	g.OnCommandRecord(func(tick uint32, player uint8, seq uint16, op uint8) {
		*log = append(*log, consumed{tick, player, seq, op})
	})
	return g, u.ID(), log
}

// stopRec encodes a valid OpStop record (1 unit, player 0) for a tick/seq.
func stopRec(t *testing.T, tick uint32, seq uint16, unit uint32) []byte {
	t.Helper()
	r := sim.CommandRecord{Version: sim.CommandVersion, Tick: tick, Player: 0, Seq: seq, Opcode: sim.OpStop, UnitCount: 1}
	r.Units[0] = sim.EntityID(unit)
	b, ok := sim.AppendEncode(nil, &r)
	if !ok {
		t.Fatalf("encode stop record tick=%d seq=%d", tick, seq)
	}
	return b
}

func turnAgg(t *testing.T, recs ...[]byte) []byte {
	t.Helper()
	p, err := EncodeTurn(recs)
	if err != nil {
		t.Fatalf("EncodeTurn: %v", err)
	}
	return p
}

// TestLockstepBlocksUntilAggregate — the core invariant: the sim never enters a
// turn's ticks before that turn's aggregate is delivered.
func TestLockstepBlocksUntilAggregate(t *testing.T) {
	gate, err := NewLockstepGate(2) // turns span 2 ticks
	if err != nil {
		t.Fatalf("NewLockstepGate: %v", err)
	}
	g, unit, log := newTwin(t)

	// Deliver only turn 0 (ticks 1,2), with a command at tick 1.
	if err := gate.Deliver(0, turnAgg(t, stopRec(t, 1, 0, unit))); err != nil {
		t.Fatalf("Deliver 0: %v", err)
	}
	adv, waiting, blocked := gate.Pump(g)
	if !blocked || waiting != 1 {
		t.Fatalf("expected block at turn 1; blocked=%v waiting=%d", blocked, waiting)
	}
	if g.Tick() != 2 || adv != 2 {
		t.Fatalf("advanced to tick %d (adv=%d), want exactly 2 (turn 0's ticks) — sim entered turn 1 before its aggregate!", g.Tick(), adv)
	}
	if len(*log) != 1 || (*log)[0].tick != 1 || (*log)[0].seq != 0 {
		t.Fatalf("consumption log = %+v, want one record at tick 1 seq 0", *log)
	}
	t.Logf("FSV block: turn 0 delivered → advanced to tick %d then BLOCKED waiting on turn %d; cmd consumed at tick 1", g.Tick(), waiting)

	// Deliver turn 1 (ticks 3,4) → sim resumes through it.
	if err := gate.Deliver(1, turnAgg(t, stopRec(t, 3, 1, unit))); err != nil {
		t.Fatalf("Deliver 1: %v", err)
	}
	_, waiting2, blocked2 := gate.Pump(g)
	if g.Tick() != 4 || !blocked2 || waiting2 != 2 {
		t.Fatalf("after turn 1: tick=%d blocked=%v waiting=%d, want tick 4 blocked on turn 2", g.Tick(), blocked2, waiting2)
	}
	if len(*log) != 2 || (*log)[1].tick != 3 || (*log)[1].seq != 1 {
		t.Fatalf("after turn 1 consumption=%+v, want second record at tick 3 seq 1", *log)
	}
	t.Logf("FSV resume: turn 1 delivered → advanced to tick %d; cmd consumed at tick 3", g.Tick())
}

// TestLockstepHashEqualAcrossTiming — two twins fed the SAME aggregates with
// DIFFERENT delivery timing (bunched vs trickled) reach the same tick with equal
// StateHash. Determinism is independent of network timing.
func TestLockstepHashEqualAcrossTiming(t *testing.T) {
	gA, uA, _ := newTwin(t)
	gB, uB, _ := newTwin(t)
	gateA, _ := NewLockstepGate(2)
	gateB, _ := NewLockstepGate(2)

	// Same three turns (units have identical ids — deterministic creation).
	aggs := [][]byte{
		turnAgg(t, stopRec(t, 1, 0, uA)),
		turnAgg(t, stopRec(t, 3, 1, uA)),
		turnAgg(t, stopRec(t, 5, 2, uA)),
	}
	if uA != uB {
		t.Fatalf("twins have different unit ids %d vs %d — not deterministic", uA, uB)
	}

	// A: bunched — deliver all, then a single Pump catches up without skipping.
	for i, a := range aggs {
		if err := gateA.Deliver(uint64(i), a); err != nil {
			t.Fatalf("A deliver %d: %v", i, err)
		}
	}
	advA, _, _ := gateA.Pump(gA)

	// B: trickled — deliver one turn, pump, repeat.
	advB := 0
	for i, a := range aggs {
		if err := gateB.Deliver(uint64(i), a); err != nil {
			t.Fatalf("B deliver %d: %v", i, err)
		}
		n, _, _ := gateB.Pump(gB)
		advB += n
	}

	if gA.Tick() != 6 || gB.Tick() != 6 {
		t.Fatalf("ticks A=%d B=%d, want both 6", gA.Tick(), gB.Tick())
	}
	if advA != advB || advA != 6 {
		t.Fatalf("advanced A=%d B=%d, want both 6 (bunched must not skip ticks)", advA, advB)
	}
	hA, hB := gA.StateHash(), gB.StateHash()
	if hA != hB {
		t.Fatalf("HASH MISMATCH despite identical aggregates: A=%#x B=%#x", hA, hB)
	}
	t.Logf("FSV timing-independent: bunched(A) and trickled(B) both reached tick 6, StateHash %#x == %#x", hA, hB)
}

// TestLockstepOutOfOrderHeld — an aggregate for T+2 arriving before T+1 is held,
// never executed out of order.
func TestLockstepOutOfOrderHeld(t *testing.T) {
	gate, _ := NewLockstepGate(2)
	g, unit, log := newTwin(t)

	// Deliver turn 0 and turn 2, but NOT turn 1.
	if err := gate.Deliver(0, turnAgg(t, stopRec(t, 1, 0, unit))); err != nil {
		t.Fatal(err)
	}
	if err := gate.Deliver(2, turnAgg(t, stopRec(t, 5, 2, unit))); err != nil {
		t.Fatal(err)
	}
	_, waiting, blocked := gate.Pump(g)
	if !blocked || waiting != 1 || g.Tick() != 2 {
		t.Fatalf("out-of-order: tick=%d blocked=%v waiting=%d, want held at tick 2 waiting turn 1", g.Tick(), blocked, waiting)
	}
	// Turn 2's command (seq 2) must NOT have executed.
	for _, c := range *log {
		if c.seq == 2 {
			t.Fatalf("turn 2 command executed out of order before turn 1: %+v", c)
		}
	}
	t.Logf("FSV out-of-order: turn 2 buffered, sim held at tick 2 waiting on turn 1 (seq-2 cmd NOT executed)")

	// Fill the gap → sim advances through turns 1 AND 2.
	if err := gate.Deliver(1, turnAgg(t, stopRec(t, 3, 1, unit))); err != nil {
		t.Fatal(err)
	}
	gate.Pump(g)
	if g.Tick() != 6 {
		t.Fatalf("after filling gap: tick=%d, want 6 (caught up through turn 2)", g.Tick())
	}
	sawSeq2 := false
	for _, c := range *log {
		if c.seq == 2 {
			sawSeq2 = true
			if c.tick != 5 {
				t.Fatalf("seq-2 command executed at tick %d, want 5", c.tick)
			}
		}
	}
	if !sawSeq2 {
		t.Fatal("seq-2 command never executed after gap filled")
	}
	t.Logf("FSV gap filled: advanced to tick 6, turn-2 cmd executed at its tick 5 (in order)")
}

// TestLockstepDuplicateDeliverRejected — a turn delivered twice is fail-closed.
func TestLockstepDuplicateDeliverRejected(t *testing.T) {
	gate, _ := NewLockstepGate(2)
	_, unit, _ := newTwin(t)
	a := turnAgg(t, stopRec(t, 1, 0, unit))
	if err := gate.Deliver(0, a); err != nil {
		t.Fatalf("first deliver: %v", err)
	}
	if err := gate.Deliver(0, a); err == nil {
		t.Fatal("duplicate deliver of turn 0 accepted")
	}
	t.Log("FSV duplicate deliver rejected")
}
