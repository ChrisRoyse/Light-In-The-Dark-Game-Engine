package luabind

// map.go — exposes a loaded skirmish map's placement data (#173 mapdata.Map) to
// Lua worlds (the Lua-exposure half of #410). The HOST loads + validates the map
// (mapdata.Load) and calls RegisterMap; Lua worlds only READ it via
// Game_MapBeacons()/Game_MapStarts() — scripts get no filesystem access, so this
// adds no sandbox surface. Placement cells (pathing-grid) are converted to world
// coordinates here (a pathing cell is 32 world units; +16 centers it — see
// sim.CellCenter / pathfinding.md §2) so a world can hand them straight to
// Game_UnitsInRange / Game_CreateUnit without knowing the grid scale.
//
// This is the binding primitive the full map↔world wiring (#410) will build on:
// once the world loader knows a world's map, it calls RegisterMap so e.g.
// worlds/firstflame/scripts/beacon.lua places a control point per Game_MapBeacons
// entry instead of hardcoding coordinates.

import (
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	lua "github.com/yuin/gopher-lua"
)

// cellToWorld converts a pathing-grid cell coordinate to its world-unit center.
func cellToWorld(cell int) float64 { return float64(cell*32 + 16) }

// RegisterMap installs Game_MapBeacons/Game_MapStarts on L, returning the placement
// data of m as world coordinates. Call after Register. If m is nil the verbs are
// still installed but raise a loud error when called (fail-closed: a world asking
// for map data when none is loaded is a bug, not an empty result).
func RegisterMap(L *lua.LState, m *mapdata.Map) {
	L.SetGlobal("Game_MapBeacons", L.NewFunction(func(L *lua.LState) int {
		if m == nil {
			L.RaiseError("Game_MapBeacons: no map loaded (host did not call RegisterMap)")
			return 0
		}
		beacons := m.Beacons()
		t := L.CreateTable(len(beacons), 0)
		for i, b := range beacons {
			e := L.CreateTable(0, 4)
			e.RawSetString("id", lua.LNumber(b.ID))
			e.RawSetString("x", lua.LNumber(cellToWorld(b.X)))
			e.RawSetString("y", lua.LNumber(cellToWorld(b.Y)))
			e.RawSetString("owner", lua.LNumber(b.Owner)) // -1 = neutral
			t.RawSetInt(i+1, e)
		}
		L.Push(t)
		return 1
	}))
	L.SetGlobal("Game_MapStarts", L.NewFunction(func(L *lua.LState) int {
		if m == nil {
			L.RaiseError("Game_MapStarts: no map loaded (host did not call RegisterMap)")
			return 0
		}
		starts := m.Starts()
		t := L.CreateTable(len(starts), 0)
		for i, s := range starts {
			e := L.CreateTable(0, 3)
			e.RawSetString("player", lua.LNumber(s.Player))
			e.RawSetString("x", lua.LNumber(cellToWorld(s.X)))
			e.RawSetString("y", lua.LNumber(cellToWorld(s.Y)))
			t.RawSetInt(i+1, e)
		}
		L.Push(t)
		return 1
	}))
}
