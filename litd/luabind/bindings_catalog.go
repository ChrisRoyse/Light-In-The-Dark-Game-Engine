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
	helpers "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api/helpers"
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

// bindGameCamera binds Game.Camera(p Player) Camera (#267). Game.Camera is a
// LitD new-capability (no manifest entry, so the generator does not emit it),
// but the generated Camera_* method verbs (Field/SetField/Follow/...) are
// unreachable without it — a script needs this resolver to obtain the per-player
// Camera handle. Same hand-written shape as the type resolvers above.
func (b gameBinder) bindGameCamera(L *lua.LState) int {
	L.Push(handleToLua(L, b.g.Camera(argPlayer(L, 1))))
	return 1
}

// bindStringHash binds the free function StringHash(s string) int32 (#267).
// A package-level func (no receiver, no game) — the generated dispatch defers
// free functions ("need a *Game / other types — later"), but the catalog seam
// binds them without a generator change or an api import in the generated file:
// the call is package-qualified here, in hand-written luabind code. StringHash
// is the deterministic WC3-style string hash a world script needs for stable
// data-table/gamecache keys; not reachable any other way from Lua.
func bindStringHash(L *lua.LState) int {
	L.Push(lua.LNumber(float64(api.StringHash(L.CheckString(1)))))
	return 1
}

// bindWeightedChoice binds helpers.WeightedChoice(g, weights []int) int (#267):
// a deterministic weighted index draw off the sim PRNG (g injected from b.g).
// The script passes the weights array; -1 means no positive-weight entry.
func (b gameBinder) bindWeightedChoice(L *lua.LState) int {
	L.Push(lua.LNumber(float64(helpers.WeightedChoice(b.g, argIntSlice(L, 1)))))
	return 1
}

// bindRandomItemType binds helpers.RandomItemType(g, codes []string) ItemType
// (#267): a deterministic random pick among the given item-type codes.
func (b gameBinder) bindRandomItemType(L *lua.LState) int {
	L.Push(handleToLua(L, helpers.RandomItemType(b.g, argStringSlice(L, 1))))
	return 1
}

// bindCreateUnits binds helpers.CreateUnits(g, n, owner, typ, pos, facing)
// []Unit (#267): bulk unit creation, returning the created units as a 1-based
// array. The game receiver is injected; the script passes the five params.
func (b gameBinder) bindCreateUnits(L *lua.LState) int {
	units := helpers.CreateUnits(b.g, L.CheckInt(1), argPlayer(L, 2), argUnitType(L, 3), argVec2(L, 4), argAngle(L, 5))
	L.Push(handleSliceToLua(L, units))
	return 1
}

// registerCatalog installs the hand-written catalog resolvers, bound to b.g.
// Called from Register alongside the generated game-bound surface.
func registerCatalog(L *lua.LState, b gameBinder) {
	L.SetGlobal("Game_Camera", L.NewFunction(b.bindGameCamera))
	L.SetGlobal("StringHash", L.NewFunction(bindStringHash))
	L.SetGlobal("WeightedChoice", L.NewFunction(b.bindWeightedChoice))
	L.SetGlobal("RandomItemType", L.NewFunction(b.bindRandomItemType))
	L.SetGlobal("CreateUnits", L.NewFunction(b.bindCreateUnits))
	L.SetGlobal("Game_UnitType", L.NewFunction(b.bindGameUnitType))
	L.SetGlobal("Game_ItemType", L.NewFunction(b.bindGameItemType))
	L.SetGlobal("Game_BuffType", L.NewFunction(b.bindGameBuffType))
	L.SetGlobal("Game_ResourceNodeType", L.NewFunction(b.bindGameResourceNodeType))
	L.SetGlobal("Game_CreateResourceNode", L.NewFunction(b.bindGameCreateResourceNode))
	L.SetGlobal("Game_Order", L.NewFunction(b.bindGameOrder))
}
