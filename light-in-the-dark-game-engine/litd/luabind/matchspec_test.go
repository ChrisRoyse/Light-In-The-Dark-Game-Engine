package luabind

// FSV for the Game_MatchSpec Lua exposure (#638 host half). SoT = the Lua table
// Game_MatchSpec() returns, read back field-by-field from the LState and compared
// to the MatchSpec the host registered. Plus the fail-closed path: no spec
// registered → calling the verb raises.

import (
	"testing"

	matchpkg "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/match"
	lua "github.com/yuin/gopher-lua"
)

func TestGameMatchSpecExposureFSV(t *testing.T) {
	const specTOML = `
seed = 7
victory = "score"
time_limit_ticks = 24000
[[players]]
slot = 0
race = "vigil"
controller = "cpu"
difficulty = "insane"
ai_strategy = "data/ai/vigil.toml"
[[players]]
slot = 1
race = "unbound"
controller = "user"
`
	spec, err := matchpkg.LoadMatchSpec([]byte(specTOML))
	if err != nil {
		t.Fatalf("LoadMatchSpec: %v", err)
	}

	L := lua.NewState()
	defer L.Close()
	RegisterMatchSpec(L, spec)

	// Drive the verb from Lua and pull every field back into Lua globals.
	if err := L.DoString(`
		local m = Game_MatchSpec()
		seed = m.seed
		victory = m.victory
		tlt = m.time_limit_ticks
		nplayers = #m.players
		p0slot = m.players[1].slot
		p0race = m.players[1].race
		p0ctrl = m.players[1].controller
		p0diff = m.players[1].difficulty
		p0ai = m.players[1].ai_strategy
		p1race = m.players[2].race
		p1ctrl = m.players[2].controller
	`); err != nil {
		t.Fatalf("Game_MatchSpec() raised: %v", err)
	}

	num := func(g string) float64 { return float64(lua.LVAsNumber(L.GetGlobal(g))) }
	str := func(g string) string { return lua.LVAsString(L.GetGlobal(g)) }
	t.Logf("FSV Game_MatchSpec table: seed=%v victory=%s tlt=%v nplayers=%v p0={%s,%s,diff=%v,%s} p1={%s,%s}",
		num("seed"), str("victory"), num("tlt"), num("nplayers"),
		str("p0race"), str("p0ctrl"), num("p0diff"), str("p0ai"), str("p1race"), str("p1ctrl"))

	if num("seed") != 7 || str("victory") != "score" || num("tlt") != 24000 || num("nplayers") != 2 {
		t.Fatalf("header mismatch: seed=%v victory=%s tlt=%v n=%v", num("seed"), str("victory"), num("tlt"), num("nplayers"))
	}
	if num("p0slot") != 0 || str("p0race") != "vigil" || str("p0ctrl") != "cpu" || num("p0diff") != 2 || str("p0ai") != "data/ai/vigil.toml" {
		t.Fatalf("player[0] mismatch: slot=%v race=%s ctrl=%s diff=%v ai=%s", num("p0slot"), str("p0race"), str("p0ctrl"), num("p0diff"), str("p0ai"))
	}
	if str("p1race") != "unbound" || str("p1ctrl") != "user" {
		t.Fatalf("player[1] mismatch: race=%s ctrl=%s", str("p1race"), str("p1ctrl"))
	}
}

func TestGameMatchSpecFailClosedWhenAbsent(t *testing.T) {
	L := lua.NewState()
	defer L.Close()
	RegisterMatchSpec(L, nil) // no match.toml loaded
	err := L.DoString(`Game_MatchSpec()`)
	t.Logf("FSV absent-spec: err=%v", err)
	if err == nil {
		t.Fatal("Game_MatchSpec() with no spec did NOT raise — must fail closed")
	}
}
