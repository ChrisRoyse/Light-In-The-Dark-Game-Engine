package luabind

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

// #573 — Lua KV binding. SoT = values read back via GetKV (typed) across
// scopes + inference.

func TestLuaKVInferenceAndScopes(t *testing.T) {
	g, L, _ := newScriptGame(t)
	_ = g
	if err := L.DoString(`
		p = Game_Player(0)
		ut = Game_UnitType("hfoo")
		u = Game_CreateUnit(p, ut, {x=10,y=10}, 0)
		other = Game_CreateUnit(p, ut, {x=20,y=20}, 0)

		SetKV(u, "count", 30)          -- KVInt
		SetKV(u, "angle", 1.5)         -- KVFixed
		SetKV(u, "name", "weapon")     -- KVString
		SetKV(u, "boss", true)         -- KVBool
		SetKV(u, "home", other)        -- KVEntity

		gotCount = GetKV(u, "count")
		gotAngle = GetKV(u, "angle")
		gotName  = GetKV(u, "name")
		gotBoss  = GetKV(u, "boss")
		gotHomeIsOther = (GetKV(u, "home") == other)
		hasCount = HasKV(u, "count")
		DeleteKV(u, "count")
		hasAfter = HasKV(u, "count")

		SetGlobalKV("phase", 2)
		gotPhase = GetGlobalKV("phase")
		SetPlayerKV(p, "score", 1500)
		gotScore = GetPlayerKV(p, "score")
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	num := func(name string) float64 { return float64(lua.LVAsNumber(L.GetGlobal(name))) }
	if num("gotCount") != 30 {
		t.Fatalf("count = %v, want 30", num("gotCount"))
	}
	if num("gotAngle") != 1.5 {
		t.Fatalf("angle = %v, want 1.5", num("gotAngle"))
	}
	if s := L.GetGlobal("gotName").String(); s != "weapon" {
		t.Fatalf("name = %q", s)
	}
	if L.GetGlobal("gotBoss") != lua.LTrue {
		t.Fatal("boss not true")
	}
	if L.GetGlobal("gotHomeIsOther") != lua.LTrue {
		t.Fatal("entity ref not round-tripped by identity")
	}
	if L.GetGlobal("hasCount") != lua.LTrue || L.GetGlobal("hasAfter") != lua.LFalse {
		t.Fatal("HasKV/DeleteKV wrong")
	}
	if num("gotPhase") != 2 {
		t.Fatalf("global phase = %v", num("gotPhase"))
	}
	if num("gotScore") != 1500 {
		t.Fatalf("player score = %v", num("gotScore"))
	}
}

func TestLuaKVGetAbsentNil(t *testing.T) {
	g, L, _ := newScriptGame(t)
	_ = g
	if err := L.DoString(`
		p = Game_Player(0)
		u = Game_CreateUnit(p, Game_UnitType("hfoo"), {x=1,y=1}, 0)
		missing = GetKV(u, "never")
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	if L.GetGlobal("missing") != lua.LNil {
		t.Fatal("GetKV of absent key not nil")
	}
}
