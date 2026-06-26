package net

import (
	"fmt"
	"sort"
	"time"
)

// RoundGate is the stall-aware lockstep accumulator (#71 capstone): it composes the
// per-peer read pumps (deadlined reads) with the StallController (pause/grace/drop
// policy) into one round loop that holds partial submissions across retries.
//
// The plain Host.CollectRound blocks the whole round on the slowest peer and
// instant-drops on any read error — a merely-slow-but-alive peer either hangs the
// match or is killed with no grace. RoundGate instead Steps: each Step polls every
// not-yet-collected peer with a short timeout, banks the turns that arrive (held
// across Steps for the same round), and — if anyone is still missing — consults the
// StallController. Within the grace window it returns GateWaiting (the data the
// stall overlay renders: who is lagging + how long until they drop). When grace
// expires it drops exactly the laggards and aggregates over the survivors. A peer
// whose stream cleanly closes is dropped immediately (a departure, not a stall — no
// overlay). The caller loops Step until it returns GateAggregated.
//
// `now` is an injected monotonic clock (time.Duration since match start), so the
// grace logic is deterministic and testable without real time; only the per-peer
// poll uses a real short timeout against the live stream.

// GateStatus is the outcome of one Step.
type GateStatus uint8

const (
	// GateWaiting: the round cannot complete yet — at least one peer is lagging and
	// still inside the grace window. Waiting/Remaining drive the stall overlay.
	GateWaiting GateStatus = iota
	// GateAggregated: the round is complete. Payload is the broadcast turn and
	// Roster is the surviving player set; Dropped lists any peers grace-expired or
	// departed this round.
	GateAggregated
)

func (s GateStatus) String() string {
	if s == GateAggregated {
		return "aggregated"
	}
	return "waiting"
}

// GateStep is one Step's result.
type GateStep struct {
	Status    GateStatus
	Turn      uint64
	Payload   []byte        // valid when Status==GateAggregated
	Roster    []uint8       // surviving players (host + live peers), sorted
	Waiting   []uint8       // lagging player ids, sorted (valid when Status==GateWaiting)
	Remaining time.Duration // grace left before the laggards drop
	Dropped   []uint8       // peers dropped this round (grace-expired or departed), sorted
}

// RoundGate owns the peer pumps and the in-progress round state.
type RoundGate struct {
	turnLen     int
	ctrl        *StallController
	pollTimeout time.Duration

	pumps map[uint8]*peerPump

	// round-in-progress state, valid while active:
	active bool
	turn   uint64
	have   map[uint8][][]byte // player -> decoded records banked this round
}

// NewRoundGate builds a gate. turnLen is the per-turn tick span; ctrl is the grace
// policy; pollTimeout is the per-peer read budget per Step (must be > 0).
func NewRoundGate(turnLen int, ctrl *StallController, pollTimeout time.Duration) (*RoundGate, error) {
	if ctrl == nil {
		return nil, fmt.Errorf("net: round gate needs a stall controller")
	}
	if pollTimeout <= 0 {
		return nil, fmt.Errorf("net: round gate poll timeout must be > 0, got %v", pollTimeout)
	}
	// turnLen is validated by NewTurnBuffer at aggregation; surface an early error.
	if turnLen < minTurnLen || turnLen > maxTurnLen {
		return nil, fmt.Errorf("net: round gate turn length %d out of [%d,%d]", turnLen, minTurnLen, maxTurnLen)
	}
	return &RoundGate{
		turnLen:     turnLen,
		ctrl:        ctrl,
		pollTimeout: pollTimeout,
		pumps:       make(map[uint8]*peerPump),
	}, nil
}

// AddPeer registers a peer's session under its player id, creating its read pump.
// Call once per admitted peer before stepping rounds.
func (g *RoundGate) AddPeer(player uint8, sess *Session) error {
	if player == HostPlayer {
		return fmt.Errorf("net: round gate: player %d is the host slot", HostPlayer)
	}
	if _, dup := g.pumps[player]; dup {
		return fmt.Errorf("net: round gate: player %d already registered", player)
	}
	g.pumps[player] = newPeerPump(sess)
	return nil
}

// Peers returns the currently-registered peer ids, sorted (host excluded).
func (g *RoundGate) Peers() []uint8 { return g.sortedPumpIDs() }

func (g *RoundGate) sortedPumpIDs() []uint8 {
	ids := make([]uint8, 0, len(g.pumps))
	for id := range g.pumps {
		ids = append(ids, id)
	}
	sort.Slice(ids, func(i, j int) bool { return ids[i] < ids[j] })
	return ids
}

func (g *RoundGate) dropPump(player uint8) {
	if p, ok := g.pumps[player]; ok {
		_ = p.Close()
		delete(g.pumps, player)
	}
}

// rosterLocked-free: host + live peers, sorted.
func (g *RoundGate) roster() []uint8 {
	r := append([]uint8{HostPlayer}, g.sortedPumpIDs()...)
	return r
}

// Step advances the round for turn. hostRecords are the host player's own records
// this round (the host never stalls). now is the injected monotonic clock. The
// first Step for a new turn snapshots the host's submission; subsequent Steps for
// the same turn re-poll only the still-missing peers. Returns GateWaiting while a
// laggard is inside grace, GateAggregated once everyone present has submitted or
// the laggards have been dropped.
func (g *RoundGate) Step(turn uint64, hostRecords [][]byte, now time.Duration) (GateStep, error) {
	if !g.active || g.turn != turn {
		g.active = true
		g.turn = turn
		g.have = map[uint8][][]byte{HostPlayer: append([][]byte(nil), hostRecords...)}
	}

	var departed []uint8
	for _, id := range g.sortedPumpIDs() {
		if _, banked := g.have[id]; banked {
			continue // already collected this round
		}
		payload, st := g.pumps[id].Poll(g.pollTimeout)
		switch st {
		case PumpReady:
			recs, err := DecodeTurn(payload)
			if err != nil {
				// A peer that sends a malformed turn is treated as departed
				// (fail-closed: never aggregate garbage).
				departed = append(departed, id)
				g.dropPump(id)
				continue
			}
			g.have[id] = recs
		case PumpClosed:
			departed = append(departed, id)
			g.dropPump(id)
		case PumpPending:
			// still lagging; leave it for the stall policy below
		}
	}

	// Who is still missing (registered, alive, not yet submitted)?
	var missing []uint8
	for _, id := range g.sortedPumpIDs() {
		if _, banked := g.have[id]; !banked {
			missing = append(missing, id)
		}
	}

	if len(missing) == 0 {
		// Everyone present has submitted. Clear any prior Waiting phase, aggregate.
		g.ctrl.Observe(false, turn, nil, now)
		step, err := g.aggregate(turn)
		if err != nil {
			return GateStep{}, err
		}
		step.Dropped = sortedU8(departed)
		return step, nil
	}

	decision := g.ctrl.Observe(true, turn, missing, now)
	if decision.Resumed && len(decision.Dropped) > 0 {
		// Grace expired: drop the laggards, aggregate over the survivors.
		for _, id := range decision.Dropped {
			g.dropPump(id)
		}
		step, err := g.aggregate(turn)
		if err != nil {
			return GateStep{}, err
		}
		dropped := append([]uint8(nil), departed...)
		dropped = append(dropped, decision.Dropped...)
		step.Dropped = sortedU8(dropped)
		return step, nil
	}

	// Still waiting inside the grace window.
	return GateStep{
		Status:    GateWaiting,
		Turn:      turn,
		Waiting:   sortedU8(missing),
		Remaining: decision.Remaining,
		Dropped:   sortedU8(departed),
	}, nil
}

// aggregate builds the broadcast payload over exactly the banked submissions and
// ends the round.
func (g *RoundGate) aggregate(turn uint64) (GateStep, error) {
	players := make([]uint8, 0, len(g.have))
	for id := range g.have {
		players = append(players, id)
	}
	tb, err := NewTurnBuffer(g.turnLen, players)
	if err != nil {
		return GateStep{}, fmt.Errorf("net: round %d gate buffer: %w", turn, err)
	}
	for id, recs := range g.have {
		if err := tb.Submit(turn, id, recs); err != nil {
			return GateStep{}, fmt.Errorf("net: round %d gate submit player %d: %w", turn, id, err)
		}
	}
	payload, err := tb.Aggregate(turn)
	if err != nil {
		return GateStep{}, fmt.Errorf("net: round %d gate aggregate: %w", turn, err)
	}
	g.active = false
	g.have = nil
	return GateStep{
		Status:  GateAggregated,
		Turn:    turn,
		Payload: payload,
		Roster:  g.roster(),
	}, nil
}

func sortedU8(in []uint8) []uint8 {
	if len(in) == 0 {
		return nil
	}
	out := append([]uint8(nil), in...)
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

// Close releases every peer pump.
func (g *RoundGate) Close() error {
	for id := range g.pumps {
		_ = g.pumps[id].Close()
	}
	g.pumps = make(map[uint8]*peerPump)
	return nil
}
