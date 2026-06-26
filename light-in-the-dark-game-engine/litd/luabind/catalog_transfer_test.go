package luabind

// Game_TransferResource + Resource_* constants (#267): a giver-taxed resource
// transfer between players, bound via the catalog (the generator skips *Game
// methods taking two Player args). SoT = the players' resource ledgers read via
// the Go api (Player.Gold), cross-checked against the Lua return (net delivered).

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

func TestGameTransferResourceBindingFSV(t *testing.T) {
	g, _ := confGame(t, 71)
	if err := g.DefineEconomy(2); err != nil {
		t.Fatalf("DefineEconomy: %v", err)
	}
	p1, p2, p3 := g.Player(1), g.Player(2), g.Player(3)
	p1.SetGold(1000)
	p2.SetGold(0)
	p3.SetGold(0)
	p1.SetTaxRate(p2, 0, 0.25) // 25% gold transfer tax p1->p2 (resource 0 = gold)

	L := lua.NewState()
	defer L.Close()
	if err := Register(L, g); err != nil {
		t.Fatalf("Register: %v", err)
	}
	for name, p := range map[string]interface{}{"p1": p1, "p2": p2, "p3": p3} {
		ud := L.NewUserData()
		ud.Value = p
		L.SetGlobal(name, ud)
	}

	transfer := func(src, dst, amount string) int {
		if err := L.DoString(`_net = Game_TransferResource(` + src + `, ` + dst + `, Resource_Gold, ` + amount + `)`); err != nil {
			t.Fatalf("Game_TransferResource(%s,%s,%s): %v", src, dst, amount, err)
		}
		return int(lua.LVAsNumber(L.GetGlobal("_net")))
	}

	// Confirm the named constant resolved.
	if err := L.DoString(`assert(Resource_Gold == 0, "Resource_Gold")`); err != nil {
		t.Fatalf("Resource_Gold constant: %v", err)
	}

	// --- Happy path: 400 gold, 25% tax → 300 delivered, 100 destroyed. ---
	if p1.Gold() != 1000 || p2.Gold() != 0 {
		t.Fatalf("BEFORE: p1=%d p2=%d, want 1000/0", p1.Gold(), p2.Gold())
	}
	if net := transfer("p1", "p2", "400"); net != 300 {
		t.Fatalf("net delivered = %d, want 300", net)
	}
	if p1.Gold() != 600 || p2.Gold() != 300 {
		t.Fatalf("AFTER transfer: p1=%d p2=%d, want 600/300 (giver -400, receiver +300, 100 taxed away)", p1.Gold(), p2.Gold())
	}
	t.Logf("FSV #267 transfer: p1 1000→600, p2 0→300, net=300 (25%% tax destroyed 100) — Go ledger SoT")

	// --- Edge 1: untaxed pair (p1->p3, no tax rate set) → full amount. ---
	if net := transfer("p1", "p3", "100"); net != 100 {
		t.Fatalf("untaxed net = %d, want 100", net)
	}
	if p1.Gold() != 500 || p3.Gold() != 100 {
		t.Fatalf("untaxed AFTER: p1=%d p3=%d, want 500/100", p1.Gold(), p3.Gold())
	}

	// --- Edge 2: over-balance request capped at the giver's balance (500). ---
	if net := transfer("p1", "p2", "99999"); net != 375 { // 500 capped, 25% tax → 375
		t.Fatalf("over-balance net = %d, want 375 (500 - 25%% tax)", net)
	}
	if p1.Gold() != 0 || p2.Gold() != 675 {
		t.Fatalf("over-balance AFTER: p1=%d p2=%d, want 0/675", p1.Gold(), p2.Gold())
	}
	t.Logf("FSV #267 transfer edges: untaxed=full(100); over-balance capped at 500 → net 375")

	// --- Edge 3: self-transfer is rejected (fail-closed) — no ledger change. ---
	p2before := p2.Gold()
	if net := transfer("p2", "p2", "100"); net != 0 {
		t.Fatalf("self-transfer net = %d, want 0 (rejected)", net)
	}
	if p2.Gold() != p2before {
		t.Fatalf("self-transfer mutated ledger: %d → %d", p2before, p2.Gold())
	}
	t.Logf("FSV #267 transfer edge: self-transfer rejected (net=0, ledger unchanged at %d)", p2before)
}
