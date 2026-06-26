package luabind

// terrain.go — bakes a loaded skirmish map's terrain (pathing flags + cliff/ramp
// levels) into a sim pathing grid: the map→sim-pathing half of the map↔world
// wiring (#410), complementing map.go's map→Lua placement exposure. The render
// path already consumes a map's cliffs (render/terrain/cliffs.go); this is the
// matching primitive so the SIM honours the same high ground — a level-1 beacon
// plateau is reachable only across a ramp, exactly as #174's "high-ground
// beacons" require. The implementation lives in asset/mapgrid now so api.NewGame
// and the Lua test surface share one conversion path.

import (
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapgrid"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// GridFromMap returns a sim pathing grid baked from m. The map's
// PathingWidth×PathingHeight cells occupy the grid origin (0,0)-anchored; cells
// outside the map keep NewGrid's zero value (no Walkable bit ⇒ impassable
// border, fail-closed). Walkability and the cliff rule (path.AdjacencyLegal) then
// run unchanged on the result. Returns an error if the map is larger than the
// fixed sim grid (path.GridSize) rather than silently truncating.
func GridFromMap(m *mapdata.Map) (*path.Grid, error) {
	return mapgrid.GridFromMap(m)
}
