package luabind

// group_bindings.go — hand-written Lua bindings for the persistent
// unit-group surface (PRD2 02, #566). Mirrors JASS `group` ergonomics:
// NewGroup / GroupAdd / GroupEach / GroupFill* / set algebra / Destroy.
// A Group is pushed as userdata (interned like every other handle, so
// `a == b` compares by identity and a captured Group upvalue survives
// save/load — the group's CONTENTS are sim state, persisted by #565).
// Same hand-written shape as bindings_catalog.go: free function, args
// from the stack, one api call, handle/scalar pushed back.

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	lua "github.com/yuin/gopher-lua"
)

// argGroup reads argument i as a Group userdata.
func argGroup(L *lua.LState, i int) api.Group {
	g, ok := handleArg(L, i).(api.Group)
	if !ok {
		L.ArgError(i, "expected Group userdata (from NewGroup)")
	}
	return g
}

// argQuery parses an optional Lua options table into an api.Query. A nil
// or absent table is the permissive zero Query. Recognized keys mirror
// the spec: aliveOnly, enemyOf=<player>, allyOf=<player>,
// structures="only"|"exclude", flying="only"|"exclude", max=<n>.
func (b gameBinder) argQuery(L *lua.LState, i int) api.Query {
	var q api.Query
	t, ok := L.Get(i).(*lua.LTable)
	if !ok {
		return q
	}
	if v, ok := t.RawGetString("aliveOnly").(lua.LBool); ok {
		q.AliveOnly = bool(v)
	}
	if v, ok := t.RawGetString("enemyOf").(lua.LNumber); ok {
		q.Enemy = b.g.Player(int(v))
	}
	if v, ok := t.RawGetString("allyOf").(lua.LNumber); ok {
		q.Ally = b.g.Player(int(v))
	}
	q.Structures = triFromLua(t.RawGetString("structures"))
	q.Flying = triFromLua(t.RawGetString("flying"))
	if v, ok := t.RawGetString("max").(lua.LNumber); ok {
		q.Max = int(v)
	}
	return q
}

func triFromLua(v lua.LValue) api.TriState {
	s, ok := v.(lua.LString)
	if !ok {
		return api.TriAny
	}
	switch string(s) {
	case "only":
		return api.TriOnly
	case "exclude":
		return api.TriExclude
	}
	return api.TriAny
}

func (b gameBinder) bindNewGroup(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.NewGroup()))
	return 1
}

func bindDestroyGroup(L *lua.LState) int { argGroup(L, 1).Destroy(); return 0 }
func bindGroupAdd(L *lua.LState) int     { argGroup(L, 1).Add(argUnit(L, 2)); return 0 }
func bindGroupRemove(L *lua.LState) int  { argGroup(L, 1).Remove(argUnit(L, 2)); return 0 }
func bindGroupClear(L *lua.LState) int   { argGroup(L, 1).Clear(); return 0 }

func bindGroupRemoveOrdered(L *lua.LState) int {
	argGroup(L, 1).RemoveOrdered(argUnit(L, 2))
	return 0
}

func bindGroupCount(L *lua.LState) int {
	L.Push(lua.LNumber(argGroup(L, 1).Count()))
	return 1
}

func bindGroupContains(L *lua.LState) int {
	L.Push(lua.LBool(argGroup(L, 1).Contains(argUnit(L, 2))))
	return 1
}

func (b gameBinder) bindGroupFirst(L *lua.LState) int {
	L.Push(pushHandle(L, argGroup(L, 1).First()))
	return 1
}

// bindGroupEach calls the Lua function (arg 2) once per member, in
// insertion order, with the member Unit. Synchronous — no wait/yield is
// reachable, matching the ForGroup idiom; safe to GroupRemove the current
// unit inside the callback (the sim Each snapshots the count).
func (b gameBinder) bindGroupEach(L *lua.LState) int {
	gr := argGroup(L, 1)
	fn := L.CheckFunction(2)
	gr.Each(func(u api.Unit) {
		L.Push(fn)
		L.Push(pushHandle(L, u))
		L.Call(1, 0)
	})
	return 0
}

func bindGroupUnion(L *lua.LState) int {
	argGroup(L, 1).Union(argGroup(L, 2), argGroup(L, 3))
	return 0
}
func bindGroupIntersect(L *lua.LState) int {
	argGroup(L, 1).Intersect(argGroup(L, 2), argGroup(L, 3))
	return 0
}
func bindGroupDifference(L *lua.LState) int {
	argGroup(L, 1).Difference(argGroup(L, 2), argGroup(L, 3))
	return 0
}
func bindGroupCopyFrom(L *lua.LState) int {
	argGroup(L, 1).CopyFrom(argGroup(L, 2))
	return 0
}

func (b gameBinder) bindGroupFillRadius(L *lua.LState) int {
	n := argGroup(L, 1).FillRadius(argVec2(L, 2), float64(L.CheckNumber(3)), b.argQuery(L, 4))
	L.Push(lua.LNumber(n))
	return 1
}

func (b gameBinder) bindGroupFillRect(L *lua.LState) int {
	n := argGroup(L, 1).FillRect(argVec2(L, 2), argVec2(L, 3), b.argQuery(L, 4))
	L.Push(lua.LNumber(n))
	return 1
}

func (b gameBinder) bindGroupFillOwner(L *lua.LState) int {
	n := argGroup(L, 1).FillOwner(argPlayer(L, 2), b.argQuery(L, 3))
	L.Push(lua.LNumber(n))
	return 1
}

func (b gameBinder) bindGroupFillType(L *lua.LState) int {
	n := argGroup(L, 1).FillType(argUnitType(L, 2), b.argQuery(L, 3))
	L.Push(lua.LNumber(n))
	return 1
}

// registerGroups installs the group verbs as globals. Called from
// Register alongside registerCatalog.
func registerGroups(L *lua.LState, b gameBinder) {
	L.SetGlobal("NewGroup", L.NewFunction(b.bindNewGroup))
	L.SetGlobal("DestroyGroup", L.NewFunction(bindDestroyGroup))
	L.SetGlobal("GroupAdd", L.NewFunction(bindGroupAdd))
	L.SetGlobal("GroupRemove", L.NewFunction(bindGroupRemove))
	L.SetGlobal("GroupRemoveOrdered", L.NewFunction(bindGroupRemoveOrdered))
	L.SetGlobal("GroupClear", L.NewFunction(bindGroupClear))
	L.SetGlobal("GroupCount", L.NewFunction(bindGroupCount))
	L.SetGlobal("GroupContains", L.NewFunction(bindGroupContains))
	L.SetGlobal("GroupFirst", L.NewFunction(b.bindGroupFirst))
	L.SetGlobal("GroupEach", L.NewFunction(b.bindGroupEach))
	L.SetGlobal("GroupUnion", L.NewFunction(bindGroupUnion))
	L.SetGlobal("GroupIntersect", L.NewFunction(bindGroupIntersect))
	L.SetGlobal("GroupDifference", L.NewFunction(bindGroupDifference))
	L.SetGlobal("GroupCopyFrom", L.NewFunction(bindGroupCopyFrom))
	L.SetGlobal("GroupFillRadius", L.NewFunction(b.bindGroupFillRadius))
	L.SetGlobal("GroupFillRect", L.NewFunction(b.bindGroupFillRect))
	L.SetGlobal("GroupFillOwner", L.NewFunction(b.bindGroupFillOwner))
	L.SetGlobal("GroupFillType", L.NewFunction(b.bindGroupFillType))
}
