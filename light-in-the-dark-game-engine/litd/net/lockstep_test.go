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
	return newTwinSeeded(t, 7)
}

// newTwinSeeded is newTwin with an explicit PRNG seed — used to prove the lobby's
// StartParams.Seed actually reaches the sim.
func newTwinSeeded(t *testing.T, seed int64) (*api.Game, uint32, *[]consumed) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: seed})
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

// moveRec encodes an OpMove toward (x,y) world units for player 0's unit — used
// to put the unit in motion so a later OpStop's TIMING is observable in state.
func moveRec(t *testing.T, tick uint32, seq uint16, unit uint32, x, y int) []byte {
	t.Helper()
	r := sim.CommandRecord{
		Version: sim.CommandVersion, Tick: tick, Player: 0, Seq: seq,
		Opcode: sim.OpMove, UnitCount: 1,
		Point: fixed.Vec2{X: fixed.FromInt(int32(x)), Y: fixed.FromInt(int32(y))},
	}
	r.Units[0] = sim.EntityID(unit)
	b, ok := sim.AppendEncode(nil, &r)
	if !ok {
		t.Fatalf("encode move record tick=%d seq=%d", tick, seq)
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

// TestHostAggregateDrivesLockstepSimFSV — the FULL net→sim chain end to end: the
// REAL host loop (Host.CollectRound) produces each round's aggregate, and two
// independent clients feed that exact aggregate through their LockstepGate into a
// real api.Game. Both must reach the same tick with equal StateHash, and both
// must consume the host's commands at their explicit ticks. This is the
// "lockstep sim integration" seam — Host aggregation wired to the deterministic
// sim — verified without a network: the prior tests prove Host.CollectRound and
// gate→sim separately; this proves they compose into a deterministic match.
func TestHostAggregateDrivesLockstepSimFSV(t *testing.T) {
	h, err := NewHost(HostOptions{BuildHash: hostBuild, Seed: hostSeed, Capacity: 2, TurnLen: 2})
	if err != nil {
		t.Fatalf("NewHost: %v", err)
	}
	defer h.Close()
	if h.PlayerCount() != 1 {
		t.Fatalf("host-only player count = %d, want 1", h.PlayerCount())
	}

	gA, uA, logA := newTwin(t)
	gB, uB, logB := newTwin(t)
	if uA != uB {
		t.Fatalf("twins have different unit ids %d vs %d — not deterministic", uA, uB)
	}
	gateA, _ := NewLockstepGate(2)
	gateB, _ := NewLockstepGate(2)

	// Three rounds. Turn 0 puts the unit in MOTION (move far along +x at tick 1),
	// turn 1 halts it at tick 3, turn 2 is an idempotent stop — so the state the
	// hash covers is non-trivial (the unit actually travelled), not an idle no-op.
	// The real Host.CollectRound aggregates each turn; BOTH clients deliver + pump.
	recOf := func(turn int, unit uint32) []byte {
		switch turn {
		case 0:
			return moveRec(t, 1, 0, unit, 5000, 100)
		case 1:
			return stopRec(t, 3, 1, unit)
		default:
			return stopRec(t, 5, 2, unit)
		}
	}
	advA, advB := 0, 0
	for turn := 0; turn < 3; turn++ {
		hostRec := recOf(turn, uint32(uA))
		payload, roster, err := h.CollectRound(uint64(turn), [][]byte{hostRec})
		if err != nil {
			t.Fatalf("CollectRound %d: %v", turn, err)
		}
		if !eq(roster, []uint8{HostPlayer}) {
			t.Fatalf("turn %d roster %v, want [%d]", turn, roster, HostPlayer)
		}
		if err := gateA.Deliver(uint64(turn), payload); err != nil {
			t.Fatalf("A deliver %d: %v", turn, err)
		}
		if err := gateB.Deliver(uint64(turn), payload); err != nil {
			t.Fatalf("B deliver %d: %v", turn, err)
		}
		nA, _, _ := gateA.Pump(gA)
		nB, _, _ := gateB.Pump(gB)
		advA += nA
		advB += nB
	}

	if gA.Tick() != 6 || gB.Tick() != 6 {
		t.Fatalf("ticks A=%d B=%d, want both 6", gA.Tick(), gB.Tick())
	}
	if advA != 6 || advB != 6 {
		t.Fatalf("advanced A=%d B=%d, want both 6", advA, advB)
	}
	hA, hB := gA.StateHash(), gB.StateHash()
	if hA != hB {
		t.Fatalf("HASH MISMATCH: the host-aggregated stream diverged the two sims A=%#x B=%#x", hA, hB)
	}
	if len(*logA) != 3 || len(*logB) != 3 {
		t.Fatalf("consumed A=%d B=%d records, want 3 each (the host's per-round commands)", len(*logA), len(*logB))
	}
	t.Logf("FSV host→gate→sim: 3 rounds of real Host.CollectRound aggregates drove both api.Game clients to tick 6, StateHash %#x == %#x; %d records consumed each", hA, hB, len(*logA))
}

// TestHostAggregateMismatchDivergesFSV — the negative control for the test above:
// if one client's host aggregate carries a command at a DIFFERENT tick, the two
// sims MUST end with different StateHash. Without this, equal hashes above could
// be a vacuous pass (e.g. commands that never reach the sim). Here client B's
// round-1 stop is aggregated for tick 4 instead of tick 3, so B's unit halts one
// tick later than A's → divergent state → divergent hash. SoT = the two hashes.
func TestHostAggregateMismatchDivergesFSV(t *testing.T) {
	h, err := NewHost(HostOptions{BuildHash: hostBuild, Seed: hostSeed, Capacity: 2, TurnLen: 2})
	if err != nil {
		t.Fatalf("NewHost: %v", err)
	}
	defer h.Close()

	gA, uA, _ := newTwin(t)
	gB, uB, _ := newTwin(t)
	if uA != uB {
		t.Fatalf("twins differ: %d vs %d", uA, uB)
	}
	gateA, _ := NewLockstepGate(2)
	gateB, _ := NewLockstepGate(2)

	// Turn 0 puts both units in motion (move at tick 1). Turn 1: A halts at tick
	// 3, B halts at tick 4 — the one-tick shift means B travels one extra tick, so
	// the units rest at different positions. Turn 2 is an idempotent stop.
	recOf := func(turn int, unit uint32, stopTick uint32) []byte {
		switch turn {
		case 0:
			return moveRec(t, 1, 0, unit, 5000, 100)
		case 1:
			return stopRec(t, stopTick, 1, unit)
		default:
			return stopRec(t, 5, 2, unit)
		}
	}
	for turn := 0; turn < 3; turn++ {
		payA, _, err := h.CollectRound(uint64(turn), [][]byte{recOf(turn, uint32(uA), 3)})
		if err != nil {
			t.Fatalf("A CollectRound %d: %v", turn, err)
		}
		payB := turnAgg(t, recOf(turn, uint32(uB), 4))
		if err := gateA.Deliver(uint64(turn), payA); err != nil {
			t.Fatalf("A deliver %d: %v", turn, err)
		}
		if err := gateB.Deliver(uint64(turn), payB); err != nil {
			t.Fatalf("B deliver %d: %v", turn, err)
		}
		gateA.Pump(gA)
		gateB.Pump(gB)
	}
	if gA.Tick() != 6 || gB.Tick() != 6 {
		t.Fatalf("ticks A=%d B=%d, want both 6", gA.Tick(), gB.Tick())
	}
	hA, hB := gA.StateHash(), gB.StateHash()
	if hA == hB {
		t.Fatalf("expected divergence from the one-tick command shift, but hashes match: %#x", hA)
	}
	t.Logf("FSV divergence control: B's stop shifted tick 3→4 → StateHash %#x != %#x (the equality check has teeth)", hA, hB)
}

// TestLobbyStartConfiguresLockstepSessionFSV — the lobby→session bootstrap seam.
// Lobby.Start() returns the StartParams (seed, turn length, input delay) the
// issue says must configure the lockstep session; this proves they actually do.
// A session built from those params (a) advances exactly TurnLen ticks per turn,
// (b) is deterministic across twins, and (c) reflects the seed — a different seed
// diverges, so the lobby's seed genuinely reaches the sim and isn't dropped.
func TestLobbyStartConfiguresLockstepSessionFSV(t *testing.T) {
	lob, err := NewLobby(2, "host", lobbyParams())
	if err != nil {
		t.Fatalf("NewLobby: %v", err)
	}
	if _, err := lob.Join("client"); err != nil {
		t.Fatalf("Join: %v", err)
	}
	if err := lob.SetReady(1, true); err != nil {
		t.Fatalf("SetReady: %v", err)
	}
	params, err := lob.Start()
	if err != nil {
		t.Fatalf("Start: %v", err)
	}
	if params.Seed != 0xABCDEF || params.TurnLen != 3 {
		t.Fatalf("start params = %+v, want seed 0xABCDEF turnLen 3", params)
	}

	// Build the session from the start params and run one turn.
	runOneTurn := func(seed uint64) (*api.Game, int) {
		gate, gerr := NewLockstepGate(params.TurnLen)
		if gerr != nil {
			t.Fatalf("NewLockstepGate(%d): %v", params.TurnLen, gerr)
		}
		g, unit, _ := newTwinSeeded(t, int64(seed))
		if derr := gate.Deliver(0, turnAgg(t, stopRec(t, 1, 0, unit))); derr != nil {
			t.Fatalf("Deliver: %v", derr)
		}
		adv, _, _ := gate.Pump(g)
		return g, adv
	}

	// (a) TurnLen wired: one turn advances exactly TurnLen ticks.
	g, adv := runOneTurn(params.Seed)
	if adv != params.TurnLen || g.Tick() != uint32(params.TurnLen) {
		t.Fatalf("turn 0 advanced %d ticks (game tick %d), want TurnLen=%d", adv, g.Tick(), params.TurnLen)
	}

	// (b) deterministic: same params → same hash.
	g2, _ := runOneTurn(params.Seed)
	if g.StateHash() != g2.StateHash() {
		t.Fatalf("same-params twins diverged: %#x != %#x", g.StateHash(), g2.StateHash())
	}

	// (c) seed wired: a different seed diverges, proving StartParams.Seed reaches
	// the sim rather than being ignored.
	gOther, _ := runOneTurn(params.Seed + 1)
	if g.StateHash() == gOther.StateHash() {
		t.Fatalf("seed ignored: session seed %#x hashes identically to %#x", params.Seed, params.Seed+1)
	}
	t.Logf("FSV lobby→session: Start{seed=%#x turnLen=%d delay=%d} → %d ticks/turn; same-seed twins %#x==%#x; seed+1 diverges (%#x)",
		params.Seed, params.TurnLen, params.InputDelay, adv, g.StateHash(), g2.StateHash(), gOther.StateHash())
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
