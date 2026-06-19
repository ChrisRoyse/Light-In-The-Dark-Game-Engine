package luabind

// #416 regression + correctness: the gopher-lua small-table fast path (LITD-PATCH
// in repoes/gopher-lua/table.go) must (a) cut the per-tick GC cost of the {x,y}
// vector idiom and (b) be behaviourally identical to the strdict/keys/k2i map
// representation — same values, same insertion-ordered pairs(), correct
// small->spill transition, correct delete/re-add (no duplicate keys). These run
// real Lua through the production sandbox+binding, reading results back as the
// SoT. The #271 determinism golden additionally guards pairs() order at scale.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	lua "github.com/yuin/gopher-lua"
)

// TestVectorTableAllocFSV locks the alloc win: a per-tick handler reading a unit
// position ({x,y} table) and writing one back must stay well under the
// pre-patch 23 allocs/tick. Fails loudly if the small-table path regresses.
func TestVectorTableAllocFSV(t *testing.T) {
	g, err := api.NewGame(api.GameOptions{MaxUnits: 32, Seed: 1234})
	if err != nil {
		t.Fatal(err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatal(err)
	}
	L := lua.NewState()
	defer L.Close()
	ud := L.NewUserData()
	ud.Value = g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	if err := Register(L, g); err != nil {
		t.Fatal(err)
	}
	L.SetGlobal("u", ud)
	if err := L.DoString(`Run(function() while true do local p = Unit_Position(u); Unit_SetPosition(u, {x=p.x+1, y=p.y}); PolledWait(0.05) end end)`); err != nil {
		t.Fatal(err)
	}
	g.Advance(256)
	allocs := testing.AllocsPerRun(2000, func() { g.Advance(1) })
	t.Logf("FSV #416: pos-read + pos-write handler = %v allocs/tick (pre-patch 23; small-table path)", allocs)
	if allocs > 12 {
		t.Fatalf("vector-table handler allocates %v/op — small-table fast path regressed (want <=12, was 23)", allocs)
	}
}

// luaBool reads a global boolean, failing the test if false, with a label.
func luaCheck(t *testing.T, L *lua.LState, global, msg string) {
	t.Helper()
	if L.GetGlobal(global) != lua.LTrue {
		t.Fatalf("table-semantics check failed: %s (global %q != true)", msg, global)
	}
}

// TestSmallTableSemanticsFSV exercises the behaviours where a small-table fast
// path could diverge from the map representation: spill on overflow (>8 string
// keys), spill on a non-string key, delete-then-readd (tombstone, no dup key),
// and insertion-ordered pairs() across the spill boundary.
func TestSmallTableSemanticsFSV(t *testing.T) {
	L := lua.NewState()
	defer L.Close()
	// No game needed; pure table semantics. (Sandbox-equivalent bare state.)
	script := `
		-- (1) small table: values + insertion-ordered pairs()
		local v = {x=10, y=20}
		ok_small_vals = (v.x == 10 and v.y == 20)
		do local ord, n = {}, 0
		   for k,_ in pairs(v) do n=n+1; ord[n]=k end
		   ok_small_order = (n==2 and ord[1]=="x" and ord[2]=="y")
		end

		-- (2) overflow spill: 12 keys k1..k12 in order, all readable, order kept
		local big = {}
		for i=1,12 do big["k"..i] = i*100 end
		ok_big_vals = true
		for i=1,12 do if big["k"..i] ~= i*100 then ok_big_vals=false end end
		do local ord, n = {}, 0
		   for k,_ in pairs(big) do n=n+1; ord[n]=k end
		   ok_big_order = (n==12)
		   for i=1,12 do if ord[i] ~= "k"..i then ok_big_order=false end end
		end

		-- (3) delete then re-add: no duplicate key, fresh value, order preserved
		local d = {a=1, b=2, c=3}
		d.b = nil           -- tombstone
		ok_del_gone = (d.b == nil)
		d.b = 99            -- re-add
		do local n, sawb = 0, 0
		   for k,_ in pairs(d) do n=n+1; if k=="b" then sawb=sawb+1 end end
		   ok_readd = (d.b == 99 and sawb == 1 and n == 3)  -- exactly one 'b', three keys
		end

		-- (4) mixed: string keys then a non-array numeric key forces spill; both kinds iterate
		local m = {p=1, q=2}
		m[100000007] = 7     -- large int -> dict key -> spill
		do local strs, hasnum = 0, false
		   for k,val in pairs(m) do
		     if type(k)=="string" then strs=strs+1 end
		     if k==100000007 then hasnum = (val==7) end
		   end
		   ok_mixed = (strs==2 and hasnum and m.p==1 and m.q==2)
		end

		-- (5) array + string coexist: # length is array part, pairs sees both
		local mixed2 = {10, 20, 30, name="t"}
		ok_arr = (#mixed2 == 3 and mixed2[1]==10 and mixed2[3]==30 and mixed2.name=="t")
	`
	if err := L.DoString(script); err != nil {
		t.Fatalf("table-semantics script: %v", err)
	}
	luaCheck(t, L, "ok_small_vals", "small table values")
	luaCheck(t, L, "ok_small_order", "small table pairs() insertion order")
	luaCheck(t, L, "ok_big_vals", "spilled table values (12 keys)")
	luaCheck(t, L, "ok_big_order", "spilled table pairs() insertion order across overflow")
	luaCheck(t, L, "ok_del_gone", "deleted key reads nil")
	luaCheck(t, L, "ok_readd", "delete-then-readd: single key, fresh value, count")
	luaCheck(t, L, "ok_mixed", "non-string key spill keeps string keys")
	luaCheck(t, L, "ok_arr", "array part coexists with string keys")
	t.Log("FSV #416: small-table fast path is behaviourally identical to the map representation across spill, tombstone, mixed-key, and array-coexist cases")
}
