package luabind

// #201 stats + edge 1 (exact match duration; clean second match). The existing
// match-flow FSV asserts duration > 0 and runs the defeat path on a fresh game,
// but never pins the duration to a KNOWN value nor asserts that a second match in
// the same process starts CLEAN (no leaked result/duration/startedat) — the
// "basic stats" deliverable and edge 1 ("second match → clean reset"). This locks
// both. The cadence is exact: Game_Every(0.05) fires once per sim tick and the
// world does `t = t + 1` at the top, so flow t == cumulative ticks advanced;
// SETUP_TICKS(5) + COUNTDOWN_TICKS(20) ⇒ PLAY at t=25 (startedAt=25), and
// duration = (the tick the result is detected) − startedAt.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

func TestMatchFlowDurationAndResetFSV(t *testing.T) {
	// --- Match A: exact-duration defeat run. ---
	g := matchFlowGame(t, 5)
	if s := flowInt(g, "state"); s != flowSetup {
		t.Fatalf("match A initial state=%d, want SETUP(%d)", s, flowSetup)
	}
	g.Advance(24) // t=24: still COUNTDOWN (24-5=19 < 20)
	if s := flowInt(g, "state"); s == flowPlay {
		t.Fatalf("match A reached PLAY a tick early (t=24) — countdown boundary off by one")
	}
	g.Advance(1) // t=25: COUNTDOWN(20) satisfied → PLAY, startedAt=25
	if s := flowInt(g, "state"); s != flowPlay {
		t.Fatalf("match A state=%d at t=25, want PLAY(%d)", s, flowPlay)
	}
	if got := flowInt(g, "startedat"); got != 25 {
		t.Fatalf("match A startedat=%d, want 25 (SETUP 5 + COUNTDOWN 20)", got)
	}
	// 40 ticks of PLAY with no result; a Defeat order then latches into
	// Player_Result on the 2nd tick after it is issued, so the detecting fire is
	// at t=67 and duration = 67 - 25 = 42 — an EXACT value, not merely > 0.
	g.Advance(40) // t=65
	if s := flowInt(g, "state"); s != flowPlay {
		t.Fatalf("match A left PLAY before any result (state=%d)", s)
	}
	g.Defeat(g.Player(1), "out-held")
	g.Advance(2) // t=67: order latches (2-tick lag) → detects Lost, duration = 67-25
	if s := flowInt(g, "state"); s != flowTerminal {
		t.Fatalf("match A state=%d after defeat, want TERMINAL(%d)", s, flowTerminal)
	}
	if d := flowInt(g, "duration"); d != 42 {
		t.Fatalf("match A duration=%d, want exactly 42 (40 PLAY ticks + 2 detect-lag ticks)", d)
	}
	if r := flowInt(g, "result"); r != int(api.ResultLost) {
		t.Fatalf("match A result=%d, want Lost(%d)", r, int(api.ResultLost))
	}
	t.Logf("FSV #201 duration: PLAY@t=25, defeat after 40 PLAY ticks → TERMINAL duration=42 (exact), result=Lost")

	// --- Match B (#201 edge 1): a fresh match in the SAME process must start
	// CLEAN at SETUP with no leaked result/duration/startedat, and run its OWN
	// independent flow to a DIFFERENT-length terminal. ---
	g2 := matchFlowGame(t, 9)
	if s := flowInt(g2, "state"); s != flowSetup {
		t.Fatalf("match B did not start clean: state=%d, want SETUP(%d)", s, flowSetup)
	}
	if r, d, sa := flowInt(g2, "result"), flowInt(g2, "duration"), flowInt(g2, "startedat"); r != 0 || d != 0 || sa != 0 {
		t.Fatalf("match B leaked prior-match state: result=%d duration=%d startedat=%d, want 0/0/0", r, d, sa)
	}
	g2.Advance(25) // → PLAY, startedAt=25
	if s := flowInt(g2, "state"); s != flowPlay {
		t.Fatalf("match B state=%d at t=25, want PLAY(%d)", s, flowPlay)
	}
	g2.Advance(10) // t=35
	g2.Victory(g2.Player(1))
	g2.Advance(2) // t=37 detects Won (2-tick lag) → duration = 37-25 = 12
	if s := flowInt(g2, "state"); s != flowTerminal {
		t.Fatalf("match B state=%d after victory, want TERMINAL(%d)", s, flowTerminal)
	}
	if d := flowInt(g2, "duration"); d != 12 {
		t.Fatalf("match B duration=%d, want 12 (10 PLAY ticks + 2) — and independent of match A duration 42", d)
	}
	if r := flowInt(g2, "result"); r != int(api.ResultWon) {
		t.Fatalf("match B result=%d, want Won(%d)", r, int(api.ResultWon))
	}
	t.Logf("FSV #201 second-match reset: fresh match started clean at SETUP (no leaked stats); ran independently to duration=12/Won vs match A 42/Lost")
}
