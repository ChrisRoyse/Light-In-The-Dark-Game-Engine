package luabind

// #599 FSV — Go vs Lua ability authoring parity (R-ABL-5). SoT = the compiled
// ability content fingerprint (#596): the SAME ability authored through the Go
// AbilitySpecDef and through the Lua RegisterAbilitySpec table must compile to
// the same spec and therefore hash identically. A mismatch means the two
// surfaces diverge — an author would get different behavior from Go vs Lua.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// goAuthoredSpec is the canonical fireball authored through the Go surface.
func goAuthoredSpec() api.AbilitySpecDef {
	return api.AbilitySpecDef{
		ID: "fireball", Name: "Fireball", CastType: "active", Indicator: "line",
		CastRange: 900, ManaCost: 75, Cooldown: 6.0, Precast: 0.3, Backswing: 0.4,
		OnCast: []api.AbilityOpDef{
			{Op: "spawn_projectile"},
			{Op: "attach_mover", Mover: "linear", Speed: 30, Range: 900, Radius: 64, Pierce: 1, HitMask: 256},
			{Op: "set_kv", Key: "charges", Arg: 3},
		},
	}
}

// luaAuthoredScript authors the byte-equivalent fireball through Lua.
const luaAuthoredScript = `
ref, err = RegisterAbilitySpec{
  id = "fireball", name = "Fireball", cast_type = "active", indicator = "line",
  cast_range = 900, mana_cost = 75, cooldown = 6.0, precast = 0.3, backswing = 0.4,
  on_cast = {
    { op = "spawn_projectile" },
    { op = "attach_mover", mover = "linear", speed = 30, range = 900, radius = 64, pierce = 1, hitmask = 256 },
    { op = "set_kv", key = "charges", arg = 3 },
  },
}
`

func TestAbilityAuthoringGoVsLuaParity(t *testing.T) {
	// Go-authored world.
	gGo, err := api.NewGame(api.GameOptions{MaxUnits: 8, Seed: 1})
	if err != nil {
		t.Fatalf("NewGame(Go): %v", err)
	}
	ref, regErr := gGo.RegisterAbilitySpec(goAuthoredSpec())
	if regErr != nil || ref == 0 {
		t.Fatalf("Go RegisterAbilitySpec failed: ref=%d err=%v", ref, regErr)
	}
	fpGo := gGo.AbilityFingerprint()

	// Lua-authored world.
	gLua, err := api.NewGame(api.GameOptions{MaxUnits: 8, Seed: 1})
	if err != nil {
		t.Fatalf("NewGame(Lua): %v", err)
	}
	L := boundState(t, gLua)
	defer L.Close()
	if err := L.DoString(luaAuthoredScript); err != nil {
		t.Fatalf("Lua authoring script must run: %v", err)
	}
	if lref := L.GetGlobal("ref"); lref.String() == "0" {
		t.Fatalf("Lua RegisterAbilitySpec returned ref 0 (err=%v)", L.GetGlobal("err"))
	}
	fpLua := gLua.AbilityFingerprint()

	t.Logf("Go fingerprint   = %016x", fpGo)
	t.Logf("Lua fingerprint  = %016x", fpLua)
	if fpGo == 0 {
		t.Fatal("Go fingerprint is 0 (nothing registered)")
	}
	if fpGo != fpLua {
		t.Fatalf("Go vs Lua authoring diverged: %016x != %016x", fpGo, fpLua)
	}
}

// TestAbilityAuthoringLuaRejectsBadRef: a Lua spec naming an unknown mover kind
// fails closed — ref 0 and a validator message, never a half-registered spec.
func TestAbilityAuthoringLuaRejectsBadRef(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 8, Seed: 1})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	L := boundState(t, g)
	defer L.Close()
	script := `
ref, err = RegisterAbilitySpec{
  id = "broken", name = "Broken",
  on_cast = { { op = "attach_mover", mover = "not_a_mover_kind" } },
}
`
	if err := L.DoString(script); err != nil {
		t.Fatalf("script must run: %v", err)
	}
	ref := L.GetGlobal("ref")
	errMsg := L.GetGlobal("err")
	t.Logf("ref=%s err=%s", ref.String(), errMsg.String())
	if ref.String() != "0" {
		t.Fatalf("bad-ref spec registered ref=%s, want 0 (fail-closed)", ref.String())
	}
	if errMsg.Type().String() != "string" || errMsg.String() == "" {
		t.Fatal("expected a validator error string for the unknown mover kind")
	}
	if g.AbilityFingerprint() != g.AbilityFingerprint() {
		t.Fatal("nondeterministic fingerprint")
	}
}
