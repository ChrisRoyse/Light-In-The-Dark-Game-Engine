package luabind

import (
	"testing"

	lua "github.com/yuin/gopher-lua"
)

// #566 — Lua group binding. SoT = values read back from Lua (GroupCount,
// GroupContains) and a Go `mark` trace driven by GroupEach.

func TestLuaGroupMembershipAndEach(t *testing.T) {
	g, L, _ := newScriptGame(t)
	_ = g
	var marks int
	L.SetGlobal("mark", L.NewFunction(func(L *lua.LState) int { marks++; return 0 }))

	if err := L.DoString(`
		p = Game_Player(0)
		ut = Game_UnitType("hfoo")
		u1 = Game_CreateUnit(p, ut, {x=10, y=10}, 0)
		u2 = Game_CreateUnit(p, ut, {x=20, y=20}, 0)
		gr = NewGroup()
		GroupAdd(gr, u1)
		GroupAdd(gr, u2)
		GroupAdd(gr, u1)               -- dup, no-op
		count = GroupCount(gr)
		has1  = GroupContains(gr, u1)
		first = (GroupFirst(gr) == u1)
		GroupEach(gr, function(u) mark() end)
		GroupRemove(gr, u1)
		after = GroupCount(gr)
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	if n := int(lua.LVAsNumber(L.GetGlobal("count"))); n != 2 {
		t.Fatalf("GroupCount = %d, want 2", n)
	}
	if L.GetGlobal("has1") != lua.LTrue || L.GetGlobal("first") != lua.LTrue {
		t.Fatal("GroupContains/GroupFirst wrong in Lua")
	}
	if marks != 2 {
		t.Fatalf("GroupEach visited %d, want 2", marks)
	}
	if n := int(lua.LVAsNumber(L.GetGlobal("after"))); n != 1 {
		t.Fatalf("count after GroupRemove = %d, want 1", n)
	}
}

func TestLuaGroupFillRadiusAndAlgebra(t *testing.T) {
	g, L, _ := newScriptGame(t)
	_ = g
	if err := L.DoString(`
		p = Game_Player(0)
		ut = Game_UnitType("hfoo")
		a1 = Game_CreateUnit(p, ut, {x=10, y=10}, 0)
		a2 = Game_CreateUnit(p, ut, {x=12, y=12}, 0)
		far = Game_CreateUnit(p, ut, {x=5000, y=5000}, 0)

		near = NewGroup()
		hit = GroupFillRadius(near, {x=10, y=10}, 300, { aliveOnly = true })

		owned = NewGroup()
		ownedN = GroupFillOwner(owned, p, {})

		dst = NewGroup()
		GroupDifference(dst, owned, near)   -- everyone owned minus the near cluster = far
		diffN = GroupCount(dst)
		hasFar = GroupContains(dst, far)
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	if n := int(lua.LVAsNumber(L.GetGlobal("hit"))); n != 2 {
		t.Fatalf("GroupFillRadius hit %d, want 2 (a1,a2; far excluded)", n)
	}
	if n := int(lua.LVAsNumber(L.GetGlobal("ownedN"))); n != 3 {
		t.Fatalf("GroupFillOwner = %d, want 3", n)
	}
	if n := int(lua.LVAsNumber(L.GetGlobal("diffN"))); n != 1 || L.GetGlobal("hasFar") != lua.LTrue {
		t.Fatalf("difference = %d (hasFar=%v), want {far}", n, L.GetGlobal("hasFar"))
	}
}

// Group handles round-trip by identity (interned userdata), so a captured
// group equals a freshly fetched global.
func TestLuaGroupHandleIdentity(t *testing.T) {
	g, L, _ := newScriptGame(t)
	_ = g
	if err := L.DoString(`
		gr = NewGroup()
		same = (gr == gr)
		DestroyGroup(gr)
	`); err != nil {
		t.Fatalf("lua: %v", err)
	}
	if L.GetGlobal("same") != lua.LTrue {
		t.Fatal("group handle identity broken")
	}
}
