package savegame

// #435 FSV: a world's top-level DATA globals (counters, strings, config tables)
// must survive a mid-game save/load. savegame.Load re-registers but never re-runs
// the world chunk (#270), so before #435 any top-level `g = ...` read back nil
// post-restore. The fix persists world globals (those added beyond Register's
// builtin baseline) in the shared scheduler graph and restores them into _G.
//
// SoT = (1) the restored global table read directly (LB.GetGlobal), the most
// direct source of truth, AND (2) a sim effect derived from a global by a
// coroutine that resumes only AFTER the restore — so a dropped global would
// surface as a nil-arithmetic script error and a missing unit, not a silent pass.

import (
	"bytes"
	"testing"

	lua "github.com/yuin/gopher-lua"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

type gameAndState struct {
	g   *api.Game
	L   *lua.LState
	reg *luabind.ChunkRegistry
}

// loadInto re-registers src in a cold game and loads buf into it, returning the
// restored game + state for direct inspection.
func loadInto(t *testing.T, name, src string, buf []byte) *gameAndState {
	t.Helper()
	g, L := newGame(t)
	reg := luabind.NewChunkRegistry()
	if _, err := reg.Register(name, src); err != nil {
		t.Fatalf("re-register: %v", err)
	}
	if err := Load(bytes.NewReader(buf), g, L, reg, fp); err != nil {
		t.Fatalf("Load: %v", err)
	}
	return &gameAndState{g: g, L: L, reg: reg}
}

// TestDataGlobalsSurviveSaveLoadFSV: a number, a string, and a config-table
// global set at chunk top level, plus a coroutine that spawns a marker at
// x=100+hits+cfg.power AFTER a wait. Saved while parked, restored cold, both the
// globals (read directly) and the derived marker (x=116) must come back.
func TestDataGlobalsSurviveSaveLoadFSV(t *testing.T) {
	const src = `hits = 7
streak = "lit"
cfg = {power = 9}
Run(function() PolledWait(50.0); Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x = 100 + hits + cfg.power, y = 0}, 0) end)`
	const saveTick, total = 500, 1500

	// Unbroken reference: marker at x = 100+7+9 = 116.
	gR, LR := newGame(t)
	regR := runChunk(t, LR, "g", src)
	gR.Advance(total)
	refHash := gR.StateHash()
	if n, x := liveUnits(gR); n != 1 || x < 115.5 || x > 116.5 {
		t.Fatalf("unbroken: want 1 unit x≈116 (100+hits+cfg.power), got %d x=%.1f", n, x)
	}
	LR.Close()
	regR.Close()

	// Save while the coroutine is parked (globals live, marker not yet spawned).
	gA, LA := newGame(t)
	regA := runChunk(t, LA, "g", src)
	gA.Advance(saveTick)
	var buf bytes.Buffer
	if err := Write(&buf, gA, LA, regA, fp); err != nil {
		t.Fatalf("Write@%d: %v", saveTick, err)
	}
	LA.Close()
	regA.Close()

	st := loadInto(t, "g", src, buf.Bytes())
	defer st.L.Close()
	defer st.reg.Close()

	// SoT (1): the restored global table directly.
	if got := st.L.GetGlobal("hits"); got != lua.LNumber(7) {
		t.Fatalf("restored global hits = %v, want 7 — data global dropped (#435)", got)
	}
	if got := st.L.GetGlobal("streak"); got != lua.LString("lit") {
		t.Fatalf("restored global streak = %v, want \"lit\"", got)
	}
	cfg, ok := st.L.GetGlobal("cfg").(*lua.LTable)
	if !ok {
		t.Fatalf("restored global cfg is %s, want table", st.L.GetGlobal("cfg").Type())
	}
	if got := cfg.RawGetString("power"); got != lua.LNumber(9) {
		t.Fatalf("restored cfg.power = %v, want 9", got)
	}

	// SoT (2): the marker the parked coroutine spawns from the globals post-restore.
	st.g.Advance(total - saveTick)
	if n, x := liveUnits(st.g); n != 1 || x < 115.5 || x > 116.5 {
		t.Fatalf("restored marker: want 1 unit x≈116 from restored globals, got %d x=%.1f (nil global → no/!misplaced unit)", n, x)
	}
	if h := st.g.StateHash(); h != refHash {
		t.Fatalf("HASH MISMATCH: restored %#x != unbroken %#x", h, refHash)
	}
	t.Logf("FSV #435: globals hits=7 streak=\"lit\" cfg.power=9 round-tripped (read directly), marker x≈116, StateHash %#x == unbroken", refHash)
}

// TestGlobalSharedWithCoroutineIdentityFSV (teeth for the single-pool fold):
// co1 mutates the GLOBAL `shared` by name; co2 captured `local s = shared` before
// the save and spawns at x=400+s.v. Only if the restored global and co2's captured
// local are the SAME object (one pool, #435+#440) does co1's +3 reach co2 → x=403.
// A separate globals section would decode two copies → x=400.
func TestGlobalSharedWithCoroutineIdentityFSV(t *testing.T) {
	const src = `shared = {v = 0}
Run(function() PolledWait(50.0); shared.v = shared.v + 3 end)
Run(function() local s = shared; PolledWait(60.0); Game_CreateUnit(Game_Player(0), Game_UnitType("hfoo"), {x = 400 + s.v, y = 0}, 0) end)`
	const saveTick, total = 500, 1500

	gR, LR := newGame(t)
	regR := runChunk(t, LR, "gs", src)
	gR.Advance(total)
	refHash := gR.StateHash()
	if n, x := liveUnits(gR); n != 1 || x < 402.5 || x > 403.5 {
		t.Fatalf("unbroken: want x≈403 (co2 sees co1's +3 via shared global), got %d x=%.1f", n, x)
	}
	LR.Close()
	regR.Close()

	gA, LA := newGame(t)
	regA := runChunk(t, LA, "gs", src)
	gA.Advance(saveTick)
	var buf bytes.Buffer
	if err := Write(&buf, gA, LA, regA, fp); err != nil {
		t.Fatalf("Write@%d: %v", saveTick, err)
	}
	LA.Close()
	regA.Close()

	st := loadInto(t, "gs", src, buf.Bytes())
	defer st.L.Close()
	defer st.reg.Close()
	st.g.Advance(total - saveTick)
	if n, x := liveUnits(st.g); n != 1 || x < 402.5 || x > 403.5 {
		t.Fatalf("restored marker x=%.1f (n=%d), want x≈403 — global and coroutine-captured local decoded as separate objects (#435 fold failed; x≈400)", x, n)
	}
	if sh, ok := st.L.GetGlobal("shared").(*lua.LTable); !ok || sh.RawGetString("v") != lua.LNumber(3) {
		t.Fatalf("restored global shared.v != 3 (co1's by-name mutation lost)")
	}
	if h := st.g.StateHash(); h != refHash {
		t.Fatalf("HASH MISMATCH: restored %#x != unbroken %#x", h, refHash)
	}
	t.Logf("FSV #435/#440: global `shared` and co2's captured local are ONE object — co1's by-name +3 reached co2 → x≈403; StateHash %#x == unbroken", refHash)
}
