package net

// #80 lobby state-model FSV. SoT = the Lobby's slot snapshot + Start outcome after
// each mutation. Covers the happy bootstrap (host + 3 join + ready + start) and the
// three edges the issue mandates: (1) join a full lobby -> refused with reason; (2)
// a client leaves -> slot frees; (3) Start with an unready slot -> blocked, no
// start. Pure, deterministic — no IO.

import (
	"strings"
	"testing"
)

func lobbyParams() StartParams { return StartParams{Seed: 0xABCDEF, TurnLen: 3, InputDelay: 2} }

func slotStates(l *Lobby) []SlotState {
	slots := l.Slots()
	out := make([]SlotState, len(slots))
	for i, s := range slots {
		out[i] = s.State
	}
	return out
}

func TestLobbyHappyBootstrapFSV(t *testing.T) {
	l, err := NewLobby(4, "Host-Aldric", lobbyParams())
	if err != nil {
		t.Fatal(err)
	}
	// BEFORE: host occupies slot 0, rest empty. Cannot start (only 1 player).
	t.Logf("FSV BEFORE: slots=%v occupied=%d canStart=%v", slotStates(l), l.OccupiedCount(), l.CanStart())
	if l.OccupiedCount() != 1 || l.CanStart() {
		t.Fatalf("fresh lobby: occupied=%d canStart=%v, want 1 / false", l.OccupiedCount(), l.CanStart())
	}

	// Three clients join the lowest free slots in order.
	for i, name := range []string{"Bryn", "Cael", "Dunn"} {
		slot, err := l.Join(name)
		if err != nil {
			t.Fatalf("join %s: %v", name, err)
		}
		if slot != uint8(i+1) {
			t.Fatalf("join %s got slot %d, want %d", name, slot, i+1)
		}
	}
	if l.OccupiedCount() != 4 {
		t.Fatalf("after 3 joins occupied=%d, want 4", l.OccupiedCount())
	}
	// All occupied but not ready => still cannot start.
	if l.CanStart() {
		t.Fatal("must not start with unready slots")
	}

	// Each client readies. Host never ready-toggles.
	for s := uint8(1); s <= 3; s++ {
		if err := l.SetReady(s, true); err != nil {
			t.Fatalf("ready slot %d: %v", s, err)
		}
	}
	t.Logf("FSV READY: slots=%v canStart=%v", slotStates(l), l.CanStart())
	if !l.CanStart() {
		t.Fatalf("all ready: canStart=false, want true (slots=%v)", slotStates(l))
	}

	// Start returns the exact configured params and latches started.
	got, err := l.Start()
	if err != nil {
		t.Fatalf("start: %v", err)
	}
	if got != lobbyParams() {
		t.Fatalf("start params=%+v, want %+v", got, lobbyParams())
	}
	if !l.Started() {
		t.Fatal("Started() false after Start")
	}
	// AFTER start: the lobby is read-only.
	if _, err := l.Join("Late"); err == nil {
		t.Fatal("join after start must be refused")
	}
	if err := l.SetReady(1, false); err == nil {
		t.Fatal("ready change after start must be refused")
	}
	if _, err := l.Start(); err == nil {
		t.Fatal("second Start must error (no re-fire)")
	}
	t.Logf("FSV AFTER start: params=%+v started=%v, mutations refused", got, l.Started())
}

// Edge 1: join attempt on a FULL lobby is refused with a reason string.
func TestLobbyJoinFullRefusedFSV(t *testing.T) {
	l, err := NewLobby(2, "Host", lobbyParams())
	if err != nil {
		t.Fatal(err)
	}
	if _, err := l.Join("P2"); err != nil { // fills slot 1 (capacity 2: host + 1)
		t.Fatalf("first join: %v", err)
	}
	t.Logf("FSV full BEFORE: occupied=%d/%d", l.OccupiedCount(), l.Capacity())

	slot, err := l.Join("P3")
	if err == nil {
		t.Fatalf("join on full lobby returned slot %d, want refusal", slot)
	}
	if !strings.Contains(err.Error(), "lobby full") {
		t.Fatalf("refusal reason=%q, want it to name 'lobby full'", err.Error())
	}
	// SoT: the refused join changed nothing.
	if l.OccupiedCount() != 2 {
		t.Fatalf("after refused join occupied=%d, want 2 (unchanged)", l.OccupiedCount())
	}
	t.Logf("FSV full AFTER: refused with %q, occupied still %d", err.Error(), l.OccupiedCount())
}

// Edge 2: a client leaves before start => its slot frees and reopens for joins.
func TestLobbyLeaveFreesSlotFSV(t *testing.T) {
	l, _ := NewLobby(4, "Host", lobbyParams())
	s2, _ := l.Join("Bryn") // slot 1
	_, _ = l.Join("Cael")   // slot 2
	_ = l.SetReady(s2, true)
	t.Logf("FSV leave BEFORE: slots=%v occupied=%d", slotStates(l), l.OccupiedCount())
	if l.OccupiedCount() != 3 {
		t.Fatalf("setup occupied=%d, want 3", l.OccupiedCount())
	}

	if err := l.Leave(s2); err != nil {
		t.Fatalf("leave slot %d: %v", s2, err)
	}
	// SoT: slot 1 is empty again, count drops, ready flag gone.
	if l.Slots()[s2].State != SlotEmpty || l.Slots()[s2].Name != "" {
		t.Fatalf("after leave slot %d = %+v, want empty/unnamed", s2, l.Slots()[s2])
	}
	if l.OccupiedCount() != 2 {
		t.Fatalf("after leave occupied=%d, want 2", l.OccupiedCount())
	}
	// The freed slot is the lowest empty, so the next join reuses it.
	reuse, err := l.Join("Echo")
	if err != nil || reuse != s2 {
		t.Fatalf("rejoin got slot %d err=%v, want freed slot %d", reuse, err, s2)
	}
	t.Logf("FSV leave AFTER: slot %d freed then reused by Echo; slots=%v", s2, slotStates(l))

	// The host slot can never leave.
	if err := l.Leave(HostPlayer); err == nil {
		t.Fatal("host slot leave must be refused")
	}
}

// Edge 3: Start clicked with an unready occupied slot is blocked; nothing starts.
func TestLobbyStartBlockedOnUnreadyFSV(t *testing.T) {
	l, _ := NewLobby(4, "Host", lobbyParams())
	_, _ = l.Join("Bryn")   // slot 1
	s2, _ := l.Join("Cael") // slot 2
	_ = l.SetReady(1, true)
	// slot 2 (Cael) stays UNready.
	t.Logf("FSV start-block BEFORE: slots=%v canStart=%v", slotStates(l), l.CanStart())
	if l.CanStart() {
		t.Fatal("canStart true with an unready slot")
	}

	params, err := l.Start()
	if err == nil {
		t.Fatalf("Start succeeded with unready slot, params=%+v", params)
	}
	if l.Started() {
		t.Fatal("Started() true after a blocked Start — no session must have started")
	}
	t.Logf("FSV start-block AFTER: Start refused (%q), started=%v", err.Error(), l.Started())

	// Ready the last slot, and now Start goes through — proves the block was the
	// unready slot, not something else.
	if err := l.SetReady(s2, true); err != nil {
		t.Fatal(err)
	}
	if _, err := l.Start(); err != nil {
		t.Fatalf("Start after readying all: %v", err)
	}
	t.Log("FSV start-unblock: after readying the last slot, Start succeeds")
}

// Construction guards (fail-closed): bad capacity / turn length / input delay / name.
func TestNewLobbyValidationFSV(t *testing.T) {
	cases := []struct {
		name    string
		cap     int
		host    string
		params  StartParams
		wantErr string
	}{
		{"cap too small", 1, "H", lobbyParams(), "capacity"},
		{"cap too large", 9, "H", lobbyParams(), "capacity"},
		{"empty host", 4, "  ", lobbyParams(), "host name"},
		{"turnlen low", 4, "H", StartParams{TurnLen: 1}, "turn length"},
		{"turnlen high", 4, "H", StartParams{TurnLen: 5}, "turn length"},
		{"neg input delay", 4, "H", StartParams{TurnLen: 3, InputDelay: -1}, "input delay"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			_, err := NewLobby(c.cap, c.host, c.params)
			if err == nil || !strings.Contains(err.Error(), c.wantErr) {
				t.Fatalf("err=%v, want one containing %q", err, c.wantErr)
			}
			t.Logf("FSV reject %s: %v", c.name, err)
		})
	}
}
