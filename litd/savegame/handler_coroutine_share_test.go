package savegame

// #446 FSV: a mutable Lua object (a closed upvalue cell or a table) shared between
// a scheduler COROUTINE and an OnEvent HANDLER must round-trip a mid-game
// save/load as ONE object. Before #446 the two serialized in separate save
// sections (handlers pre-sim, coroutines post-sim) so a shared object decoded as
// two independent copies — and DetectCrossThreadSharing fail-closed REFUSED the
// save outright rather than risk a silent divergence. As of #446 the handler
// closures join the one shared scheduler pool (SaveScripts), so the save both
// SUCCEEDS and round-trips.
//
// SoT = a SIM effect derived from the shared object across the save boundary (a
// marker unit's X position, read via the Go api) + Game.StateHash == the unbroken
// reference run. The X position is a discriminator: if the shared object came back
// as two copies, a post-restore mutation through one would be invisible to the
// other and the marker would land at the wrong X (and the hash would break).

import (
	"bytes"
	"testing"
)

// TestHandlerWritesCoroutineReadsSharedCellFSV: a death HANDLER and a parked
// marker COROUTINE share a closed cell `n`. Deaths fire both BEFORE and AFTER the
// save; the coroutine reads `n` only after the restore. n = 5 per death (2 deaths
// → 10), marker at x = 100+n = 110. If the handler's `n` and the coroutine's `n`
// were separate copies, the post-restore death (tick 700, after save@500) would
// bump only the handler's copy and the marker would read the saved 5 → x = 105.
func TestHandlerWritesCoroutineReadsSharedCellFSV(t *testing.T) {
	const src = `local n = 0
OnEvent(1, function() n = n + 5 end)
local f1 = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), {x = 500, y = 500}, 0)
local f2 = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), {x = 540, y = 500}, 0)
Run(function() PolledWait(15.0); Unit_Kill(f1); PolledWait(20.0); Unit_Kill(f2) end)
Run(function() PolledWait(50.0); Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x = 100 + n, y = 0}, 0) end)`
	const saveTick, total = 500, 1500

	// Unbroken reference: both deaths land, marker at x = 100+10 = 110.
	gR, LR := newGame(t)
	regR := runChunk(t, LR, "hc", src)
	gR.Advance(total)
	refHash := gR.StateHash()
	if n, x := liveUnits(gR); n != 1 || x < 109.5 || x > 110.5 {
		t.Fatalf("unbroken: want 1 unit x≈110 (100 + 2 deaths*5 via shared n), got %d x=%.1f", n, x)
	}
	LR.Close()
	regR.Close()

	// Save while the marker coroutine is parked and only ONE death has fired
	// (n=5 in the shared cell). The killer coroutine is parked before the 2nd kill.
	gA, LA := newGame(t)
	regA := runChunk(t, LA, "hc", src)
	gA.Advance(saveTick)
	var buf bytes.Buffer
	if err := Write(&buf, gA, LA, regA, fp); err != nil {
		t.Fatalf("Write@%d refused or failed — #446 save with a coroutine<->handler shared cell must now succeed: %v", saveTick, err)
	}
	LA.Close()
	regA.Close()

	// Restore cold and run to completion. The 2nd death (tick 700) fires on the
	// restored game; its handler must mutate the SAME cell the restored marker
	// coroutine reads at tick 1000.
	st := loadInto(t, "hc", src, buf.Bytes())
	defer st.L.Close()
	defer st.reg.Close()
	st.g.Advance(total - saveTick)

	if n, x := liveUnits(st.g); n != 1 || x < 109.5 || x > 110.5 {
		t.Fatalf("restored marker x=%.1f (n=%d), want x≈110 — a post-restore handler mutation did not reach the coroutine's shared cell (separate copies → x≈105)", x, n)
	}
	if h := st.g.StateHash(); h != refHash {
		t.Fatalf("HASH MISMATCH: restored %#x != unbroken %#x", h, refHash)
	}
	t.Logf("FSV #446: handler-mutated shared cell reached the parked coroutine across save/restore — marker x≈110, StateHash %#x == unbroken", refHash)
}

// TestCoroutineWritesHandlerReadsSharedTableFSV (the inverse + a table, not a
// cell): a parked COROUTINE mutates a shared table `s`; a death HANDLER reads it.
// The coroutine bumps s.v to 7 at tick 800 (after the save@500); a death at tick
// 1000 makes the handler spawn a marker at x = 200 + s.v. Only if the coroutine's
// `s` and the handler's `s` are ONE table does the handler see 7 → x = 207. Two
// copies → handler reads 0 → x = 200.
func TestCoroutineWritesHandlerReadsSharedTableFSV(t *testing.T) {
	const src = `local s = {v = 0}
OnEvent(1, function() Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x = 200 + s.v, y = 0}, 0) end)
local f1 = Game_CreateUnit(Game_Player(1), Game_UnitType("hfoo"), {x = 600, y = 600}, 0)
Run(function() PolledWait(40.0); s.v = s.v + 7 end)
Run(function() PolledWait(50.0); Unit_Kill(f1) end)`
	const saveTick, total = 500, 1500

	// Unbroken: coroutine sets s.v=7 at tick 800; death at tick 1000 → marker x=207.
	gR, LR := newGame(t)
	regR := runChunk(t, LR, "ch", src)
	gR.Advance(total)
	refHash := gR.StateHash()
	if n, x := liveUnits(gR); n != 1 || x < 206.5 || x > 207.5 {
		t.Fatalf("unbroken: want 1 unit x≈207 (handler reads coroutine's s.v=7), got %d x=%.1f", n, x)
	}
	LR.Close()
	regR.Close()

	// Save while BOTH coroutines are parked (s.v still 0, no death yet).
	gA, LA := newGame(t)
	regA := runChunk(t, LA, "ch", src)
	gA.Advance(saveTick)
	var buf bytes.Buffer
	if err := Write(&buf, gA, LA, regA, fp); err != nil {
		t.Fatalf("Write@%d refused or failed — #446 save with a coroutine<->handler shared table must now succeed: %v", saveTick, err)
	}
	LA.Close()
	regA.Close()

	st := loadInto(t, "ch", src, buf.Bytes())
	defer st.L.Close()
	defer st.reg.Close()
	st.g.Advance(total - saveTick)

	if n, x := liveUnits(st.g); n != 1 || x < 206.5 || x > 207.5 {
		t.Fatalf("restored marker x=%.1f (n=%d), want x≈207 — the handler did not see the coroutine's post-restore table mutation (separate copies → x≈200)", x, n)
	}
	if h := st.g.StateHash(); h != refHash {
		t.Fatalf("HASH MISMATCH: restored %#x != unbroken %#x", h, refHash)
	}
	t.Logf("FSV #446 inverse: coroutine-mutated shared table reached the death handler across save/restore — marker x≈207, StateHash %#x == unbroken", refHash)
}
