package main

// FSV for #481: the data-backed worlds round-trip a mid-game save through the
// production savegame container (sim state + suspended Lua scheduler). The fix
// that makes this possible: the world loader now runs the entry (and require'd
// siblings) through their REGISTERED prototypes, so every closure a world
// creates is persistable (previously SaveScripts failed closed on a separately
// compiled entry proto). SoT = Game.StateHash() (unbroken H1 vs save@N→load→H2)
// plus world-specific state.

import (
	"bytes"
	"path/filepath"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/savegame"
)

const worldFP = uint64(0x114D504C)

// TestDevSandboxSaveLoadFSV — dev-sandbox uses require() to install the firebolt
// trigger (#412/#479). Proves a require'd trigger's Lua closures persist: cast
// firebolt at an enemy, save mid-burn through the savegame container, reload into
// a freshly re-run world, and the burn resumes bit-identically.
func TestDevSandboxSaveLoadFSV(t *testing.T) {
	world := filepath.Join("..", "..", "worlds", "dev-sandbox")

	// reference: cast, run to tick 40 unbroken.
	ref := func() uint64 {
		g, _, _, cleanup, err := loadWorldFull(world, 1, 200_000_000)
		if err != nil {
			t.Fatalf("ref load: %v", err)
		}
		defer cleanup()
		castSandboxFirebolt(t, g)
		g.Advance(40)
		return g.StateHash()
	}()

	// save @ tick 8 (bolt landed, burn ticking).
	gs, Ls, regs, cls, err := loadWorldFull(world, 1, 200_000_000)
	if err != nil {
		t.Fatalf("save load: %v", err)
	}
	enemy := castSandboxFirebolt(t, gs)
	gs.Advance(16) // past the 0.5s cast point: bolt landed, burn ticking
	burnAtSave := enemy.HasBuff(gs.BuffType("burn"))
	var buf bytes.Buffer
	if err := savegame.Write(&buf, gs, Ls, regs, worldFP); err != nil {
		t.Fatalf("savegame.Write: %v", err)
	}
	cls()
	t.Logf("dev-sandbox save@16: burning=%v container=%d bytes", burnAtSave, buf.Len())
	if !burnAtSave {
		t.Fatal("precondition: enemy must be burning at save")
	}

	// restore: re-run the world (re-creates + re-binds the firebolt trigger), then
	// load the container, then re-stage the SAME cast? No — the cast is part of the
	// saved sim/script state, restored by Load. Re-running only rebuilds the
	// trigger graph + chunk registry.
	gr, Lr, regr, clr, err := loadWorldFull(world, 1, 200_000_000)
	if err != nil {
		t.Fatalf("restore load: %v", err)
	}
	defer clr()
	if err := savegame.Load(bytes.NewReader(buf.Bytes()), gr, Lr, regr, worldFP); err != nil {
		t.Fatalf("savegame.Load: %v", err)
	}
	gr.Advance(24) // 16 → 40
	got := gr.StateHash()
	t.Logf("dev-sandbox FSV #481: unbroken@40=%#016x  save@16→load→@40=%#016x  MATCH=%v", ref, got, got == ref)
	if got != ref {
		t.Fatalf("dev-sandbox mid-game save/load not bit-identical: %#x != %#x", got, ref)
	}
}

// castSandboxFirebolt makes p0/p1 enemies, spawns a caster + enemy, and casts
// firebolt. Returns the enemy. Identical on every call (deterministic ids).
func castSandboxFirebolt(t *testing.T, g *api.Game) api.Unit {
	t.Helper()
	p0, p1 := g.Player(0), g.Player(1)
	p0.SetAlliance(p1, 0)
	p1.SetAlliance(p0, 0)
	caster := g.CreateUnit(p0, g.UnitType("hfoo"), api.Vec2{X: 200, Y: 200}, api.Deg(0))
	enemy := g.CreateUnit(p1, g.UnitType("hfoo"), api.Vec2{X: 260, Y: 200}, api.Deg(0))
	ref, ok := g.AbilityRef("firebolt")
	if !ok {
		t.Fatal("firebolt must resolve")
	}
	fb := caster.AddAbility(ref)
	if !caster.Cast(fb, enemy) {
		t.Fatal("cast firebolt failed")
	}
	return enemy
}

// TestDeterminismLuaSaveLoadFSV — determinism-lua exercises OnEvent + coroutines
// (Run/PolledWait) + math.random. Save mid-run through the container, reload, and
// the run finishes bit-identically to the unbroken reference.
func TestDeterminismLuaSaveLoadFSV(t *testing.T) {
	world := filepath.Join("..", "..", "worlds", "determinism-lua")
	const saveAt, finish = 12, 60

	gr, _, _, clr, err := loadWorldFull(world, 7, 50_000_000)
	if err != nil {
		t.Fatalf("ref load: %v", err)
	}
	gr.Advance(finish)
	ref := gr.StateHash()
	clr()

	gs, Ls, regs, cls, err := loadWorldFull(world, 7, 50_000_000)
	if err != nil {
		t.Fatalf("save load: %v", err)
	}
	gs.Advance(saveAt)
	var buf bytes.Buffer
	if err := savegame.Write(&buf, gs, Ls, regs, worldFP); err != nil {
		t.Fatalf("savegame.Write: %v", err)
	}
	cls()

	gg, Lg, regg, clg, err := loadWorldFull(world, 7, 50_000_000)
	if err != nil {
		t.Fatalf("restore load: %v", err)
	}
	defer clg()
	if err := savegame.Load(bytes.NewReader(buf.Bytes()), gg, Lg, regg, worldFP); err != nil {
		t.Fatalf("savegame.Load: %v", err)
	}
	gg.Advance(finish - saveAt)
	got := gg.StateHash()
	t.Logf("determinism-lua FSV #481: unbroken@%d=%#016x save@%d→load→@%d=%#016x MATCH=%v",
		finish, ref, saveAt, finish, got, got == ref)
	if got != ref {
		t.Fatalf("determinism-lua mid-game save/load not bit-identical: %#x != %#x", got, ref)
	}
}
