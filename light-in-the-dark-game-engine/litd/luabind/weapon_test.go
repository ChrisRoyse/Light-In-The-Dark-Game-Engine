package luabind

// FSV for the #476 Lua weapon-override surface: Unit_SetWeaponField /
// Unit_ClearWeaponField / Unit_WeaponField + the WEAPON_* field constants.
// SoT = the value read back from the sim through Unit_WeaponField, and the real
// attack damage the override produces.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// weaponLuaGame: an attacker armed with a base-10 instant weapon (normal type)
// next to a high-life victim, both reachable as Lua globals `atk` / `vic`.
func weaponLuaGame(t *testing.T) (*api.Game, *lua.LState, api.Unit, api.Unit) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 5})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineDamageTypes([]string{"normal", "holy"}, []string{"unarmored"}); err != nil {
		t.Fatalf("DefineDamageTypes: %v", err)
	}
	if err := g.DefineCombat([][]int{{1000}, {2000}}); err != nil {
		t.Fatalf("DefineCombat: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{{
		ID: "hfoo", Life: 1000, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16,
		Attacks: []data.Attack{{AttackType: 0, DamageBase: 10, Range: fixed.FromInt(300), CooldownTicks: 30}},
	}}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	atk := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	vic := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 0}, api.Deg(0))
	if !atk.Valid() || !vic.Valid() {
		t.Fatal("unit spawn failed")
	}
	L := lua.NewState()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	set := func(name string, v any) {
		ud := L.NewUserData()
		ud.Value = v
		L.SetGlobal(name, ud)
	}
	set("atk", atk)
	set("vic", vic)
	return g, L, atk, vic
}

// TestLuaSetWeaponFieldFSV — a script overrides the weapon damage base 10→40;
// Unit_WeaponField reads back 40, and a clear reverts to 10.
func TestLuaSetWeaponFieldFSV(t *testing.T) {
	_, L, atk, _ := weaponLuaGame(t)
	defer L.Close()
	if err := L.DoString(`
		ok = Unit_SetWeaponField(atk, 0, WEAPON_DAMAGE_BASE, 40)
		v = Unit_WeaponField(atk, 0, WEAPON_DAMAGE_BASE)
	`); err != nil {
		t.Fatalf("DoString set: %v", err)
	}
	if ok := L.GetGlobal("ok"); ok != lua.LTrue {
		t.Fatalf("Unit_SetWeaponField returned %v, want true", ok)
	}
	if v := L.GetGlobal("v"); v != lua.LNumber(40) {
		t.Fatalf("Unit_WeaponField after set = %v, want 40", v)
	}
	// confirm against the sim store directly (SoT).
	if sv, _ := atk.WeaponField(0, api.WeaponDamageBase); sv != 40 {
		t.Fatalf("sim store = %d, want 40", sv)
	}
	if err := L.DoString(`
		cleared = Unit_ClearWeaponField(atk, 0, WEAPON_DAMAGE_BASE)
		v2 = Unit_WeaponField(atk, 0, WEAPON_DAMAGE_BASE)
	`); err != nil {
		t.Fatalf("DoString clear: %v", err)
	}
	if L.GetGlobal("cleared") != lua.LTrue || L.GetGlobal("v2") != lua.LNumber(10) {
		t.Fatalf("after clear: cleared=%v v2=%v, want true/10", L.GetGlobal("cleared"), L.GetGlobal("v2"))
	}
	t.Logf("FSV #476 lua: set 10→40 (read back 40, sim store 40), clear→10")
}

// TestLuaWeaponAttackTypeOverrideFSV — a script switches the weapon to the holy
// attack type; a real attack then deals the holy-scaled damage (2× normal).
func TestLuaWeaponAttackTypeOverrideFSV(t *testing.T) {
	g, L, atk, vic := weaponLuaGame(t)
	defer L.Close()
	holy, ok := g.AttackTypeID("holy")
	if !ok {
		t.Fatal("AttackTypeID(holy) not found")
	}
	if err := L.DoString(`ok = Unit_SetWeaponField(atk, 0, WEAPON_ATTACK_TYPE, ` + itoa(holy) + `)`); err != nil {
		t.Fatalf("DoString: %v", err)
	}
	if L.GetGlobal("ok") != lua.LTrue {
		t.Fatal("Unit_SetWeaponField(attack type) returned false")
	}
	if !atk.Order(api.OrderAttack, api.TargetUnit(vic)) {
		t.Fatal("Order(Attack) returned false")
	}
	before := vic.Life()
	g.Advance(120) // let the attacker wind up and fire
	dealt := before - vic.Life()
	t.Logf("FSV #476 lua attack-type holy: victim life %v→%v, dealt=%v (base 10 × holy 2×)", before, vic.Life(), dealt)
	if dealt < 20 {
		t.Fatalf("dealt %v, want >=20 (holy-scaled base 10) — override did not reach the attack", dealt)
	}
}
