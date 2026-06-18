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
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
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

// bindGameResourceNodeType binds Game.ResourceNodeType(code string)
// ResourceNodeType (#401) — resolves a node code a loaded world spawns.
func (b gameBinder) bindGameResourceNodeType(L *lua.LState) int {
	L.Push(handleToLua(L, b.g.ResourceNodeType(L.CheckString(1))))
	return 1
}

// bindGameCreateResourceNode binds Game.CreateResourceNode(typ ResourceNodeType,
// pos Vec2) Unit (#401), so a world's main.lua can place harvestable nodes. Like
// the resolvers it is a Go-API convenience, not a JASS native (WC3 places mines
// via map data), so it carries no manifest entry. A wrong arg-1 type raises.
func (b gameBinder) bindGameCreateResourceNode(L *lua.LState) int {
	typ, ok := L.CheckUserData(1).Value.(api.ResourceNodeType)
	if !ok {
		L.ArgError(1, "expected ResourceNodeType userdata (from Game_ResourceNodeType)")
	}
	L.Push(handleToLua(L, b.g.CreateResourceNode(typ, argVec2(L, 2))))
	return 1
}

// bindGameOrder binds Game.Order(name string) Order (#267) — the Game-bound
// resolver a script uses to obtain an order verb (no ambient globals), the
// analogue of Game_UnitType for the order catalog. Pairs with the generated
// Unit_Order (which now marshals its OrderTarget arg).
func (b gameBinder) bindGameOrder(L *lua.LState) int {
	L.Push(handleToLua(L, b.g.Order(L.CheckString(1))))
	return 1
}

// registerCatalog installs the hand-written catalog resolvers, bound to b.g.
// Called from Register alongside the generated game-bound surface.
func registerCatalog(L *lua.LState, b gameBinder) {
	L.SetGlobal("Game_UnitType", L.NewFunction(b.bindGameUnitType))
	L.SetGlobal("Game_ItemType", L.NewFunction(b.bindGameItemType))
	L.SetGlobal("Game_BuffType", L.NewFunction(b.bindGameBuffType))
	L.SetGlobal("Game_ResourceNodeType", L.NewFunction(b.bindGameResourceNodeType))
	L.SetGlobal("Game_CreateResourceNode", L.NewFunction(b.bindGameCreateResourceNode))
	L.SetGlobal("Game_Order", L.NewFunction(b.bindGameOrder))
}
