package luabind

// bindings_catalog.go — hand-written bindings for the Game type-catalog
// resolvers (#393). Game.UnitType/ItemType/BuffType turn a string code into a
// typed handle. They are Go-API conveniences, not JASS natives, so they carry
// no manifest entry and the -emit-lua generator does not emit them — yet a
// runtime-loaded world (#268) needs them to resolve the types it spawns
// (otherwise the host must inject every type as a global, the dev-sandbox
// `footman` stopgap). These follow the generated Game_* ABI exactly (free
// function, args from stack at 1, one api call, handle pushed as userdata) and
// are installed by registerCatalog from Register. This file is the hand-written
// reference shape for these resolvers, sibling to the generated dispatch — it
// never edits a generated file, so the byte-identical regen gate (#267) is
// unaffected.

import (
	lua "github.com/yuin/gopher-lua"
)

// bindGameUnitType binds Game.UnitType(code string) UnitType.
func (b gameBinder) bindGameUnitType(L *lua.LState) int {
	L.Push(handleToLua(L, b.g.UnitType(L.CheckString(1))))
	return 1
}

// bindGameItemType binds Game.ItemType(code string) ItemType.
func (b gameBinder) bindGameItemType(L *lua.LState) int {
	L.Push(handleToLua(L, b.g.ItemType(L.CheckString(1))))
	return 1
}

// bindGameBuffType binds Game.BuffType(code string) BuffType.
func (b gameBinder) bindGameBuffType(L *lua.LState) int {
	L.Push(handleToLua(L, b.g.BuffType(L.CheckString(1))))
	return 1
}

// registerCatalog installs the hand-written catalog resolvers, bound to b.g.
// Called from Register alongside the generated game-bound surface.
func registerCatalog(L *lua.LState, b gameBinder) {
	L.SetGlobal("Game_UnitType", L.NewFunction(b.bindGameUnitType))
	L.SetGlobal("Game_ItemType", L.NewFunction(b.bindGameItemType))
	L.SetGlobal("Game_BuffType", L.NewFunction(b.bindGameBuffType))
}
