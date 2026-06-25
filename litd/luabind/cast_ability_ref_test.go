package luabind

// #667 Unit_CastAbilityRef — the save-safe cast-by-ref verb that removes the #663
// footgun (capturing an Ability HANDLE in a serialized closure kills the save). The
// verb takes the ability REF (a plain int) and re-derives the handle internally via
// the idempotent Unit.AddAbility, so a closure that uses it captures only ints +
// entity-backed unit handles — all marshalable.
//
// SoT: (1) the ability granted on the caster (read through the Go api), (2) the cast
// boolean compared to the explicit AddAbility+Cast two-step (same cast machine), and
// (3) the SaveScripts result for a closure capturing the ref vs one capturing the
// handle — the latter must still hit the #663 marshal failure.

import (
	"bytes"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

func TestUnitCastAbilityRefFSV(t *testing.T) {
	g, caster := confGame(t, 67) // hfoo at (100,100), player 1
	target := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 140, Y: 100}, api.Deg(0))
	if !target.Valid() {
		t.Fatal("target unit invalid")
	}
	ref := g.RegisterAbility(api.AbilityDef{ID: "ablz", Name: "Blizzard", ManaCost: 0, Cooldown: 0})
	if ref == 0 {
		t.Fatal("RegisterAbility returned ref 0")
	}

	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	set := func(n string, v any) { ud := L.NewUserData(); ud.Value = v; L.SetGlobal(n, ud) }
	set("caster", caster)
	set("target", target)
	L.SetGlobal("ref", lua.LNumber(int(ref)))

	// SoT BEFORE: the caster has not been granted the ability yet (no AddAbility
	// call has run). A fresh AddAbility would be the FIRST grant.
	if got := g.StateHash(); got == 0 {
		t.Fatal("state hash zero — game not initialized")
	}

	// Happy path: cast-by-ref. The verb grants (idempotent AddAbility) then casts.
	if err := L.DoString(`ok = Unit_CastAbilityRef(caster, ref, target)`); err != nil {
		t.Fatalf("Unit_CastAbilityRef raised: %v", err)
	}
	okRef := L.GetGlobal("ok")

	// SoT AFTER: the ability is now present on the caster. AddAbility is idempotent,
	// so this returns the SAME instance the verb granted — a valid handle proves the
	// grant happened.
	ab := caster.AddAbility(ref)
	t.Logf("FSV #667 cast-by-ref: ok=%v, ability granted valid=%v", okRef, ab.Valid())
	if !ab.Valid() {
		t.Fatal("cast-by-ref did not grant the ability (AddAbility handle invalid)")
	}

	// Equivalence: the explicit two-step yields the same boolean — same cast machine.
	if err := L.DoString(`ok2 = Unit_CastAbility(caster, Unit_AddAbility(caster, ref), target)`); err != nil {
		t.Fatalf("explicit cast raised: %v", err)
	}
	if okRef != L.GetGlobal("ok2") {
		t.Fatalf("cast-by-ref %v != explicit AddAbility+Cast %v — not the same cast machine", okRef, L.GetGlobal("ok2"))
	}

	// Edge: unknown ref (0) must not panic and must not grant a phantom ability.
	L.SetGlobal("badref", lua.LNumber(0))
	if err := L.DoString(`okbad = Unit_CastAbilityRef(caster, badref, target)`); err != nil {
		t.Fatalf("cast-by-ref with ref 0 raised (should fail-closed to false, not panic): %v", err)
	}
	t.Logf("FSV #667 edge ref=0: okbad=%v (no panic, fail-closed)", L.GetGlobal("okbad"))
	if L.GetGlobal("okbad") != lua.LFalse {
		t.Fatalf("cast-by-ref with unknown ref should be false, got %v", L.GetGlobal("okbad"))
	}
}

// The headline #667 guarantee: a closure using cast-by-ref captures only the ref
// (an int) + unit handles and SAVES CLEAN, whereas the equivalent handle-capturing
// closure still dies at the marshal seam (#663). This is the whole reason the verb
// exists.
func TestUnitCastAbilityRefSaveSafeFSV(t *testing.T) {
	g, L, reg := newScriptGame(t)
	defer L.Close()
	defer reg.Close()

	caster := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	target := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 140, Y: 100}, api.Deg(0))
	ref := g.RegisterAbility(api.AbilityDef{ID: "ablz", Name: "Blizzard", ManaCost: 0, Cooldown: 0})
	if !caster.Valid() || !target.Valid() || ref == 0 {
		t.Fatalf("fixture broken: caster=%v target=%v ref=%d", caster.Valid(), target.Valid(), ref)
	}
	set := func(n string, v any) { ud := L.NewUserData(); ud.Value = v; L.SetGlobal(n, ud) }
	set("caster", caster)
	set("target", target)
	L.SetGlobal("smiteRef", lua.LNumber(int(ref)))

	// Save-safe: the closure captures smiteRef (int) + caster/target (entity-backed).
	runRegisteredChunk(t, L, reg, `
		Game_Every(0.05, function()
			Unit_CastAbilityRef(caster, smiteRef, target)
		end)`)

	var cb bytes.Buffer
	if err := SaveScripts(L, reg, &cb); err != nil {
		t.Fatalf("FSV #667: a cast-by-ref closure MUST save clean, but SaveScripts failed: %v", err)
	}
	t.Logf("FSV #667: cast-by-ref closure saved clean (%d bytes) — no Ability handle captured", cb.Len())
	if cb.Len() == 0 {
		t.Fatal("SaveScripts produced empty blob")
	}

	// Contrast (proves the footgun the verb avoids is real): a closure capturing the
	// Ability HANDLE still dies at the #663 marshal seam.
	g2, L2, reg2 := newScriptGame(t)
	defer L2.Close()
	defer reg2.Close()
	c2 := g2.CreateUnit(g2.Player(1), g2.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	t2 := g2.CreateUnit(g2.Player(2), g2.UnitType("hfoo"), api.Vec2{X: 140, Y: 100}, api.Deg(0))
	ref2 := g2.RegisterAbility(api.AbilityDef{ID: "ablz", Name: "Blizzard", ManaCost: 0, Cooldown: 0})
	set2 := func(n string, v any) { ud := L2.NewUserData(); ud.Value = v; L2.SetGlobal(n, ud) }
	set2("caster", c2)
	set2("target", t2)
	L2.SetGlobal("smiteRef", lua.LNumber(int(ref2)))
	runRegisteredChunk(t, L2, reg2, `
		local ab = Unit_AddAbility(caster, smiteRef)
		Game_Every(0.05, function()
			Unit_CastAbility(caster, ab, target)
		end)`)
	var cb2 bytes.Buffer
	err := SaveScripts(L2, reg2, &cb2)
	if err == nil {
		t.Fatal("FSV #667 contrast: a handle-capturing closure was expected to FAIL the save (#663), but it succeeded")
	}
	t.Logf("FSV #667 contrast: handle-capturing closure fails the save as expected: %v", err)
	if !strings.Contains(err.Error(), "#663") && !strings.Contains(err.Error(), "not entity-backed") {
		t.Fatalf("handle-capture save failed, but not with the #663 marshal error: %v", err)
	}
}
