package luabind

// #270 keystone FSV: a Lua coroutine suspended mid-PolledWait survives a
// save/load and resumes to a state BIT-IDENTICAL with the uninterrupted run.
// SoT = the sim StateHash (R-FSV-2) of the restored run vs the unbroken run at
// the same tick, plus the unit life the coroutine writes (read back through the
// Go api). No mocks; the coroutine, its captured handle, and its wake record all
// round-trip through real save/load, not a stub.

import (
	"bytes"
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
