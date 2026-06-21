package luabind

// Flicker-cycle integration FSV (#170 dim-phase empowerment, dogfooding #267):
// loads worlds/flicker-cycle — written purely against the bound surface
// (Game_Every, Game_AllUnits, Unit_HasBuff, Unit_ApplyBuff, Unit_RemoveAllBuffs,
// Storage) — and drives it headlessly. SoT = the phase published to Storage +
// each unit's buff state via the Go api, across ≥2 full cycles. Invariant:
// empowered IFF dim phase.

import (
	"bytes"
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

func flickerGame(t *testing.T, seed int64) (*api.Game, api.Unit) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: seed})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	if err := g.DefineBuffTypes([]data.BuffType{
		{ID: "dimpwr", DurationTicks: 50, Stacking: data.StackRefresh, MaxStacks: 1},
	}); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("unit invalid")
	}
	return g, u
}

func TestFlickerCycleWorldFSV(t *testing.T) {
	worldDir := filepath.Join("..", "..", "worlds", "flicker-cycle")

	g, u := flickerGame(t, 5)
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })
	if _, err := LoadWorld(L, reg, worldDir); err != nil {
		t.Fatalf("LoadWorld(flicker-cycle): %v", err)
	}
	bt := g.BuffType("dimpwr")

	phase := func() int { v, _ := g.Storage().GetInt("flicker", "phase"); return v }

	// Sample across 2.5 cycles; at every sample the invariant must hold:
	// the unit carries the empowerment IFF the published phase is dim(1).
	// 250 ticks at 20tps; sample every 10 ticks.
	const DIM = 1
	dimSamples, brightSamples := 0, 0
	for i := 0; i < 25; i++ {
		g.Advance(10)
		ph := phase()
		has := u.HasBuff(bt)
		if (ph == DIM) != has {
			t.Fatalf("tick~%d: phase=%d HasBuff=%v — empowerment must hold IFF dim", (i+1)*10, ph, has)
		}
		if ph == DIM {
			dimSamples++
		} else {
			brightSamples++
		}
	}
	if dimSamples == 0 || brightSamples == 0 {
		t.Fatalf("did not observe both phases over 2.5 cycles: dim=%d bright=%d", dimSamples, brightSamples)
	}
	trans, _ := g.Storage().GetInt("flicker", "transitions")
	if trans < 4 { // ≥2 full cycles ⇒ ≥4 phase transitions
		t.Fatalf("transitions=%d over 2.5 cycles, want ≥4", trans)
	}
	t.Logf("FSV #170 flicker: empowered IFF dim held over 25 samples (dim=%d bright=%d, transitions=%d)", dimSamples, brightSamples, trans)

	// --- Determinism edge: a second seeded run produces the identical cycle. ---
	g2, _ := flickerGame(t, 5)
	L2 := lua.NewState()
	if err := Register(L2, g2); err != nil {
		t.Fatalf("Register#2: %v", err)
	}
	reg2 := NewChunkRegistry()
	t.Cleanup(func() { L2.Close(); reg2.Close() })
	if _, err := LoadWorld(L2, reg2, worldDir); err != nil {
		t.Fatalf("LoadWorld#2: %v", err)
	}
	g2.Advance(250) // run 1 is already at tick 250 from the sampling loop above

	p1v, _ := g.Storage().GetInt("flicker", "phase")
	p2v, _ := g2.Storage().GetInt("flicker", "phase")
	t1v, _ := g.Storage().GetInt("flicker", "transitions")
	t2v, _ := g2.Storage().GetInt("flicker", "transitions")
	if p1v != p2v || t1v != t2v || g.StateHash() != g2.StateHash() {
		t.Fatalf("non-deterministic: run1 phase=%d trans=%d hash=%#x | run2 phase=%d trans=%d hash=%#x",
			p1v, t1v, g.StateHash(), p2v, t2v, g2.StateHash())
	}
	t.Logf("FSV #170 determinism: two seeded runs identical — phase=%d transitions=%d hash=%#x", p1v, t1v, g.StateHash())
}

// TestFlickerStripsOnlyItsOwnBuffFSV (#170 edge): the bright transition must
// remove ONLY the flicker's empowerment, not every buff on the unit. Bug: the
// return-to-bright branch called Unit_RemoveAllBuffs(u), nuking unrelated buffs
// (ability/item/aura). A unit carrying a persistent non-flicker buff must keep it
// across a dim->bright transition, while the flicker's dimpwr is stripped.
// SoT = the unit's buff state (Go api) after a bright transition.
func TestFlickerStripsOnlyItsOwnBuffFSV(t *testing.T) {
	worldDir := filepath.Join("..", "..", "worlds", "flicker-cycle")

	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 5})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	// dimpwr = the flicker buff; anthem = a long-lived NON-flicker buff (think an
	// aura/ability) the unit should retain regardless of the flicker phase.
	if err := g.DefineBuffTypes([]data.BuffType{
		{ID: "dimpwr", DurationTicks: 50, Stacking: data.StackRefresh, MaxStacks: 1},
		{ID: "anthem", DurationTicks: 60000, Stacking: data.StackRefresh, MaxStacks: 1},
	}); err != nil {
		t.Fatalf("DefineBuffTypes: %v", err)
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	reg := NewChunkRegistry()
	t.Cleanup(func() { L.Close(); reg.Close() })

	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("unit invalid")
	}
	anthem, dimpwr := g.BuffType("anthem"), g.BuffType("dimpwr")
	u.ApplyBuff(anthem) // a non-flicker buff present before the world runs

	if _, err := LoadWorld(L, reg, worldDir); err != nil {
		t.Fatalf("LoadWorld(flicker-cycle): %v", err)
	}

	// Cross one full dim->bright boundary. CYCLE=100 (bright 0..59, dim 60..99);
	// the return-to-bright strip runs at tick 100. Advance to 110 (bright).
	g.Advance(110)
	if ph, _ := g.Storage().GetInt("flicker", "phase"); ph != 0 {
		t.Fatalf("precondition: expected bright(0) at tick 110, got phase=%d", ph)
	}
	if u.HasBuff(dimpwr) {
		t.Fatalf("flicker's dimpwr should be stripped at the bright transition, but it persists")
	}
	if !u.HasBuff(anthem) {
		t.Fatalf("FLICKER STRIPPED A NON-FLICKER BUFF: the unit's anthem buff was removed at the bright transition — Unit_RemoveAllBuffs nukes EVERY buff, not just the flicker's empowerment; must use Unit_RemoveBuff(EMPWR)")
	}
	t.Logf("FSV #170 targeted-strip: bright transition removed dimpwr but kept the non-flicker anthem buff")
}

// loadFlicker builds a flicker game (seed) + a registered Lua runtime with the
// flicker-cycle world loaded. Returned so the save/load edge can re-create an
// identical runtime on the load side. Shared shape with TestFlickerCycleWorldFSV.
func loadFlicker(t *testing.T, seed int64) (*api.Game, api.Unit, *lua.LState, *ChunkRegistry) {
	t.Helper()
	g, u := flickerGame(t, seed)
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	reg := NewChunkRegistry()
	if _, err := LoadWorld(L, reg, filepath.Join("..", "..", "worlds", "flicker-cycle")); err != nil {
		t.Fatalf("LoadWorld(flicker-cycle): %v", err)
	}
	return g, u, L, reg
}

// TestFlickerSaveLoadAcrossPhaseFSV (#170 edge 2): a mid-DIM-phase save, loaded
// into a fresh runtime, resumes the tick-anchored cycle bit-identically to an
// unbroken run. This is the lockstep-safety proof: the flicker's cycle position
// lives in the Game_Every callback's Lua upvalues (t / lastPhase / transitions)
// — they must round-trip (#464/#270), or a loaded game would drift its day/night
// phase from a live one and desync a multiplayer match (D-5).
//
// SoT = published flicker/phase + flicker/transitions + Game.StateHash(), unbroken
// (H1) vs save@70(dim)→load→finish (H2).
func TestFlickerSaveLoadAcrossPhaseFSV(t *testing.T) {
	const fp = uint64(0xF11CCE12)
	const finish = 250

	// --- Unbroken reference run straight to `finish`. ---
	gRef, _, LRef, regRef := loadFlicker(t, 5)
	defer LRef.Close()
	defer regRef.Close()
	gRef.Advance(finish)
	refPhase, _ := gRef.Storage().GetInt("flicker", "phase")
	refTrans, _ := gRef.Storage().GetInt("flicker", "transitions")
	refHash := gRef.StateHash()
	t.Logf("UNBROKEN @%d: phase=%d transitions=%d hash=%#x", finish, refPhase, refTrans, refHash)

	// --- Save run: advance into the DIM phase (CYCLE=100, dim 60..99), save @70. ---
	const saveTick = 70
	gSave, _, LSave, regSave := loadFlicker(t, 5)
	defer LSave.Close()
	defer regSave.Close()
	gSave.Advance(saveTick)
	if ph, _ := gSave.Storage().GetInt("flicker", "phase"); ph != 1 {
		t.Fatalf("precondition: expected DIM(1) at tick %d, got phase=%d", saveTick, ph)
	}
	savePhase, _ := gSave.Storage().GetInt("flicker", "phase")
	saveTrans, _ := gSave.Storage().GetInt("flicker", "transitions")
	t.Logf("SAVE @%d: phase=%d transitions=%d (mid-dim)", saveTick, savePhase, saveTrans)
	var simBuf, scrBuf bytes.Buffer
	if err := gSave.SaveState(&simBuf, fp); err != nil {
		t.Fatalf("SaveState: %v", err)
	}
	if err := SaveScripts(LSave, regSave, &scrBuf); err != nil {
		t.Fatalf("SaveScripts: %v", err)
	}

	// --- Load into a fresh runtime: re-create the world (re-arms the periodic +
	// re-registers the action slot), then restore sim + scripts (upvalues). ---
	gLoad, _, LLoad, regLoad := loadFlicker(t, 5)
	defer LLoad.Close()
	defer regLoad.Close()
	if err := gLoad.LoadState(bytes.NewReader(simBuf.Bytes()), fp); err != nil {
		t.Fatalf("LoadState: %v", err)
	}
	if err := LoadScripts(LLoad, regLoad, bytes.NewReader(scrBuf.Bytes())); err != nil {
		t.Fatalf("LoadScripts: %v", err)
	}
	// Note: Storage is only re-published on the next tick, so reading it here
	// (before any post-load Advance) would show the stale pre-load value — the
	// resumed cycle position lives in the restored upvalues. The SoT is the final
	// @finish comparison below, which is driven by those upvalues.
	_ = saveTrans
	gLoad.Advance(finish - saveTick) // 70 → 250
	loadPhase, _ := gLoad.Storage().GetInt("flicker", "phase")
	loadTrans, _ := gLoad.Storage().GetInt("flicker", "transitions")
	loadHash := gLoad.StateHash()
	t.Logf("RESUMED @%d: phase=%d transitions=%d hash=%#x", finish, loadPhase, loadTrans, loadHash)

	if loadPhase != refPhase || loadTrans != refTrans || loadHash != refHash {
		t.Fatalf("save/load across phase boundary DRIFTED:\n  unbroken phase=%d trans=%d hash=%#x\n  resumed  phase=%d trans=%d hash=%#x",
			refPhase, refTrans, refHash, loadPhase, loadTrans, loadHash)
	}
	t.Logf("FSV #170 save/load: mid-dim save@70 → load → @250 BIT-IDENTICAL to unbroken (phase=%d trans=%d hash=%#x)", loadPhase, loadTrans, loadHash)
}

// TestFlickerUnitTrainedOnTransitionFSV (#170 edge 1): a unit created on the
// exact dim-transition tick must pick up the dim empowerment that same tick —
// the world empowers every un-buffed unit each dim tick (idempotent), so a unit
// born mid-transition is not skipped. SoT = the new unit's buff state.
func TestFlickerUnitTrainedOnTransitionFSV(t *testing.T) {
	g, _, L, reg := loadFlicker(t, 5)
	defer L.Close()
	defer reg.Close()
	dimpwr := g.BuffType("dimpwr")

	// CYCLE=100: bright 0..59, the first DIM tick is 60. Advance to 59 (still
	// bright), train a fresh unit, then step onto tick 60 (the dim publish).
	g.Advance(59)
	if ph, _ := g.Storage().GetInt("flicker", "phase"); ph != 0 {
		t.Fatalf("precondition: expected BRIGHT(0) at tick 59, got phase=%d", ph)
	}
	born := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
	if !born.Valid() {
		t.Fatal("trained unit invalid")
	}
	if born.HasBuff(dimpwr) {
		t.Fatal("precondition: unit born in bright must not yet carry dimpwr")
	}
	g.Advance(1) // step onto tick 60 — the dim-phase empower pass runs
	if ph, _ := g.Storage().GetInt("flicker", "phase"); ph != 1 {
		t.Fatalf("expected DIM(1) at tick 60, got phase=%d", ph)
	}
	if !born.HasBuff(dimpwr) {
		t.Fatal("unit trained on the bright→dim transition tick was NOT empowered — the dim empower pass skips units born that tick")
	}
	t.Logf("FSV #170 trained-on-transition: unit born at tick 59 is empowered on the tick-60 dim pass (idempotent empower)")
}
