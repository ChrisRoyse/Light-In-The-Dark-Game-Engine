package luabind

// #270 keystone FSV: a Lua coroutine suspended mid-PolledWait survives a
// save/load and resumes to a state BIT-IDENTICAL with the uninterrupted run.
// SoT = the sim StateHash (R-FSV-2) of the restored run vs the unbroken run at
// the same tick, plus the unit life the coroutine writes (read back through the
// Go api). No mocks; the coroutine, its captured handle, and its wake record all
// round-trip through real save/load, not a stub.

import (
	"bytes"
	"io"
	"os"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// coScript spawns one coroutine that creates a unit, sets its life, waits 3
// ticks (0.15s at 0.05s/tick), then sets it again. The local `u` lives on the
// coroutine stack across the wait, so save/load must serialize + rebind it.
const coScript = `Run(function()
	local u = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=100, y=100}, 0)
	Unit_SetLife(u, 30)
	PolledWait(0.15)
	Unit_SetLife(u, 77)
end)`

func newScriptGame(t *testing.T) (*api.Game, *lua.LState, *ChunkRegistry) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 7})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 4 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	return g, L, NewChunkRegistry()
}

// runRegisteredChunk registers src and runs it via the REGISTERED proto, so any
// coroutine it spawns has protos resolvable in reg (SaveThread maps protos to
// chunk ids). This mirrors how a persistence-correct world loader runs an entry.
func runRegisteredChunk(t *testing.T, L *lua.LState, reg *ChunkRegistry, src string) {
	t.Helper()
	cid, err := reg.Register("world", src)
	if err != nil {
		t.Fatalf("Register chunk: %v", err)
	}
	proto, err := reg.ResolveProto(cid, "")
	if err != nil {
		t.Fatalf("ResolveProto: %v", err)
	}
	L.Push(L.NewFunctionFromProto(proto))
	if err := L.PCall(0, 0, nil); err != nil {
		t.Fatalf("run chunk: %v", err)
	}
}

func soleUnitLife(t *testing.T, g *api.Game) float64 {
	t.Helper()
	us := g.AllUnits(nil)
	if len(us) != 1 {
		t.Fatalf("want exactly 1 unit, got %d", len(us))
	}
	return us[0].Life()
}

func TestSaveLoadLuaCoroutineRoundTripFSV(t *testing.T) {
	const fp = uint64(0xC0DEF00D) // world fingerprint tag

	// --- Reference: one unbroken run to completion. ---
	gRef, LRef, regRef := newScriptGame(t)
	defer LRef.Close()
	defer regRef.Close()
	runRegisteredChunk(t, LRef, regRef, coScript)
	if got := soleUnitLife(t, gRef); got != 30 {
		t.Fatalf("reference pre-advance life=%v, want 30 (set before the wait)", got)
	}
	gRef.Advance(5) // past the 3-tick wait → coroutine resumes
	refHash := gRef.StateHash()
	if got := soleUnitLife(t, gRef); got != 77 {
		t.Fatalf("reference final life=%v, want 77", got)
	}

	// --- Run a second game, save mid-wait. ---
	gA, LA, regA := newScriptGame(t)
	defer LA.Close()
	defer regA.Close()
	runRegisteredChunk(t, LA, regA, coScript)
	gA.Advance(1) // 1 < 3 ticks: coroutine still parked
	if got := soleUnitLife(t, gA); got != 30 {
		t.Fatalf("mid-run life=%v, want 30 (still parked)", got)
	}
	if n := PendingScriptWaits(LA); n != 1 {
		t.Fatalf("want 1 parked coroutine at save, got %d", n)
	}
	var simBlob, scriptBlob bytes.Buffer
	if err := gA.SaveState(&simBlob, fp); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	if err := SaveScripts(LA, regA, &scriptBlob); err != nil {
		t.Fatalf("SaveScripts: %v", err)
	}
	t.Logf("FSV save@tick1: sim=%dB script=%dB, 1 coroutine parked, life=30", simBlob.Len(), scriptBlob.Len())

	// --- Restore into a FRESH game + runtime. ---
	gB, LB, regB := newScriptGame(t)
	defer LB.Close()
	defer regB.Close()
	// Re-register the chunk (content-addressed → same id + protos) but do NOT
	// run it — running would re-spawn a duplicate coroutine. LoadScripts restores
	// the real one.
	if _, err := regB.Register("world", coScript); err != nil {
		t.Fatalf("re-register chunk: %v", err)
	}
	if err := gB.LoadState(&simBlob, fp); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if err := LoadScripts(LB, regB, &scriptBlob); err != nil {
		t.Fatalf("LoadScripts: %v", err)
	}
	if got := soleUnitLife(t, gB); got != 30 {
		t.Fatalf("post-restore life=%v, want 30 (restored parked, not yet resumed)", got)
	}
	if n := PendingScriptWaits(LB); n != 1 {
		t.Fatalf("post-restore want 1 parked coroutine, got %d", n)
	}

	// Advance the restored game to completion: the wake record must fire and
	// resume the restored coroutine.
	gB.Advance(4) // loaded at tick 1, wake at tick 3 → resumes within these 4
	if got := soleUnitLife(t, gB); got != 77 {
		t.Fatalf("restored final life=%v, want 77 (coroutine did not resume after restore)", got)
	}
	restoredHash := gB.StateHash()

	// SoT: bit-identical final state vs the unbroken run.
	if restoredHash != refHash {
		t.Fatalf("HASH MISMATCH: restored %#x != unbroken %#x — save/load is not bit-identical", restoredHash, refHash)
	}
	t.Logf("FSV #270 round-trip: save@1 → fresh restore → resume → life=77; final StateHash %#x == unbroken %#x", restoredHash, refHash)
}

// unbrokenHash runs script straight through totalTicks and returns the hash.
func unbrokenHash(t *testing.T, script string, totalTicks int) uint64 {
	t.Helper()
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, script)
	g.Advance(totalTicks)
	return g.StateHash()
}

// saveAt runs script, advances saveTick, and returns the sim + script blobs.
func saveAt(t *testing.T, script string, saveTick int) (sim, scr []byte) {
	t.Helper()
	const fp = uint64(0xC0DEF00D)
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, script)
	g.Advance(saveTick)
	var sb, cb bytes.Buffer
	if err := g.SaveState(&sb, fp); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	if err := SaveScripts(L, reg, &cb); err != nil {
		t.Fatalf("SaveScripts: %v", err)
	}
	return sb.Bytes(), cb.Bytes()
}

// restoreHash restores sim+scr into a fresh game with chunkSrc registered,
// advances `more` ticks, and returns the final hash. A nil error is required.
func restoreHash(t *testing.T, chunkSrc string, sim, scr []byte, more int) uint64 {
	t.Helper()
	const fp = uint64(0xC0DEF00D)
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	if _, err := reg.Register("world", chunkSrc); err != nil {
		t.Fatalf("register chunk: %v", err)
	}
	if err := g.LoadState(bytes.NewReader(sim), fp); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if err := LoadScripts(L, reg, bytes.NewReader(scr)); err != nil {
		t.Fatalf("LoadScripts: %v", err)
	}
	g.Advance(more)
	return g.StateHash()
}

// TestSaveLoad100CoroutinesFSV — the issue's headline edge: save while 100 Lua
// coroutines wait on 100 DIFFERENT ticks, restore through real on-disk save
// files in a fresh runtime, and prove (a) the wake schedule survives (pending
// count at save == post-restore), (b) the state hash AT the save tick is
// identical (the surviving wake records round-tripped byte-exactly), and (c) the
// final hash after every coroutine wakes is identical to the unbroken run.
func TestSaveLoad100CoroutinesFSV(t *testing.T) {
	const fp = uint64(0xC0DEF00D)
	// 100 coroutines: coroutine i parks for i ticks, so at tick 50 exactly the
	// first 50 have woken/finished and 50 remain parked.
	const script = `
		for i = 1, 100 do
			Run((function(n) return function() PolledWait(n * 0.05) end end)(i))
		end`
	const saveTick = 50

	// --- Unbroken reference: capture the hash at the save tick AND at the end. ---
	gR, LR, regR := newScriptGame(t)
	defer LR.Close()
	defer regR.Close()
	runRegisteredChunk(t, LR, regR, script)
	if got := PendingScriptWaits(LR); got != 100 {
		t.Fatalf("after spawn: pending=%d, want 100", got)
	}
	gR.Advance(saveTick)
	refMidHash := gR.StateHash()
	refMidPending := PendingScriptWaits(LR)
	gR.Advance(100 - saveTick + 1)
	refFinalHash := gR.StateHash()
	if got := PendingScriptWaits(LR); got != 0 {
		t.Fatalf("unbroken end: pending=%d, want 0 (all woke)", got)
	}
	t.Logf("FSV ref: tick%d pending=%d hash=%#x ; final pending=0 hash=%#x", saveTick, refMidPending, refMidHash, refFinalHash)

	// --- Save run: advance to the save tick, write sim + scripts to real files. ---
	gA, LA, regA := newScriptGame(t)
	defer LA.Close()
	defer regA.Close()
	runRegisteredChunk(t, LA, regA, script)
	gA.Advance(saveTick)
	if got := PendingScriptWaits(LA); got != refMidPending {
		t.Fatalf("save run pending=%d, want %d", got, refMidPending)
	}
	dir := t.TempDir()
	simPath := dir + "/state.sav"
	scrPath := dir + "/scripts.sav"
	writeSave(t, simPath, func(w io.Writer) error { return gA.SaveState(w, fp) })
	writeSave(t, scrPath, func(w io.Writer) error { return SaveScripts(LA, regA, w) })

	// --- Restore in a fresh runtime by READING the files back. ---
	gB, LB, regB := newScriptGame(t)
	defer LB.Close()
	defer regB.Close()
	if _, err := regB.Register("world", script); err != nil {
		t.Fatalf("re-register: %v", err)
	}
	readLoad(t, simPath, func(r io.Reader) error { return gB.LoadState(r, fp) })
	readLoad(t, scrPath, func(r io.Reader) error { return LoadScripts(LB, regB, r) })

	// SoT (a): wake schedule survived — same number parked.
	if got := PendingScriptWaits(LB); got != refMidPending {
		t.Fatalf("post-restore pending=%d, want %d (wake schedule lost)", got, refMidPending)
	}
	// SoT (b): hash AT the save tick is byte-identical (surviving records intact).
	if got := gB.StateHash(); got != refMidHash {
		t.Fatalf("post-restore tick%d HASH MISMATCH: %#x != %#x", saveTick, got, refMidHash)
	}
	// SoT (c): run the rest — every remaining coroutine wakes, final hash matches.
	gB.Advance(100 - saveTick + 1)
	if got := PendingScriptWaits(LB); got != 0 {
		t.Fatalf("restored end: pending=%d, want 0 (not all woke)", got)
	}
	if got := gB.StateHash(); got != refFinalHash {
		t.Fatalf("restored final HASH MISMATCH: %#x != %#x", got, refFinalHash)
	}
	t.Logf("FSV #270 100-coroutine on-disk round-trip: save@%d (pending %d → restore %d → final 0), tick%d hash %#x, final hash %#x — all == unbroken",
		saveTick, refMidPending, refMidPending, saveTick, refMidHash, refFinalHash)
}

// writeSave / readLoad round-trip a save section through a real file on disk, so
// the test exercises the on-disk save format, not just an in-memory buffer.
func writeSave(t *testing.T, path string, fn func(io.Writer) error) {
	t.Helper()
	f, err := os.Create(path)
	if err != nil {
		t.Fatalf("create %s: %v", path, err)
	}
	if err := fn(f); err != nil {
		f.Close()
		t.Fatalf("write %s: %v", path, err)
	}
	if err := f.Close(); err != nil {
		t.Fatalf("close %s: %v", path, err)
	}
}

func readLoad(t *testing.T, path string, fn func(io.Reader) error) {
	t.Helper()
	f, err := os.Open(path)
	if err != nil {
		t.Fatalf("open %s: %v", path, err)
	}
	defer f.Close()
	if err := fn(f); err != nil {
		t.Fatalf("load %s: %v", path, err)
	}
}

// TestSaveLoadMultiCoroutineFSV — three coroutines parked on DIFFERENT wake
// ticks at save time all resume on their correct tick after restore, and the
// final state is bit-identical to the unbroken run (#270 edge: many waiters).
func TestSaveLoadMultiCoroutineFSV(t *testing.T) {
	const script = `
		local function worker(x, life1, life2, wait)
			return function()
				local u = Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x=x, y=100}, 0)
				Unit_SetLife(u, life1)
				PolledWait(wait)
				Unit_SetLife(u, life2)
			end
		end
		Run(worker(100, 10, 60, 0.15))
		Run(worker(300, 20, 70, 0.25))
		Run(worker(500, 30, 80, 0.35))`

	const total = 8 // > 7-tick longest wait
	ref := unbrokenHash(t, script, total)
	sim, scr := saveAt(t, script, 1) // tick 1: all three still parked (waits 3,5,7)
	got := restoreHash(t, script, sim, scr, total-1)
	if got != ref {
		t.Fatalf("multi-coroutine HASH MISMATCH: restored %#x != unbroken %#x", got, ref)
	}
	t.Logf("FSV #270 multi-coroutine: 3 coroutines parked on ticks 3/5/7, saved@1, restored → final hash %#x == unbroken", got)
}

// TestSaveLoadChunkEditRefusedFSV — restoring against an EDITED world chunk is a
// loud refusal: the saved coroutine's proto is content-addressed, so its
// chunk-id is absent from the edited registry (#270 edge: modified world).
func TestSaveLoadChunkEditRefusedFSV(t *testing.T) {
	sim, scr := saveAt(t, coScript, 1)
	_ = sim
	// An edited script has a different content hash → different chunk id.
	edited := coScript + "\n-- edited\n"
	_, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	if _, err := reg.Register("world", edited); err != nil {
		t.Fatalf("register edited: %v", err)
	}
	if err := LoadScripts(L, reg, bytes.NewReader(scr)); err == nil {
		t.Fatal("LoadScripts accepted a coroutine against an edited chunk (must refuse)")
	} else {
		t.Logf("FSV #270 chunk-edit refusal: %v", err)
	}
}

// TestSaveLoadDoubleRestorePureFSV — restoring the SAME blobs twice yields
// identical final hashes: restore is pure, the blob is not consumed/mutated
// (#270 edge: double restore).
func TestSaveLoadDoubleRestorePureFSV(t *testing.T) {
	ref := unbrokenHash(t, coScript, 5)
	sim, scr := saveAt(t, coScript, 1)
	h1 := restoreHash(t, coScript, sim, scr, 4)
	h2 := restoreHash(t, coScript, sim, scr, 4)
	if h1 != h2 || h1 != ref {
		t.Fatalf("double-restore not pure/identical: h1=%#x h2=%#x ref=%#x", h1, h2, ref)
	}
	t.Logf("FSV #270 double-restore: two restores from one blob → identical %#x == unbroken", h1)
}

// TestSaveLoadCoroutineCorruptRefused — a truncated script blob is a loud
// refusal, never a partial restore (#270 fail-closed).
func TestSaveLoadCoroutineCorruptRefused(t *testing.T) {
	gA, LA, regA := newScriptGame(t)
	defer LA.Close()
	defer regA.Close()
	runRegisteredChunk(t, LA, regA, coScript)
	gA.Advance(1)
	var scriptBlob bytes.Buffer
	if err := SaveScripts(LA, regA, &scriptBlob); err != nil {
		t.Fatalf("SaveScripts: %v", err)
	}
	full := scriptBlob.Bytes()
	if len(full) < 16 {
		t.Fatalf("script blob unexpectedly small: %d", len(full))
	}

	_, LB, regB := newScriptGame(t)
	defer LB.Close()
	defer regB.Close()
	if _, err := regB.Register("world", coScript); err != nil {
		t.Fatalf("re-register: %v", err)
	}
	// Truncate the blob mid-image.
	truncated := bytes.NewReader(full[:len(full)-8])
	if err := LoadScripts(LB, regB, truncated); err == nil {
		t.Fatal("LoadScripts accepted a truncated blob (must fail closed)")
	} else {
		t.Logf("FSV #270 corrupt-refusal: truncated blob rejected — %v", err)
	}

	// Bad magic is also refused.
	if err := LoadScripts(LB, regB, bytes.NewReader([]byte("NOTLITD!"))); err == nil {
		t.Fatal("LoadScripts accepted bad magic (must fail closed)")
	} else {
		t.Logf("FSV #270 bad-magic refusal: %v", err)
	}
}
