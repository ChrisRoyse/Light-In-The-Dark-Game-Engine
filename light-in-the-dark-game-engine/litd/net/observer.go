package net

import "sort"

// Observer support (#84). An observer is a spectator client that receives the
// SAME aggregated turn stream as players and runs the full sim, but submits no
// turns and occupies no player slot — a zero-delay replay viewer fed by the live
// command stream (D-2026-06-11-16). The defining invariant: observer count and
// observer lag must never affect lockstep timing — the host never waits on an
// observer.
//
// This file is the host-side POLICY that guarantees that invariant structurally:
// observers live in an ObserverSet that is DELIBERATELY SEPARATE from the player
// roster the lockstep gate (RoundGate) accumulates and waits on. RoundGate has no
// reference to observers, so they can never enter its waiting/roster set; an
// observer is fed purely by consuming the aggregated GateStep.Payload after the
// round has already closed on the players alone. Each observer carries its own
// delivery cursor over the turn stream, so a slow observer falls behind itself
// with no back-pressure onto the host.

// ObserverID identifies one live observer within a match.
type ObserverID uint32

// Observer is one spectator. JoinTurn is the aggregated turn it began watching
// from (zero-delay: the host's current turn at join, since no replay-from-zero
// or state-snapshot catch-up has shipped yet — those are #83). cursor is the
// next turn it still needs delivered.
type Observer struct {
	ID       ObserverID
	Name     string
	JoinTurn uint64
	cursor   uint64
}

// Cursor is the next turn this observer still needs (read-only snapshot copy).
func (o Observer) Cursor() uint64 { return o.cursor }

// ObserverSet is the host-side registry of live observers, separate from the
// lockstep player roster. Not safe for concurrent use (the host loop owns it,
// like the rest of litd/net's policy state).
type ObserverSet struct {
	obs    map[ObserverID]*Observer
	order  []ObserverID // join order — deterministic iteration (no map ranging)
	nextID ObserverID
}

// NewObserverSet returns an empty registry.
func NewObserverSet() *ObserverSet {
	return &ObserverSet{obs: make(map[ObserverID]*Observer)}
}

// Join admits an observer watching from currentTurn (the host's latest
// aggregated turn) onward and returns its new id. Ids are monotonic and never
// reused within a set, so a rejoining spectator is a distinct observer.
func (s *ObserverSet) Join(name string, currentTurn uint64) ObserverID {
	id := s.nextID
	s.nextID++
	s.obs[id] = &Observer{ID: id, Name: name, JoinTurn: currentTurn, cursor: currentTurn}
	s.order = append(s.order, id)
	return id
}

// Leave removes an observer; returns false for an unknown id. A departure never
// touches the player roster, so it cannot stall the match.
func (s *ObserverSet) Leave(id ObserverID) bool {
	if _, ok := s.obs[id]; !ok {
		return false
	}
	delete(s.obs, id)
	for i, x := range s.order {
		if x == id {
			s.order = append(s.order[:i], s.order[i+1:]...)
			break
		}
	}
	return true
}

// Count is the number of live observers.
func (s *ObserverSet) Count() int { return len(s.obs) }

// Has reports whether id is a live observer.
func (s *ObserverSet) Has(id ObserverID) bool { _, ok := s.obs[id]; return ok }

// Observers returns a snapshot of every observer in join order (stable,
// map-iteration-free).
func (s *ObserverSet) Observers() []Observer {
	out := make([]Observer, 0, len(s.order))
	for _, id := range s.order {
		out = append(out, *s.obs[id])
	}
	return out
}

// Pending returns the contiguous turns observer id still needs given the host's
// latest aggregated turn (inclusive): [cursor, cursor+1, ..., latestTurn].
// Empty when the observer is caught up (cursor > latestTurn) or unknown. Pure —
// reading it never advances anything and never signals the host.
func (s *ObserverSet) Pending(id ObserverID, latestTurn uint64) []uint64 {
	o, ok := s.obs[id]
	if !ok || o.cursor > latestTurn {
		return nil
	}
	out := make([]uint64, 0, latestTurn-o.cursor+1)
	for t := o.cursor; t <= latestTurn; t++ {
		out = append(out, t)
	}
	return out
}

// Deliver records that every turn up to and including upTo has been sent to
// observer id, advancing its cursor to upTo+1. A cursor only moves forward, so
// re-delivering an already-seen turn is a no-op; gaps are impossible because the
// cursor is contiguous. Returns the new cursor (0 for an unknown id). The host
// never blocks on this — it fans out at its own pace and a slow observer simply
// has a low cursor.
func (s *ObserverSet) Deliver(id ObserverID, upTo uint64) uint64 {
	o, ok := s.obs[id]
	if !ok {
		return 0
	}
	if upTo+1 > o.cursor {
		o.cursor = upTo + 1
	}
	return o.cursor
}

// MaxLag returns the largest number of turns any observer is behind the host's
// latest aggregated turn (0 when all caught up or no observers). A diagnostic
// for the host UI; it is NEVER an input to lockstep timing — the host advances
// regardless of how large this grows.
func (s *ObserverSet) MaxLag(latestTurn uint64) uint64 {
	var max uint64
	ids := make([]ObserverID, len(s.order))
	copy(ids, s.order)
	sort.Slice(ids, func(i, j int) bool { return ids[i] < ids[j] })
	for _, id := range ids {
		o := s.obs[id]
		if o.cursor <= latestTurn {
			if lag := latestTurn - o.cursor + 1; lag > max {
				max = lag
			}
		}
	}
	return max
}
