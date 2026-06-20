package luabind

// Characterization of a KNOWN save/load limitation (#204, gating #200 edge-3 /
// #201 / #210 edge-3): a Game_Every periodic callback — the driver of every catalog
// world (firstflame, match-flow) — does NOT round-trip a mid-run save into a fresh
// runtime. The callback bridges a sim timer (g.Every) to a Lua closure via a Go
// continuation; that continuation is not reconstructable from the serialized bytes.
//
// This pins the EXACT current behavior (so a regression that silently "loads" a
// dead world is caught, and so #204's fix flips this test) and documents that the
// save layer fails CLOSED (§1.9): it refuses the load loudly rather than resuming a
// broken world. The callback captures a Storage userdata handle + an upvalue
// counter — exactly firstflame's holdSteps pattern — so it is a faithful proxy.
//
// SoT = the LoadState error string (mode 1) and the Storage counter (mode 2).

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

// saveEveryProbeAt runs the probe world, advances saveTick, and returns the sim +
// script save blobs plus the counter value at save time.
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

// Mode 1 — restore WITHOUT re-running the entry: the saved sim scheduler references
// the Game_Every continuation, which a fresh runtime never registered, so LoadState
// fails closed with a named error (never a silent dead-world load).
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
	err := g.LoadState(bytes.NewReader(sim), fp)
	if err == nil {
		t.Fatalf("LoadState SUCCEEDED — expected a fail-closed refusal of the unresolved Game_Every continuation (#204 may be fixed; flip this test)")
	}
	if !strings.Contains(err.Error(), "unregistered ContID") {
		t.Fatalf("LoadState error = %q, want it to name the unregistered continuation (fail-closed posture changed)", err)
	}
	t.Logf("FSV #204 fail-closed: a Game_Every world save refuses to load into a fresh runtime — %v", err)
}

// Mode 2 — restore by RE-RUNNING the entry first (so Game_Every re-registers a
// continuation under the same deterministic ContID, letting LoadState resolve):
// LoadState now succeeds, but the re-run's fresh closures are not the ones the
// saved state references, so the periodic callback never fires after restore and
// the saved upvalue is not reinstated. The counter stays frozen — proof that the
// re-run protocol does NOT recover the running world (it only avoids the crash).
func TestGameEverySaveLoadRerunDoesNotResume(t *testing.T) {
	const fp = uint64(0xC0DEF00D)
	sim, scr, _ := saveEveryProbeAt(t, 10)

	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()
	runRegisteredChunk(t, L, reg, everyProbeScript) // re-run: re-registers the continuation
	if n, _ := g.Storage().GetInt("probe", "n"); n != 0 {
		t.Fatalf("after fresh re-run, counter = %d, want 0", n)
	}
	if err := g.LoadState(bytes.NewReader(sim), fp); err != nil {
		t.Fatalf("LoadState after re-run: %v (re-run was expected to satisfy the continuation reference)", err)
	}
	if err := LoadScripts(L, reg, bytes.NewReader(scr)); err != nil {
		t.Fatalf("LoadScripts after re-run: %v", err)
	}
	g.Advance(10) // unbroken would reach 20 here
	final, _ := g.Storage().GetInt("probe", "n")
	if final != 0 {
		t.Fatalf("restored Game_Every callback FIRED (counter=%d) — the re-run protocol now resumes the world; #204 may be fixed, flip this test", final)
	}
	t.Logf("FSV #204 re-run gap: after re-run+load+resume, Game_Every counter frozen at %d (unbroken reaches 20) — the periodic does not resume", final)
}
