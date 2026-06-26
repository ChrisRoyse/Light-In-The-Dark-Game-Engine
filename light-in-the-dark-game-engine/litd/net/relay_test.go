package net

import (
	"reflect"
	"testing"
)

// #64 FSV: the relay's multi-session routing registry. SoT = the session map and
// the rosters/targets it returns. Edges the issue names: (1) join a nonexistent
// session → refused; (2) a peer vanishes mid-session → dropped, survivors'
// roster returned; plus session isolation (a turn never cross-routes to another
// game) and graceful drain.

func mustRoster(t *testing.T, r *RelayRegistry, id string) []uint8 {
	t.Helper()
	rs, ok := r.Roster(id)
	if !ok {
		t.Fatalf("Roster(%q) not found", id)
	}
	return rs
}

func eqU8(a, b []uint8) bool {
	if len(a) == 0 && len(b) == 0 {
		return true
	}
	return reflect.DeepEqual(a, b)
}

func TestRelayRoutingFSV(t *testing.T) {
	r, err := NewRelayRegistry(4)
	if err != nil {
		t.Fatalf("NewRelayRegistry: %v", err)
	}

	// Two independent games on the one relay.
	if err := r.Open("game-A"); err != nil {
		t.Fatalf("Open A: %v", err)
	}
	if err := r.Open("game-B"); err != nil {
		t.Fatalf("Open B: %v", err)
	}
	// Duplicate open refused.
	if err := r.Open("game-A"); err == nil {
		t.Fatal("Open of a duplicate session id must be refused")
	}

	for _, p := range []uint8{0, 1, 2} {
		if err := r.Join("game-A", p); err != nil {
			t.Fatalf("Join A %d: %v", p, err)
		}
	}
	for _, p := range []uint8{5, 6} {
		if err := r.Join("game-B", p); err != nil {
			t.Fatalf("Join B %d: %v", p, err)
		}
	}
	t.Logf("BEFORE: sessions=%v rosterA=%v rosterB=%v", r.SessionIDs(), mustRoster(t, r, "game-A"), mustRoster(t, r, "game-B"))

	// Forwarding: a turn from peer 1 in game-A goes to the OTHER game-A peers
	// only — never to game-B (session isolation).
	tgt, ok := r.Targets("game-A", 1)
	if !ok || !eqU8(tgt, []uint8{0, 2}) {
		t.Fatalf("Targets(game-A, from=1) = %v (ok=%v), want [0 2] — sender excluded, no cross-route", tgt, ok)
	}
	if tgtB, _ := r.Targets("game-B", 5); !eqU8(tgtB, []uint8{6}) {
		t.Fatalf("Targets(game-B, from=5) = %v, want [6]", tgtB)
	}
	t.Logf("FSV isolation: turn from A:1 → %v (only A peers); turn from B:5 → %v", tgt, []uint8{6})

	// Edge 1: join a nonexistent session → refused, session map unchanged.
	before := r.SessionIDs()
	if err := r.Join("game-Z", 9); err == nil {
		t.Fatal("Join to a nonexistent session must be refused (relay never auto-creates)")
	}
	if !reflect.DeepEqual(before, r.SessionIDs()) {
		t.Fatalf("a refused join changed the session map: %v -> %v", before, r.SessionIDs())
	}
	t.Logf("FSV edge1: Join(game-Z) refused; sessions still %v", r.SessionIDs())

	// Edge 2: a peer vanishes → dropped, survivors' roster returned for the
	// relay to broadcast.
	survivors, err := r.Drop("game-A", 1)
	if err != nil {
		t.Fatalf("Drop A:1: %v", err)
	}
	if !eqU8(survivors, []uint8{0, 2}) {
		t.Fatalf("after drop A:1 roster = %v, want [0 2]", survivors)
	}
	// Dropping a peer not present → refused.
	if _, err := r.Drop("game-A", 1); err == nil {
		t.Fatal("dropping an absent peer must be refused")
	}
	t.Logf("FSV edge2: A:1 vanished → survivors roster %v broadcast", survivors)

	// Edge 3: graceful drain — Close returns the final roster and removes it.
	final, ok := r.Close("game-B")
	if !ok || !eqU8(final, []uint8{5, 6}) {
		t.Fatalf("Close(game-B) = %v (ok=%v), want [5 6]", final, ok)
	}
	if _, exists := r.Roster("game-B"); exists {
		t.Fatal("session must be gone after Close")
	}
	if _, ok := r.Targets("game-B", 5); ok {
		t.Fatal("Targets on a closed session must report not-found (no forwarding to a drained game)")
	}
	t.Logf("FSV drain: Close(game-B) → final roster %v, session removed; sessions now %v", final, r.SessionIDs())
}

func TestRelayFullSessionRefusedFSV(t *testing.T) {
	r, _ := NewRelayRegistry(2)
	if err := r.Open("g"); err != nil {
		t.Fatal(err)
	}
	if err := r.Join("g", 0); err != nil {
		t.Fatal(err)
	}
	if err := r.Join("g", 1); err != nil {
		t.Fatal(err)
	}
	err := r.Join("g", 2) // third into a cap-2 session
	t.Logf("FSV full: third join into cap-2 session → %v", err)
	if err == nil {
		t.Fatal("join into a full session must be refused")
	}
	// Duplicate peer refused too.
	if err := r.Join("g", 0); err == nil {
		t.Fatal("duplicate peer join must be refused")
	}
}
