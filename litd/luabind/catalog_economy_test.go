package luabind

// Player economy + Unit vitals accessors (#267): the split/convenience getters
// the generator left unbound (Gold bound but not Lumber/Food; SetMana bound but
// not the Mana getter). SoT = the sim player/unit state, cross-checked via the
// Go api after Lua writes.

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

func TestEconomyVitalsBindingsFSV(t *testing.T) {
	g, hero := confGame(t, 29)
	// SetLumber/SetFoodCap no-op until the resource ledger is initialised (#388).
	if err := g.DefineEconomy(2); err != nil {
		t.Fatalf("DefineEconomy: %v", err)
	}
	p1 := g.Player(1)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	pud := L.NewUserData()
	pud.Value = p1
	L.SetGlobal("p1", pud)
	uud := L.NewUserData()
	uud.Value = hero
	L.SetGlobal("u", uud)

	// Player economy round-trips (set from Lua, read back Lua + Go SoT).
	if err := L.DoString(`Player_SetLumber(p1, 500); Player_SetFoodCap(p1, 80)
		_lum = Player_Lumber(p1); _cap = Player_FoodCap(p1); _used = Player_FoodUsed(p1)`); err != nil {
		t.Fatalf("economy lua: %v", err)
	}
	if v := int(lua.LVAsNumber(L.GetGlobal("_lum"))); v != 500 {
		t.Fatalf("Lua Player_Lumber = %d, want 500", v)
	}
	if v := int(lua.LVAsNumber(L.GetGlobal("_cap"))); v != 80 {
		t.Fatalf("Lua Player_FoodCap = %d, want 80", v)
	}
	// Cross-check Go SoT.
	if gl := p1.Lumber(); gl != 500 {
		t.Fatalf("Go SoT Player.Lumber = %d, want 500 — Lua write did not persist", gl)
	}
	if gc := p1.FoodCap(); gc != 80 {
		t.Fatalf("Go SoT Player.FoodCap = %d, want 80", gc)
	}
	// FoodUsed: Lua read must equal the Go read (whatever the value).
	if lu, gu := int(lua.LVAsNumber(L.GetGlobal("_used"))), p1.FoodUsed(); lu != gu {
		t.Fatalf("Player_FoodUsed Lua=%d Go=%d disagree", lu, gu)
	}
	t.Logf("FSV #267 economy: Lumber 500 / FoodCap 80 round-trip (Lua+Go agree); FoodUsed Lua==Go")

	// Unit vitals: the Mana getter must agree with the Go api (cross-language
	// correctness); IsHero likewise. confGame's footman has no mana pool and is
	// not a hero — read both rather than assume, and compare to Go.
	if err := L.DoString(`_mana = Unit_Mana(u); _hero = Unit_IsHero(u)`); err != nil {
		t.Fatalf("vitals lua: %v", err)
	}
	if lm, gm := float64(lua.LVAsNumber(L.GetGlobal("_mana"))), hero.Mana(); lm != gm {
		t.Fatalf("Unit_Mana Lua=%v Go=%v disagree", lm, gm)
	}
	if lh, gh := lua.LVAsBool(L.GetGlobal("_hero")), hero.IsHero(); lh != gh {
		t.Fatalf("Unit_IsHero Lua=%v Go=%v disagree", lh, gh)
	}
	t.Logf("FSV #267 vitals: Unit_Mana=%v Unit_IsHero=%v (both equal Go api SoT)",
		float64(lua.LVAsNumber(L.GetGlobal("_mana"))), lua.LVAsBool(L.GetGlobal("_hero")))
}
