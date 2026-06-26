package net

// host.go: the in-process LAN host / star-center loop (#62, D-2026-06-11-26). The
// hosting player's engine runs the aggregation loop in-process: it admits 1–7
// remote clients (2–8 players total, D-5), collects every player's command turn
// each round — the host's own turns enter aggregation locally with NO network
// round-trip — aggregates them into one ordered payload (TurnBuffer, #65), and
// broadcasts the identical payload to all clients (the lockstep guarantee).
//
// The same loop code is reused UNMODIFIED by cmd/litd-relay (#64): a relay is
// just a Host with no local sim and no host-player turns. The aggregation/roster
// machinery here is sim-agnostic — the host's own sim advances separately, gated
// by the LockstepGate on the payload this loop returns. So this file imports
// neither litd/sim nor litd/api (the net→sim direction stays closed).
//
// Threading: the accept loop (Serve/Admit) and the turn loop (CollectRound) are
// driven by the caller OUTSIDE the sim tick, so hosting never blocks rendering.
// Roster reads/writes are mutex-guarded; Admit is called serially from one accept
// goroutine.

import (
	"fmt"
	"sort"
	"sync"
)

// HostPlayer is the hosting player's fixed slot.
const HostPlayer uint8 = 0

// HostOptions configures a Host.
type HostOptions struct {
	BuildHash string // join guard cross-checks this (#74)
	Seed      uint64 // map/PRNG seed every peer must share
	Capacity  int    // total players including the host (2–8)
	TurnLen   int    // ticks per turn (2–4)
}

type clientConn struct {
	player uint8
	sess   *Session
	addr   string
}

// Host is the in-process star center. Safe for concurrent Roster/Remove while a
// single accept goroutine calls Admit.
type Host struct {
	buildHash string
	seed      uint64
	capacity  int
	turnLen   int

	mu      sync.Mutex
	clients map[uint8]*clientConn // remote player id → conn (host is implicit)
	used    []bool                // slot occupancy; index 0 is the host
	events  []string              // join/departure/refusal log — the FSV source of truth
}

// NewHost builds a host for capacity total players (2–8) and turnLen-tick turns.
func NewHost(opts HostOptions) (*Host, error) {
	if opts.Capacity < 2 || opts.Capacity > 8 {
		return nil, fmt.Errorf("net: host capacity %d out of [2,8]", opts.Capacity)
	}
	if opts.TurnLen < minTurnLen || opts.TurnLen > maxTurnLen {
		return nil, fmt.Errorf("net: host turn length %d out of [%d,%d]", opts.TurnLen, minTurnLen, maxTurnLen)
	}
	h := &Host{
		buildHash: opts.BuildHash,
		seed:      opts.Seed,
		capacity:  opts.Capacity,
		turnLen:   opts.TurnLen,
		clients:   make(map[uint8]*clientConn),
		used:      make([]bool, opts.Capacity),
	}
	h.used[HostPlayer] = true // the host always occupies slot 0
	h.logf("host up: capacity=%d turnLen=%d (host is player %d)", opts.Capacity, opts.TurnLen, HostPlayer)
	return h, nil
}

func (h *Host) logf(format string, a ...any) {
	h.events = append(h.events, fmt.Sprintf(format, a...))
}

// playerCountLocked returns the current total players (host + remotes).
func (h *Host) playerCountLocked() int { return 1 + len(h.clients) }

// freeSlotLocked finds the lowest unused player id, or (0,false) if full.
func (h *Host) freeSlotLocked() (uint8, bool) {
	for i := 1; i < len(h.used); i++ {
		if !h.used[i] {
			return uint8(i), true
		}
	}
	return 0, false
}

// Admit runs the capacity-aware join handshake on an authenticated session and,
// on success, adds the client to the roster and returns its player id. On refusal
// (build/seed mismatch or full session) it returns an error; the caller closes
// the session. Call serially from the accept loop.
func (h *Host) Admit(s *Session, addr string) (uint8, error) {
	h.mu.Lock()
	room := h.playerCountLocked() < h.capacity
	h.mu.Unlock()

	if err := s.HostAdmit(h.buildHash, h.seed, room); err != nil {
		h.mu.Lock()
		h.logf("REFUSED %s: %v (roster stays %v)", addr, err, h.rosterLocked())
		h.mu.Unlock()
		return 0, err
	}

	h.mu.Lock()
	defer h.mu.Unlock()
	slot, ok := h.freeSlotLocked() // re-check under lock (serial accept → always ok here)
	if !ok {
		h.logf("REFUSED %s: lost race for last slot", addr)
		return 0, fmt.Errorf("net: host: session full at commit")
	}
	h.used[slot] = true
	h.clients[slot] = &clientConn{player: slot, sess: s, addr: addr}
	h.logf("JOIN player %d from %s (roster now %v)", slot, addr, h.rosterLocked())
	return slot, nil
}

// Remove drops a client (on disconnect), freeing its slot. The loop continues for
// the rest. Returns whether the player was present.
func (h *Host) Remove(player uint8) bool {
	h.mu.Lock()
	defer h.mu.Unlock()
	c, ok := h.clients[player]
	if !ok {
		return false
	}
	_ = c.sess.Close()
	delete(h.clients, player)
	if int(player) < len(h.used) {
		h.used[player] = false
	}
	h.logf("DEPART player %d (%s) (roster now %v)", player, c.addr, h.rosterLocked())
	return true
}

func (h *Host) rosterLocked() []uint8 {
	out := []uint8{HostPlayer}
	for p := range h.clients {
		out = append(out, p)
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}

// Roster returns the current player ids (host first), sorted.
func (h *Host) Roster() []uint8 {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.rosterLocked()
}

// PlayerCount is the current total players (host + remotes).
func (h *Host) PlayerCount() int {
	h.mu.Lock()
	defer h.mu.Unlock()
	return h.playerCountLocked()
}

// Events returns a copy of the host event log (joins, departures, refusals) — the
// FSV source of truth.
func (h *Host) Events() []string {
	h.mu.Lock()
	defer h.mu.Unlock()
	return append([]string(nil), h.events...)
}

// CollectRound runs one aggregation round for turn: it takes the host player's own
// records (hostRecords, already encoded command records — empty slice is a valid
// heartbeat), reads one turn frame from each connected client, builds the
// aggregate over the SURVIVING roster, and broadcasts the identical payload to all
// surviving clients. A client whose read fails is removed (the round still
// completes for the rest). It returns the broadcast payload (which the host's own
// LockstepGate then Delivers) and the roster the aggregate was built over.
func (h *Host) CollectRound(turn uint64, hostRecords [][]byte) (payload []byte, roster []uint8, err error) {
	// Snapshot the current clients.
	h.mu.Lock()
	conns := make([]*clientConn, 0, len(h.clients))
	for _, c := range h.clients {
		conns = append(conns, c)
	}
	h.mu.Unlock()

	// Read each client's turn; collect departures rather than aborting the round.
	type sub struct {
		player  uint8
		records [][]byte
	}
	subs := []sub{{player: HostPlayer, records: hostRecords}}
	var departed []uint8
	for _, c := range conns {
		turnBytes, rerr := c.sess.RecvTurn()
		if rerr != nil {
			departed = append(departed, c.player)
			continue
		}
		recs, derr := DecodeTurn(turnBytes)
		if derr != nil {
			departed = append(departed, c.player)
			continue
		}
		subs = append(subs, sub{player: c.player, records: recs})
	}
	for _, p := range departed {
		h.Remove(p)
	}

	// Build the aggregate over exactly the surviving players.
	players := make([]uint8, 0, len(subs))
	for _, s := range subs {
		players = append(players, s.player)
	}
	tb, err := NewTurnBuffer(h.turnLen, players)
	if err != nil {
		return nil, nil, err
	}
	for _, s := range subs {
		if err := tb.Submit(turn, s.player, s.records); err != nil {
			return nil, nil, fmt.Errorf("net: host round %d: submit player %d: %w", turn, s.player, err)
		}
	}
	payload, err = tb.Aggregate(turn)
	if err != nil {
		return nil, nil, fmt.Errorf("net: host round %d: aggregate: %w", turn, err)
	}

	// Broadcast to the survivors.
	h.mu.Lock()
	sessions := make([]*Session, 0, len(h.clients))
	for _, c := range h.clients {
		sessions = append(sessions, c.sess)
	}
	roster = h.rosterLocked()
	h.mu.Unlock()
	if berr := Broadcast(payload, sessions); berr != nil {
		return payload, roster, fmt.Errorf("net: host round %d: broadcast: %w", turn, berr)
	}
	h.logf("ROUND %d: aggregated %d player(s) %v, broadcast %d B to %d client(s)", turn, len(subs), players, len(payload), len(sessions))
	return payload, roster, nil
}

// Close tears down all client sessions.
func (h *Host) Close() error {
	h.mu.Lock()
	defer h.mu.Unlock()
	for p, c := range h.clients {
		_ = c.sess.Close()
		delete(h.clients, p)
		h.used[p] = false
	}
	return nil
}
