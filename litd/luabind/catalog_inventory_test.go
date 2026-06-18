package luabind

// Inventory action + upkeep getters (#267): Unit_EnableInventory / Unit_SwapItems
// and Player Gold/Lumber upkeep getters. SoT = the unit's inventory slots and the
// player's upkeep state, read via the Go api and compared to the Lua bindings.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	lua "github.com/yuin/gopher-lua"
)

func TestInventoryUpkeepBindingsFSV(t *testing.T) {
	g, hero := confGame(t, 37)
	if err := g.DefineEconomy(2); err != nil {
		t.Fatalf("DefineEconomy: %v", err)
	}
	if err := g.DefineItems([]data.Item{{ID: "a", Class: 0}, {ID: "b", Class: 0}}); err != nil {
		t.Fatalf("DefineItems: %v", err)
	}
	p1 := g.Player(1)
	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	setG := func(name string, v any) {
		ud := L.NewUserData()
		ud.Value = v
		L.SetGlobal(name, ud)
	}
	setG("u", hero)
	setG("p1", p1)

	// EnableInventory via Lua → true; proven by a subsequent AddItem succeeding.
	if err := L.DoString(`_en = Unit_EnableInventory(u)`); err != nil {
		t.Fatalf("Unit_EnableInventory: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_en")) {
		t.Fatal("Unit_EnableInventory returned false")
	}
	ia := g.CreateItem(g.ItemType("a"), api.Vec2{X: 110, Y: 100})
	ib := g.CreateItem(g.ItemType("b"), api.Vec2{X: 112, Y: 100})
	if !ia.Valid() || !ib.Valid() {
		t.Fatal("CreateItem fixture invalid")
	}
	if !hero.AddItem(ia) || !hero.AddItem(ib) {
		t.Fatal("AddItem failed — Unit_EnableInventory did not actually enable inventory")
	}
	// Slots fill in add order.
	if hero.ItemInSlot(0) != ia || hero.ItemInSlot(1) != ib {
		t.Fatalf("pre-swap slots wrong: slot0==ia=%v slot1==ib=%v", hero.ItemInSlot(0) == ia, hero.ItemInSlot(1) == ib)
	}

	// SwapItems via Lua → true; slots swap in the sim (Go SoT).
	if err := L.DoString(`_sw = Unit_SwapItems(u, 0, 1)`); err != nil {
		t.Fatalf("Unit_SwapItems: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_sw")) {
		t.Fatal("Unit_SwapItems returned false")
	}
	if hero.ItemInSlot(0) != ib || hero.ItemInSlot(1) != ia {
		t.Fatalf("post-swap SoT wrong: slot0==ib=%v slot1==ia=%v", hero.ItemInSlot(0) == ib, hero.ItemInSlot(1) == ia)
	}
	t.Logf("FSV #267 inventory: EnableInventory→AddItem ok; SwapItems(0,1) swapped slots in sim (Go SoT)")

	// Upkeep getters: each Lua read equals the Go api reading.
	if err := L.DoString(`_gur=Player_GoldUpkeepRate(p1); _lur=Player_LumberUpkeepRate(p1)
		_glt=Player_GoldLostToUpkeep(p1); _llt=Player_LumberLostToUpkeep(p1)`); err != nil {
		t.Fatalf("upkeep getters: %v", err)
	}
	if lv, gv := float64(lua.LVAsNumber(L.GetGlobal("_gur"))), p1.GoldUpkeepRate(); lv != gv {
		t.Fatalf("GoldUpkeepRate Lua=%v Go=%v", lv, gv)
	}
	if lv, gv := float64(lua.LVAsNumber(L.GetGlobal("_lur"))), p1.LumberUpkeepRate(); lv != gv {
		t.Fatalf("LumberUpkeepRate Lua=%v Go=%v", lv, gv)
	}
	if lv, gv := int(lua.LVAsNumber(L.GetGlobal("_glt"))), p1.GoldLostToUpkeep(); lv != gv {
		t.Fatalf("GoldLostToUpkeep Lua=%v Go=%v", lv, gv)
	}
	if lv, gv := int(lua.LVAsNumber(L.GetGlobal("_llt"))), p1.LumberLostToUpkeep(); lv != gv {
		t.Fatalf("LumberLostToUpkeep Lua=%v Go=%v", lv, gv)
	}
	t.Logf("FSV #267 upkeep: GoldRate=%v LumberRate=%v GoldLost=%d LumberLost=%d (all Lua==Go)",
		p1.GoldUpkeepRate(), p1.LumberUpkeepRate(), p1.GoldLostToUpkeep(), p1.LumberLostToUpkeep())
}
