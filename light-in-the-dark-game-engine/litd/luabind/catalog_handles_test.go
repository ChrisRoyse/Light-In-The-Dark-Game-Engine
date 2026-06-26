package luabind

// Handle-getter completeness (#267): Ability_Cooldown/ManaCost, Item_Carried/
// Carrier, Force_Count — getters whose siblings were already bound. SoT = the
// sim handle state, read through the Go api and compared to the Lua binding.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	lua "github.com/yuin/gopher-lua"
)

func TestHandleGetterBindingsFSV(t *testing.T) {
	g, hero := confGame(t, 31)
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

	// --- Force_Count: a force with two players reads 2. ---
	f := g.CreateForce()
	f.AddPlayer(g.Player(1))
	f.AddPlayer(g.Player(2))
	setG("force", f)
	if err := L.DoString(`_fc = Force_Count(force)`); err != nil {
		t.Fatalf("Force_Count: %v", err)
	}
	if lc, gc := int(lua.LVAsNumber(L.GetGlobal("_fc"))), f.Count(); lc != gc || gc != 2 {
		t.Fatalf("Force_Count Lua=%d Go=%d (want 2)", lc, gc)
	}
	t.Logf("FSV #267 Force_Count: Lua=Go=%d", f.Count())

	// --- Ability_Cooldown / ManaCost: register with known def, grant, read. ---
	ref := g.RegisterAbility(api.AbilityDef{ID: "ablz", Name: "Blizzard", ManaCost: 25, Cooldown: 6})
	if ref == 0 {
		t.Fatal("RegisterAbility returned ref 0")
	}
	ab := hero.AddAbility(ref)
	if !ab.Valid() {
		t.Fatal("AddAbility produced an invalid handle")
	}
	setG("ab", ab)
	if err := L.DoString(`_cd = Ability_Cooldown(ab); _mc = Ability_ManaCost(ab)`); err != nil {
		t.Fatalf("Ability getters: %v", err)
	}
	if lc, gc := float64(lua.LVAsNumber(L.GetGlobal("_cd"))), ab.Cooldown(); lc != gc {
		t.Fatalf("Ability_Cooldown Lua=%v Go=%v disagree", lc, gc)
	}
	if lm, gm := float64(lua.LVAsNumber(L.GetGlobal("_mc"))), ab.ManaCost(); lm != gm {
		t.Fatalf("Ability_ManaCost Lua=%v Go=%v disagree", lm, gm)
	}
	t.Logf("FSV #267 Ability: Cooldown Lua=Go=%v, ManaCost Lua=Go=%v", ab.Cooldown(), ab.ManaCost())

	// --- Item_Carried / Carrier: ground item vs carried item. ---
	if err := g.DefineItems([]data.Item{{ID: "potion", Class: 1, Charges: 2}}); err != nil {
		t.Fatalf("DefineItems: %v", err)
	}
	it := g.CreateItem(g.ItemType("potion"), api.Vec2{X: 110, Y: 100})
	if !it.Valid() {
		t.Fatal("CreateItem returned invalid handle")
	}
	setG("it", it)
	// Ground item: not carried, no carrier.
	if err := L.DoString(`_carried = Item_Carried(it); _carrier_valid = Valid(Item_Carrier(it))`); err != nil {
		t.Fatalf("ground item getters: %v", err)
	}
	if lua.LVAsBool(L.GetGlobal("_carried")) {
		t.Fatal("ground item reports Carried=true")
	}
	if lua.LVAsBool(L.GetGlobal("_carrier_valid")) {
		t.Fatal("ground item has a valid Carrier")
	}
	// Give it to the hero (enable inventory first), then it is carried by the hero.
	if !hero.EnableInventory() {
		t.Fatal("EnableInventory failed — fixture broken")
	}
	if !hero.AddItem(it) {
		t.Fatal("AddItem failed — fixture broken")
	}
	if !it.Carried() || it.Carrier() != hero {
		t.Fatalf("Go fixture wrong: carried=%v carrier==hero=%v", it.Carried(), it.Carrier() == hero)
	}
	if err := L.DoString(`_carried2 = Item_Carried(it); _carrier2_valid = Valid(Item_Carrier(it))`); err != nil {
		t.Fatalf("carried item getters: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_carried2")) {
		t.Fatal("carried item reports Carried=false via Lua")
	}
	if !lua.LVAsBool(L.GetGlobal("_carrier2_valid")) {
		t.Fatal("carried item Carrier invalid via Lua")
	}
	// Carrier correctness SoT is the Go check above (it.Carrier() == hero). On the
	// Lua side, the pooled marshaler must return a STABLE handle: two Carrier reads
	// of the same unit yield the identity-equal userdata.
	if err := L.DoString(`_same = (Item_Carrier(it) == Item_Carrier(it))`); err != nil {
		t.Fatalf("carrier pooling: %v", err)
	}
	if !lua.LVAsBool(L.GetGlobal("_same")) {
		t.Fatal("two Item_Carrier(it) reads returned different userdata (pooling broken)")
	}
	t.Logf("FSV #267 Item: ground (carried=false,carrier invalid); after AddItem carried=true, Carrier==hero (Go SoT) + stable pooled handle")
}
