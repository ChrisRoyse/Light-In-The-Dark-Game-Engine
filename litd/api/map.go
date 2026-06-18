package litd

// map.go: the Go-side seam wiring a loaded skirmish map (litd/asset/mapdata, #173)
// into a Game (#410). The host loads + validates the map (mapdata.Load) and hands
// it to NewGame via GameOptions.Map; setup seeds the players' start locations from
// it, and these accessors expose the placements (pathing cells converted to world
// coordinates) to Go callers and — via the script-binding layer's RegisterMap —
// to Lua worlds, so a world places control points at the authored cells instead
// of hardcoded literals.
//
// litd/api may import litd/asset/mapdata (asset, not render — the sim-never-imports-
// render rule is untouched). The cell→world convention is the engine-wide one
// (sim.CellCenter / luabind.cellToWorld): a pathing cell is 32 world units and
// +16 centers it.

import (
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
)

// MapStart is a player start placement from the loaded map, in world coordinates.
type MapStart struct {
	Player uint8
	Pos    Vec2
}

// MapBeacon is a capturable control-point placement from the loaded map, in world
// coordinates. Owner is the initial controller: -1 (neutral) or a player index.
type MapBeacon struct {
	ID    uint32
	Pos   Vec2
	Owner int
}

// mapCellCenter converts a pathing-grid cell (x,y) to its world-unit center,
// matching sim.CellCenter and luabind.cellToWorld (cell = 32 units, +16 centers).
func mapCellCenter(x, y int) Vec2 {
	return Vec2{X: float64(x*32 + 16), Y: float64(y*32 + 16)}
}

// MapData returns the loaded map (GameOptions.Map), or nil if none was supplied.
// The script-binding layer reads it to expose Game_MapStarts/Game_MapBeacons to
// Lua worlds.
func (g *Game) MapData() *mapdata.Map {
	if g == nil {
		return nil
	}
	return g.mapData
}

// MapStarts returns the map's player start placements in world coordinates, or
// nil if no map is loaded.
func (g *Game) MapStarts() []MapStart {
	if g == nil || g.mapData == nil {
		return nil
	}
	src := g.mapData.Starts()
	out := make([]MapStart, len(src))
	for i, s := range src {
		out[i] = MapStart{Player: s.Player, Pos: mapCellCenter(s.X, s.Y)}
	}
	return out
}

// MapBeacons returns the map's control-point placements in world coordinates, or
// nil if no map is loaded.
func (g *Game) MapBeacons() []MapBeacon {
	if g == nil || g.mapData == nil {
		return nil
	}
	src := g.mapData.Beacons()
	out := make([]MapBeacon, len(src))
	for i, b := range src {
		out[i] = MapBeacon{ID: b.ID, Pos: mapCellCenter(b.X, b.Y), Owner: b.Owner}
	}
	return out
}
