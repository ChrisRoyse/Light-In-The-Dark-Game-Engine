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
	L.Push(pushHandle(L, b.g.UnitType(L.CheckString(1))))
	return 1
}

// bindGameItemType binds Game.ItemType(code string) ItemType.
func (b gameBinder) bindGameItemType(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.ItemType(L.CheckString(1))))
	return 1
}

// bindGameBuffType binds Game.BuffType(code string) BuffType.
func (b gameBinder) bindGameBuffType(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.BuffType(L.CheckString(1))))
	return 1
}

// bindGameResourceNodeType binds Game.ResourceNodeType(code string)
// ResourceNodeType (#401) — resolves a node code a loaded world spawns.
func (b gameBinder) bindGameResourceNodeType(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.ResourceNodeType(L.CheckString(1))))
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
	L.Push(pushHandle(L, b.g.CreateResourceNode(typ, argVec2(L, 2))))
	return 1
}

// bindGameOrder binds Game.Order(name string) Order (#267) — the Game-bound
// resolver a script uses to obtain an order verb (no ambient globals), the
// analogue of Game_UnitType for the order catalog. Pairs with the generated
// Unit_Order (which now marshals its OrderTarget arg).
func (b gameBinder) bindGameOrder(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.Order(L.CheckString(1))))
	return 1
}

// bindGameCamera binds Game.Camera(p Player) Camera (#267). Game.Camera is a
// LitD new-capability (no manifest entry, so the generator does not emit it),
// but the generated Camera_* method verbs (Field/SetField/Follow/...) are
// unreachable without it — a script needs this resolver to obtain the per-player
// Camera handle. Same hand-written shape as the type resolvers above.
func (b gameBinder) bindGameCamera(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.Camera(argPlayer(L, 1))))
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
	L.Push(pushHandle(L, helpers.RandomItemType(b.g, argStringSlice(L, 1))))
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

// bindGameAllUnits binds Game.AllUnits(nil) []Unit (#267): every live unit. The
// generated dispatch defers AllUnits because its UnitFilter argument is a Lua
// predicate (callback-gated, #265); this catalog binding exposes the non-gated
// nil-filter (return-all) variant — a script filters the returned array in Lua.
func (b gameBinder) bindGameAllUnits(L *lua.LState) int {
	L.Push(handleSliceToLua(L, b.g.AllUnits(nil)))
	return 1
}

// bindGamePlayers binds Game.Players(nil) []Player (#267): all players. Same
// nil-filter rationale as bindGameAllUnits.
func (b gameBinder) bindGamePlayers(L *lua.LState) int {
	L.Push(handleSliceToLua(L, b.g.Players(nil)))
	return 1
}

// bindGameUnitsInRange binds Game.UnitsInRange(pos, r, nil) []Unit (#267): units
// within r of pos. nil-filter variant (see bindGameAllUnits).
func (b gameBinder) bindGameUnitsInRange(L *lua.LState) int {
	L.Push(handleSliceToLua(L, b.g.UnitsInRange(argVec2(L, 1), float64(L.CheckNumber(2)), nil)))
	return 1
}

// bindGameUnitsIn binds Game.UnitsIn(rect, nil) []Unit (#267): units inside a
// rect. nil-filter variant (see bindGameAllUnits).
func (b gameBinder) bindGameUnitsIn(L *lua.LState) int {
	L.Push(handleSliceToLua(L, b.g.UnitsIn(argRect(L, 1), nil)))
	return 1
}

// bindGamePrint binds Game.Print(to []Player, msg string) (#267): emit a UIPrint
// message to the given players. The generated dispatch defers Print for its
// variadic ...PrintOption; this binding exposes the common no-options form (a
// script passes the player array + text). emits a UIMessageEvent observable via
// OnUI.
func (b gameBinder) bindGamePrint(L *lua.LState) int {
	b.g.Print(argPlayerSlice(L, 1), L.CheckString(2))
	return 0
}

// bindGameClearMessages binds Game.ClearMessages(to []Player) (#267): clear the
// given players' on-screen messages (emits a UIClear event).
func (b gameBinder) bindGameClearMessages(L *lua.LState) int {
	b.g.ClearMessages(argPlayerSlice(L, 1))
	return 0
}

// bindStorageGetInt binds Storage.GetInt(category, key string) (int, bool)
// (#267): the generated dispatch defers it for its two-value return, but Lua
// takes multiple returns natively — push the value then the found-flag. Pairs
// with the generated Storage_SetInt for a full Lua round-trip of saved ints.
func bindStorageGetInt(L *lua.LState) int {
	v, ok := argStorage(L, 1).GetInt(L.CheckString(2), L.CheckString(3))
	L.Push(lua.LNumber(v))
	L.Push(lua.LBool(ok))
	return 2
}

// bindGameCreateDestructable binds Game.CreateDestructable(o DestructableOptions)
// Destructable (#267): the generated dispatch defers it for its options-struct
// argument; this reads the named-field options table and pushes the handle.
func (b gameBinder) bindGameCreateDestructable(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.CreateDestructable(argDestructableOptions(L, 1))))
	return 1
}

// bindGameNewFogModifier binds Game.NewFogModifier(p, state, area) FogModifier
// (#267). The generated dispatch defers the whole FogModifier type: its
// constructor takes the api.Area interface (no generatable arg marshaler) and
// its return type is not in the generator's pushHandle set, so the method verbs
// have no arg reader either. The catalog binds the no-options form (created
// stopped — call FogModifier_Start) and the methods below. SoT: Game_FogStateAt.
func (b gameBinder) bindGameNewFogModifier(L *lua.LState) int {
	L.Push(pushHandle(L, b.g.NewFogModifier(argPlayer(L, 1), argFogState(L, 2), argArea(L, 3))))
	return 1
}

// bindGameSetFogState binds Game.SetFogState(p, state, area, sharedVision)
// (#267): stamp a fog state over an area immediately (no modifier lifetime).
func (b gameBinder) bindGameSetFogState(L *lua.LState) int {
	b.g.SetFogState(argPlayer(L, 1), argFogState(L, 2), argArea(L, 3), L.ToBool(4))
	return 0
}

// bindPlayerResult binds Player.Result() MatchResult (#200/#346) — the READ side
// of the match-result surface. Game_Victory/Defeat/EndMatch (write) are generated
// and the terminal events dispatch via OnEvent(EventVictory/EventDefeat), but
// Player.Result carries no manifest entry (a LitD convenience getter, like
// Game.Camera), so the generator never bound it — a script could stage results
// yet not read them, which #200's victory.lua / #201 match flow require. Returns
// the MatchResult enum as a number: Playing=0, Won=1, Lost=2, Left=3.
func bindPlayerResult(L *lua.LState) int {
	L.Push(lua.LNumber(float64(argPlayer(L, 1).Result())))
	return 1
}

func bindFogModifierStart(L *lua.LState) int   { argFogModifier(L, 1).Start(); return 0 }
func bindFogModifierStop(L *lua.LState) int    { argFogModifier(L, 1).Stop(); return 0 }
func bindFogModifierDestroy(L *lua.LState) int { argFogModifier(L, 1).Destroy(); return 0 }

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
	L.SetGlobal("Game_AllUnits", L.NewFunction(b.bindGameAllUnits))
	L.SetGlobal("Game_Players", L.NewFunction(b.bindGamePlayers))
	L.SetGlobal("Game_UnitsInRange", L.NewFunction(b.bindGameUnitsInRange))
	L.SetGlobal("Game_UnitsIn", L.NewFunction(b.bindGameUnitsIn))
	L.SetGlobal("Game_Print", L.NewFunction(b.bindGamePrint))
	L.SetGlobal("Game_ClearMessages", L.NewFunction(b.bindGameClearMessages))
	L.SetGlobal("Storage_GetInt", L.NewFunction(bindStorageGetInt))
	L.SetGlobal("Game_CreateDestructable", L.NewFunction(b.bindGameCreateDestructable))
	L.SetGlobal("Game_NewFogModifier", L.NewFunction(b.bindGameNewFogModifier))
	L.SetGlobal("Game_SetFogState", L.NewFunction(b.bindGameSetFogState))
	L.SetGlobal("FogModifier_Start", L.NewFunction(bindFogModifierStart))
	L.SetGlobal("FogModifier_Stop", L.NewFunction(bindFogModifierStop))
	L.SetGlobal("FogModifier_Destroy", L.NewFunction(bindFogModifierDestroy))
	L.SetGlobal("Player_Result", L.NewFunction(bindPlayerResult))
}
