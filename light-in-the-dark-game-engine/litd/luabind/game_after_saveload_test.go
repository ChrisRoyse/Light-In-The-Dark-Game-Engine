package luabind

// #661 FSV: a Game_After one-shot now round-trips a mid-game save. Before #661 it
// was a raw Go-closure timer (g.After) and a pending one-shot was silently dropped
// on load — it never fired after restore. It is now backed by a serializable
// single-fire timer (g.AfterCont on the timer wheel) whose callback is interned by
// slot (onceActions) and re-resolved to the scriptAfterCont bridge on load, exactly
// the #464 migration applied to single-fire.
//
// SoT = the Lua global `fireCount` the one-shot bumps when it fires — a world
// global, which SaveScripts persists and LoadScripts restores (unlike script-set
// Storage, which is not part of the sim save). Two directions:
//  (1) save BEFORE the fire tick → load → it STILL fires at the right tick (the
//      pending one-shot survived);
//  (2) save AFTER it fired → load → fireCount restores to 1 and the one-shot does
//      NOT fire again (no double-fire across the load-time entry re-run — the
//      restored timer wheel replaces the re-run's freshly-armed timer).

import (
	"bytes"
	"testing"

	lua "github.com/yuin/gopher-lua"
)

const afterProbeScript = `
	fireCount = 0                   -- world global: persisted by SaveScripts
	Game_After(0.5, function()      -- 0.5s / 0.05s-per-tick = fires once at tick 10
		fireCount = fireCount + 1
	end)`

func fireCountOf(L *lua.LState) int {
	if n, ok := L.GetGlobal("fireCount").(lua.LNumber); ok {
		return int(n)
	}
	return -1
}

// saveAfterProbeAt runs the one-shot probe, advances saveTick, and returns the sim
// + script blobs plus the fireCount at save time.
func saveAfterProbeAt(t *testing.T, saveTick int) (sim, scr []byte, atSave int) {
	t.Helper()
	const fp = uint64(0xA57E2)
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, afterProbeScript)
	g.Advance(saveTick)
	atSave = fireCountOf(L)
	var sb, cb bytes.Buffer
	if err := g.SaveState(&sb, fp); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	if err := SaveScripts(L, reg, &cb); err != nil {
		t.Fatalf("SaveScripts: %v", err)
	}
	return sb.Bytes(), cb.Bytes(), atSave
}

func TestGameAfterSaveLoadFiresAfterRestoreFSV(t *testing.T) {
	const fp = uint64(0xA57E2)

	// Unbroken reference: the one-shot fires exactly once by tick 20 → counter 1.
	gRef, LRef, regRef := newScriptGame(t)
	defer LRef.Close()
	defer regRef.Close()
	runRegisteredChunk(t, LRef, regRef, afterProbeScript)
	gRef.Advance(20)
	refCount := fireCountOf(LRef)
	refHash := gRef.StateHash()
	t.Logf("UNBROKEN @20: fireCount=%d hash=%#x", refCount, refHash)
	if refCount != 1 {
		t.Fatalf("unbroken one-shot fireCount=%d, want 1", refCount)
	}

	// --- (1) Save BEFORE the fire (tick 5, counter still 0). The pending one-shot
	//     must survive the load and fire on resume. ---
	sim, scr, atSave := saveAfterProbeAt(t, 5)
	if atSave != 0 {
		t.Fatalf("counter at save@5 = %d, want 0 (one-shot has not fired yet)", atSave)
	}
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, afterProbeScript) // re-run rebuilds onceActions + arms a fresh timer
	if err := g.LoadState(bytes.NewReader(sim), fp); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if err := LoadScripts(L, reg, bytes.NewReader(scr)); err != nil {
		t.Fatalf("LoadScripts: %v", err)
	}
	resumed := fireCountOf(L)
	if resumed != 0 {
		t.Fatalf("post-load@5 fireCount=%d, want 0 (must not have fired before tick 10)", resumed)
	}
	g.Advance(15) // 5 -> 20, crossing the fire tick 10
	gotCount := fireCountOf(L)
	gotHash := g.StateHash()
	t.Logf("RESUMED @20 (saved@5): fireCount=%d hash=%#x (want %d, %#x)", gotCount, gotHash, refCount, refHash)
	if gotCount != refCount {
		t.Fatalf("one-shot did not fire after restore: fireCount=%d, want %d — pending Game_After dropped (the #661 bug)", gotCount, refCount)
	}
	if gotHash != refHash {
		t.Fatalf("resumed StateHash %#x != unbroken %#x — one-shot save/load not bit-identical", gotHash, refHash)
	}

	// --- (2) Save AFTER the fire (tick 15, counter already 1). On load the one-shot
	//     must NOT fire again — the restored (empty) timer slot replaces the timer
	//     the load-time entry re-run freshly armed. ---
	// unbroken to tick 30 — the timer is long spent; this is the no-double-fire ref.
	gRef.Advance(10) // 20 -> 30
	ref30Count := fireCountOf(LRef)
	ref30Hash := gRef.StateHash()

	sim2, scr2, atSave2 := saveAfterProbeAt(t, 15)
	if atSave2 != 1 {
		t.Fatalf("fireCount at save@15 = %d, want 1 (one-shot already fired)", atSave2)
	}
	g2, L2, reg2 := newScriptGame(t)
	defer L2.Close()
	defer reg2.Close()
	runRegisteredChunk(t, L2, reg2, afterProbeScript)
	if err := g2.LoadState(bytes.NewReader(sim2), fp); err != nil {
		t.Fatalf("LoadState@15: %v", err)
	}
	if err := LoadScripts(L2, reg2, bytes.NewReader(scr2)); err != nil {
		t.Fatalf("LoadScripts@15: %v", err)
	}
	// Post-load fireCount must restore to 1 (the spent state), NOT the re-run's 0.
	if pl := fireCountOf(L2); pl != 1 {
		t.Fatalf("post-load@15 fireCount=%d, want 1 (the spent one-shot's global must restore)", pl)
	}
	g2.Advance(15) // 15 -> 30, well past any spuriously re-armed fire tick
	dbl := fireCountOf(L2)
	dblHash := g2.StateHash()
	t.Logf("RESUMED @30 (saved@15, after fire): fireCount=%d hash=%#x (want %d, %#x — no double-fire)", dbl, dblHash, ref30Count, ref30Hash)
	if dbl != 1 {
		t.Fatalf("one-shot fired again after restore: fireCount=%d, want 1 — double-fire across the load-time re-run (#661)", dbl)
	}
	if dblHash != ref30Hash {
		t.Fatalf("resumed@30 StateHash %#x != unbroken %#x — a spurious timer survived the restore", dblHash, ref30Hash)
	}
	t.Logf("FSV #661: Game_After round-trips — pending one-shot fires after restore, a spent one does not re-fire (timer-wheel hash matches unbroken)")
}
