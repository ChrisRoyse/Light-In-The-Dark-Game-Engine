package luabind

// #413 FSV: a Lua coroutine can park on a game EVENT (WaitForEvent), not only a
// timer (PolledWait), and that wait round-trips through a mid-wait save/load —
// the edge #270 could not exercise. SoT = the unit life the coroutine writes
// (read back through the Go api) + the sim StateHash (R-FSV-2): a coroutine that
// parks on EventUnitDeath, is saved mid-wait, restored into a FRESH runtime, and
// then sees the death fire POST-RESTORE must resume and leave the world
// bit-identical to the unbroken run. No mocks; real save/load, real sim death.
//
// EventUnitDeath == 1 (api.EventKind iota+1); WaitForEvent takes that int.

import (
	"bytes"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

const evUnitDeath = 1 // api.EventUnitDeath

// markerWaitsOnDeath: the coroutine creates a marker unit at (100,0), sets life
// 30, parks on a unit-death event, then on resume sets life 77. The marker is
// the coroutine's own stack local — it must survive save/load like coScript's.
const markerWaitsOnDeath = `Run(function()
	local m = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=100, y=0}, 0)
	Unit_SetLife(m, 30)
	WaitForEvent(1)            -- EventUnitDeath
	Unit_SetLife(m, 77)
end)`

// unitNearX returns the single live unit whose world X is within 1 of x (units
// are created at distinct X so each is findable across a fresh-game restore,
// where Go handle identity does not survive but sim positions do).
func unitNearX(t *testing.T, g *api.Game, x float64) api.Unit {
	t.Helper()
	var found api.Unit
	n := 0
	for _, u := range g.AllUnits(nil) {
		if px := u.Position().X; px > x-1 && px < x+1 {
			found, n = u, n+1
		}
	}
	if n != 1 {
		t.Fatalf("want exactly 1 unit near x=%v, got %d", x, n)
	}
	return found
}

func lifeNearX(t *testing.T, g *api.Game, x float64) float64 {
	t.Helper()
	return unitNearX(t, g, x).Life()
}

// spawnVictim creates a unit at (vx, 0) whose death will be the event trigger.
func spawnVictim(g *api.Game, vx float64) api.Unit {
	return g.CreateUnit(g.Player(0), g.UnitType("hfoo"), api.Vec2{X: vx, Y: 0}, api.Deg(0))
}

// TestWaitForEventResumeFSV — happy path + no-spurious-wake. The coroutine parks
// on EventUnitDeath; it must NOT wake on ticks where nothing dies, and MUST wake
// the tick after a death fires.
func TestWaitForEventResumeFSV(t *testing.T) {
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, markerWaitsOnDeath)
	victim := spawnVictim(g, 500)

	if got := lifeNearX(t, g, 100); got != 30 {
		t.Fatalf("pre-wait marker life=%v, want 30", got)
	}
	if n := PendingScriptWaits(L); n != 1 {
		t.Fatalf("want 1 coroutine parked on the event, got %d", n)
	}

	// No-spurious-wake: 20 ticks pass with no death — the coroutine stays parked.
	g.Advance(20)
	if got := lifeNearX(t, g, 100); got != 30 {
		t.Fatalf("marker life=%v after 20 deathless ticks — coroutine woke spuriously (want 30)", got)
	}
	if n := PendingScriptWaits(L); n != 1 {
		t.Fatalf("want still 1 parked after deathless ticks, got %d", n)
	}
	t.Logf("FSV #413 BEFORE: marker life=30, parked=1 (survived 20 deathless ticks)")

	// Trigger: kill the victim → EventUnitDeath fires → wake posted → resume.
	victim.Kill()
	g.Advance(5)
	if got := lifeNearX(t, g, 100); got != 77 {
		t.Fatalf("marker life=%v after death — coroutine did not resume (want 77)", got)
	}
	if n := PendingScriptWaits(L); n != 0 {
		t.Fatalf("want 0 parked after resume+finish, got %d", n)
	}
	t.Logf("FSV #413 AFTER: death fired → marker life=77, parked=0")
}

// runToDeathHash sets up the chunk + a victim at vx, advances pre ticks, kills
// the victim, advances post ticks, and returns the final StateHash and marker
// life. Shared by the unbroken reference and (structurally) the restored run.
func runToDeathHash(t *testing.T, pre, post int) (uint64, float64) {
	t.Helper()
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, markerWaitsOnDeath)
	victim := spawnVictim(g, 500)
	g.Advance(pre)
	victim.Kill()
	g.Advance(post)
	return g.StateHash(), lifeNearX(t, g, 100)
}

// TestWaitForEventSaveRestoreFSV — the keystone. Save while the coroutine is
// parked on the event; restore into a fresh runtime; the death fires POST-RESTORE
// (caught by the dispatcher re-subscribed in LoadScripts) and the coroutine
// resumes. Final StateHash must equal the unbroken run's.
func TestWaitForEventSaveRestoreFSV(t *testing.T) {
	const fp = uint64(0xE7E27411) // world fingerprint tag

	// Reference: unbroken. Kill at tick 5 (pre=5), then 5 more ticks.
	refHash, refLife := runToDeathHash(t, 5, 5)
	if refLife != 77 {
		t.Fatalf("reference marker life=%v, want 77", refLife)
	}
	// Determinism check: the reference is reproducible bit-for-bit.
	if h2, _ := runToDeathHash(t, 5, 5); h2 != refHash {
		t.Fatalf("reference not reproducible: %#x != %#x", h2, refHash)
	}

	// Run B: save at tick 1, mid-wait (before the kill).
	gA, LA, regA := newScriptGame(t)
	defer LA.Close()
	defer regA.Close()
	runRegisteredChunk(t, LA, regA, markerWaitsOnDeath)
	spawnVictim(gA, 500)
	gA.Advance(1)
	if got := lifeNearX(t, gA, 100); got != 30 {
		t.Fatalf("pre-save marker life=%v, want 30 (parked)", got)
	}
	if n := PendingScriptWaits(LA); n != 1 {
		t.Fatalf("want 1 parked at save, got %d", n)
	}
	var simBlob, scriptBlob bytes.Buffer
	if err := gA.SaveState(&simBlob, fp); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	if err := SaveScripts(LA, regA, &scriptBlob); err != nil {
		t.Fatalf("SaveScripts: %v", err)
	}
	t.Logf("FSV #413 save@tick1: sim=%dB script=%dB, 1 coroutine parked on EventUnitDeath, life=30",
		simBlob.Len(), scriptBlob.Len())

	// Restore into a FRESH game + runtime. Re-register the chunk (do NOT run it —
	// LoadScripts restores the real coroutine), then LoadState + LoadScripts.
	gB, LB, regB := newScriptGame(t)
	defer LB.Close()
	defer regB.Close()
	if _, err := regB.Register("world", markerWaitsOnDeath); err != nil {
		t.Fatalf("re-register chunk: %v", err)
	}
	if err := gB.LoadState(&simBlob, fp); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if err := LoadScripts(LB, regB, &scriptBlob); err != nil {
		t.Fatalf("LoadScripts: %v", err)
	}
	if got := lifeNearX(t, gB, 100); got != 30 {
		t.Fatalf("post-restore marker life=%v, want 30 (restored parked, not resumed)", got)
	}
	if n := PendingScriptWaits(LB); n != 1 {
		t.Fatalf("post-restore want 1 parked coroutine, got %d", n)
	}
	t.Logf("FSV #413 post-restore: marker life=30, parked=1 (event wait round-tripped)")

	// Advance to the same kill tick (saved at 1 → 4 more = tick 5), kill the
	// restored victim, advance the remaining 5 — the death fires POST-RESTORE.
	gB.Advance(4)
	victimB := unitNearX(t, gB, 500)
	victimB.Kill()
	gB.Advance(5)
	if got := lifeNearX(t, gB, 100); got != 77 {
		t.Fatalf("restored marker life=%v, want 77 (post-restore death did not resume the coroutine)", got)
	}
	restoredHash := gB.StateHash()
	if restoredHash != refHash {
		t.Fatalf("HASH MISMATCH: restored %#x != unbroken %#x — event-wait save/load not bit-identical",
			restoredHash, refHash)
	}
	t.Logf("FSV #413 keystone: save mid-event-wait → fresh restore → death fires post-restore → resume; "+
		"life=77, StateHash %#x == unbroken %#x", restoredHash, refHash)
}

// TestWaitForEventUnknownKindFSV — fail-closed: WaitForEvent on a kind with no
// sim mapping must NOT park the coroutine forever; it surfaces a loud error and
// retires the coroutine (no silent fallback that hides the bad kind).
func TestWaitForEventUnknownKindFSV(t *testing.T) {
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	var gotErr string
	OnScriptError(L, func(err error) { gotErr = err.Error() })
	// 9999 is a valid uint16 but maps to no sim event kind.
	runRegisteredChunk(t, L, reg, `Run(function() WaitForEvent(9999) end)`)
	if n := PendingScriptWaits(L); n != 0 {
		t.Fatalf("unknown-kind coroutine left parked (%d) — must fail closed, not wait forever", n)
	}
	if !strings.Contains(gotErr, "unknown event kind") {
		t.Fatalf("want a loud 'unknown event kind' error, got %q", gotErr)
	}
	// And it never spuriously wakes anything on a later real event.
	v := spawnVictim(g, 500)
	v.Kill()
	g.Advance(5)
	if n := PendingScriptWaits(L); n != 0 {
		t.Fatalf("parked=%d after a real event — retired coroutine must stay gone", n)
	}
	t.Logf("FSV #413 fail-closed: WaitForEvent(9999) → retired + error %q (not parked forever)", gotErr)
}

// TestWaitForEventMultiWaiterFSV — one event wakes ALL coroutines parked on it,
// in deterministic order. Three coroutines park on EventUnitDeath; a single death
// must resume all three.
func TestWaitForEventMultiWaiterFSV(t *testing.T) {
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	const script = `for i=1,3 do
		local x = i*100
		Run(function()
			local m = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=x, y=0}, 0)
			Unit_SetLife(m, 30)
			WaitForEvent(1)
			Unit_SetLife(m, 77)
		end)
	end`
	runRegisteredChunk(t, L, reg, script)
	victim := spawnVictim(g, 500)

	if n := PendingScriptWaits(L); n != 3 {
		t.Fatalf("want 3 coroutines parked on the event, got %d", n)
	}
	for _, x := range []float64{100, 200, 300} {
		if got := lifeNearX(t, g, x); got != 30 {
			t.Fatalf("marker@%v pre-death life=%v, want 30", x, got)
		}
	}
	t.Logf("FSV #413 multi BEFORE: 3 markers life=30, parked=3")

	victim.Kill()
	g.Advance(5)
	for _, x := range []float64{100, 200, 300} {
		if got := lifeNearX(t, g, x); got != 77 {
			t.Fatalf("marker@%v post-death life=%v, want 77 — one death did not wake all waiters", x, got)
		}
	}
	if n := PendingScriptWaits(L); n != 0 {
		t.Fatalf("want 0 parked after all three resume, got %d", n)
	}
	t.Logf("FSV #413 multi AFTER: one death woke all 3 → markers life=77, parked=0")
}

// TestWaitForEventRewaitFSV — the filter pattern: a coroutine re-parks on the
// same kind after waking, and a single event must NOT double-wake it. The
// coroutine completes only after the SECOND distinct death.
func TestWaitForEventRewaitFSV(t *testing.T) {
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	// Wakes set life to 40+count; the body needs TWO deaths to reach count==2 and
	// finish (life 77). If one event double-woke, count would jump and finish early.
	const script = `Run(function()
		local m = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=100, y=0}, 0)
		Unit_SetLife(m, 30)
		local count = 0
		while count < 2 do
			WaitForEvent(1)
			count = count + 1
			Unit_SetLife(m, 40 + count)
		end
		Unit_SetLife(m, 77)
	end)`
	runRegisteredChunk(t, L, reg, script)
	v1 := spawnVictim(g, 500)
	v2 := spawnVictim(g, 600)

	if n := PendingScriptWaits(L); n != 1 {
		t.Fatalf("want 1 parked, got %d", n)
	}

	// First death: one wake → count 1 → life 41 → re-parked (not finished).
	v1.Kill()
	g.Advance(5)
	if got := lifeNearX(t, g, 100); got != 41 {
		t.Fatalf("after 1st death life=%v, want 41 (single wake, re-parked)", got)
	}
	if n := PendingScriptWaits(L); n != 1 {
		t.Fatalf("after 1st death want 1 re-parked (a single event must not double-wake), got %d", n)
	}
	t.Logf("FSV #413 rewait: 1st death → life=41, re-parked=1 (no double-wake)")

	// Second death: second wake → count 2 → loop exits → life 77.
	v2.Kill()
	g.Advance(5)
	if got := lifeNearX(t, g, 100); got != 77 {
		t.Fatalf("after 2nd death life=%v, want 77 (re-wait completed)", got)
	}
	if n := PendingScriptWaits(L); n != 0 {
		t.Fatalf("after 2nd death want 0 parked, got %d", n)
	}
	t.Logf("FSV #413 rewait: 2nd death → life=77, parked=0 (filter-by-rewait works)")
}
