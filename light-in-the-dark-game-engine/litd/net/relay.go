package net

// relay.go is the relay's multi-session routing policy (#64). A relay is a star
// center that forwards lockstep turns among the peers of MANY concurrent games —
// unlike host.go, which serves a single game. This registry is the piece host.go
// does not provide: sessions keyed by id, each holding a peer set, with
// join-existing-only, drop-with-roster, and per-session forwarding targets.
//
// It is pure routing: it never runs the sim and never inspects a payload beyond
// the session id it belongs to (the issue's "pure turn aggregation/broadcast
// forwarding" constraint). It is sim-free like the rest of litd/net. The QUIC
// listener, PSK session-token auth, and graceful-drain wiring live in the
// cmd/litd-relay binary and stay gated on the cert/auth dependency; this is the
// in-memory session model they drive, testable headlessly.

import (
	"fmt"
	"sort"
)

// RelayRegistry tracks the relay's live sessions. Not safe for concurrent use;
// the relay drives it from its single accept/forward loop.
type RelayRegistry struct {
	maxPeers int
	sessions map[string]map[uint8]bool
}

// NewRelayRegistry builds a registry capping each session at maxPeers (2–8 to
// match the lobby bounds). maxPeers <= 0 is an error.
func NewRelayRegistry(maxPeers int) (*RelayRegistry, error) {
	if maxPeers <= 0 {
		return nil, fmt.Errorf("net: relay: maxPeers must be > 0, got %d", maxPeers)
	}
	return &RelayRegistry{maxPeers: maxPeers, sessions: make(map[string]map[uint8]bool)}, nil
}

// Open registers a new session id. It refuses a duplicate — a relay never
// silently merges two games onto one id.
func (r *RelayRegistry) Open(id string) error {
	if id == "" {
		return fmt.Errorf("net: relay: empty session id")
	}
	if _, ok := r.sessions[id]; ok {
		return fmt.Errorf("net: relay: session %q already open", id)
	}
	r.sessions[id] = make(map[uint8]bool)
	return nil
}

// Join adds peer to an EXISTING session. It refuses an unknown session id (the
// relay never auto-creates on join — edge 1), a duplicate peer, or a full session.
func (r *RelayRegistry) Join(id string, peer uint8) error {
	s, ok := r.sessions[id]
	if !ok {
		return fmt.Errorf("net: relay: no such session %q", id)
	}
	if s[peer] {
		return fmt.Errorf("net: relay: peer %d already in session %q", peer, id)
	}
	if len(s) >= r.maxPeers {
		return fmt.Errorf("net: relay: session %q full (%d peers)", id, r.maxPeers)
	}
	s[peer] = true
	return nil
}

// Drop removes a peer from a session and returns the remaining roster (sorted) —
// what the relay broadcasts to the survivors when a peer vanishes (edge 2). It
// refuses an unknown session or a peer that is not present.
func (r *RelayRegistry) Drop(id string, peer uint8) ([]uint8, error) {
	s, ok := r.sessions[id]
	if !ok {
		return nil, fmt.Errorf("net: relay: no such session %q", id)
	}
	if !s[peer] {
		return nil, fmt.Errorf("net: relay: peer %d not in session %q", peer, id)
	}
	delete(s, peer)
	return roster(s), nil
}

// Roster returns the sorted peer ids of a session, and whether it exists.
func (r *RelayRegistry) Roster(id string) ([]uint8, bool) {
	s, ok := r.sessions[id]
	if !ok {
		return nil, false
	}
	return roster(s), true
}

// Targets returns the peers a turn from `from` in session id forwards to: the
// roster minus the sender (a relay echoes a peer's turn to the others, never back
// to itself). The second return is false for an unknown session — the relay
// refuses to forward a turn for a session it does not host, never cross-routing
// to another game's peers.
func (r *RelayRegistry) Targets(id string, from uint8) ([]uint8, bool) {
	s, ok := r.sessions[id]
	if !ok {
		return nil, false
	}
	out := make([]uint8, 0, len(s))
	for p := range s {
		if p != from {
			out = append(out, p)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out, true
}

// Close removes a session (graceful drain / game over), returning its final
// roster so the relay can close those peers cleanly.
func (r *RelayRegistry) Close(id string) ([]uint8, bool) {
	s, ok := r.sessions[id]
	if !ok {
		return nil, false
	}
	final := roster(s)
	delete(r.sessions, id)
	return final, true
}

// SessionIDs returns all live session ids, sorted — for the relay log and the
// SIGTERM drain that closes every session.
func (r *RelayRegistry) SessionIDs() []string {
	ids := make([]string, 0, len(r.sessions))
	for id := range r.sessions {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return ids
}

func roster(s map[uint8]bool) []uint8 {
	out := make([]uint8, 0, len(s))
	for p := range s {
		out = append(out, p)
	}
	sort.Slice(out, func(i, j int) bool { return out[i] < out[j] })
	return out
}
