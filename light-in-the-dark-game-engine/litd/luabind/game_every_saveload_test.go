package luabind

// FSV that a Game_Every periodic — the driver of every catalog world
// (firstflame, match-flow) — round-trips a mid-run save into a fresh runtime
// (#464, resolving the #204/#450/#433 class). Since #464 Game_Every is backed
// by a serializable periodic-timer Trigger (sim ArmPeriodic): the schedule is a
// value-typed scheduler continuation, the callback is interned by slot in the
// shared script pool (its captured upvalues — counter, Storage handle — round-
// trip), and re-running the entry on load re-creates the trigger and re-reads
// the slot. These tests previously PINNED the broken behavior (fail-closed +
// frozen counter) and were marked "flip when #204 lands"; they now assert the
// resume.
//
// SoT = the Storage counter + Game.StateHash() (H1 unbroken vs H2
// save@N→load→finish), plus the LoadState error string for the no-rebind and
// corruption paths.

import (
	"bytes"
	"strings"
	"testing"
)

const everyProbeScript = `
	local store = Game_Storage()
	local n = 0
	Game_Every(0.05, function()
		n = n + 1
		Storage_SetInt(store, "probe", "n", n)
	end)`

// saveEveryProbeAt runs the probe world, advances saveTick, and returns the sim
// + script save blobs plus the counter value at save time.
func saveEveryProbeAt(t *testing.T, saveTick int) (sim, scr []byte, atSave int) {
	t.Helper()
	const fp = uint64(0xC0DEF00D)
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, everyProbeScript)
	g.Advance(saveTick)
	atSave, _ = g.Storage().GetInt("probe", "n")
	var sb, cb bytes.Buffer
	if err := g.SaveState(&sb, fp); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	if err := SaveScripts(L, reg, &cb); err != nil {
		t.Fatalf("SaveScripts: %v", err)
	}
	return sb.Bytes(), cb.Bytes(), atSave
}

// TestGameEverySaveLoadRerunResumes — the headline #464 case (flipped from the
// old "does not resume"): re-run the entry, load the sim + scripts, and the
// periodic resumes bit-identically to an unbroken run. H1 (straight to 20) ==
// H2 (save@10 → load → run to 20), counters equal.
func TestGameEverySaveLoadRerunResumes(t *testing.T) {
	const fp = uint64(0xC0DEF00D)

	// unbroken reference run to tick 20.
	gRef, LRef, regRef := newScriptGame(t)
	defer LRef.Close()
	defer regRef.Close()
	runRegisteredChunk(t, LRef, regRef, everyProbeScript)
	gRef.Advance(20)
	refCount, _ := gRef.Storage().GetInt("probe", "n")
	refHash := gRef.StateHash()
	t.Logf("UNBROKEN @20: counter=%d hash=%#x", refCount, refHash)
	if refCount != 20 {
		t.Fatalf("unbroken counter=%d, want 20", refCount)
	}

	// save@10 → fresh runtime → re-run entry → load sim + scripts → resume.
	sim, scr, atSave := saveEveryProbeAt(t, 10)
	if atSave != 10 {
		t.Fatalf("counter at save = %d, want 10", atSave)
	}
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, everyProbeScript) // re-create the trigger + re-register the action slot
	if err := g.LoadState(bytes.NewReader(sim), fp); err != nil {
		t.Fatalf("LoadState after re-run: %v", err)
	}
	if err := LoadScripts(L, reg, bytes.NewReader(scr)); err != nil {
		t.Fatalf("LoadScripts after re-run: %v", err)
	}
	resumed, _ := g.Storage().GetInt("probe", "n")
	t.Logf("RESTORED @10: counter=%d (the periodic's upvalues reattached)", resumed)
	g.Advance(10) // 10 → 20

	gotCount, _ := g.Storage().GetInt("probe", "n")
	gotHash := g.StateHash()
	t.Logf("RESUMED @20: counter=%d hash=%#x (want counter=%d, hash=%#x)", gotCount, gotHash, refCount, refHash)
	if gotCount != refCount {
		t.Fatalf("resumed counter=%d, want %d (unbroken) — periodic did not resume cleanly", gotCount, refCount)
	}
	if gotHash != refHash {
		t.Fatalf("resumed StateHash=%#x != unbroken %#x — save/load is not bit-identical", gotHash, refHash)
	}
}

// TestGameEverySaveLoadFailsClosedWithoutRebind — restoring WITHOUT re-running
// the entry still fails closed loudly (never a silent dead-world load): the
// saved trigger graph references handler identities the fresh world has not
// re-registered, so LoadState refuses against the handler registry.
func TestGameEverySaveLoadFailsClosedWithoutRebind(t *testing.T) {
	const fp = uint64(0xC0DEF00D)
	sim, _, atSave := saveEveryProbeAt(t, 10)
	if atSave != 10 {
		t.Fatalf("probe counter at save = %d, want 10", atSave)
	}
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	if _, err := reg.Register("world", everyProbeScript); err != nil {
		t.Fatalf("re-register: %v", err)
	}
	// NB: chunk registered but NOT run — the trigger + its action handler are
	// never re-created, so the registry is empty.
	err := g.LoadState(bytes.NewReader(sim), fp)
	if err == nil {
		t.Fatal("LoadState SUCCEEDED — expected a fail-closed refusal of the unrebound trigger graph")
	}
	if !strings.Contains(err.Error(), "registry") && !strings.Contains(err.Error(), "registered") {
		t.Fatalf("LoadState error = %q, want it to name the unresolved handler/registry mismatch", err)
	}
	t.Logf("FSV fail-closed without rebind: %v", err)
}

// TestGameEverySaveLoadCorruptByteRefused — corrupting one byte of the sim save
// is refused loudly; no partial world loads.
func TestGameEverySaveLoadCorruptByteRefused(t *testing.T) {
	const fp = uint64(0xC0DEF00D)
	sim, _, _ := saveEveryProbeAt(t, 10)

	corrupt := append([]byte(nil), sim...)
	corrupt[len(corrupt)/2] ^= 0xFF // flip a byte in the middle of the blob

	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, everyProbeScript)
	err := g.LoadState(bytes.NewReader(corrupt), fp)
	t.Logf("corrupt-byte LoadState → err=%v", err)
	if err == nil {
		t.Fatal("LoadState accepted a corrupted save — must fail closed")
	}
}
