package luabind

// terrain.go — bakes a loaded skirmish map's terrain (pathing flags + cliff/ramp
// levels) into a sim pathing grid: the map→sim-pathing half of the map↔world
// wiring (#410), complementing map.go's map→Lua placement exposure. The render
// path already consumes a map's cliffs (render/terrain/cliffs.go); this is the
// matching primitive so the SIM honours the same high ground — a level-1 beacon
// plateau is reachable only across a ramp, exactly as #174's "high-ground
// beacons" require. Kept a pure constructor (no World mutation, no global state)
// so it can be unit-FSV'd in isolation; the live world-load call site is #410.

import (
	"fmt"

	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// GridFromMap returns a sim pathing grid baked from m. The map's
// PathingWidth×PathingHeight cells occupy the grid origin (0,0)-anchored; cells
// outside the map keep NewGrid's zero value (no Walkable bit ⇒ impassable
// border, fail-closed). Walkability and the cliff rule (path.AdjacencyLegal) then
// run unchanged on the result. Returns an error if the map is larger than the
// fixed sim grid (path.GridSize) rather than silently truncating.
func GridFromMap(m *mapdata.Map) (*path.Grid, error) {
	if m.PathingWidth > path.GridSize || m.PathingHeight > path.GridSize {
		return nil, fmt.Errorf("luabind: map pathing %dx%d exceeds sim grid %d",
			m.PathingWidth, m.PathingHeight, path.GridSize)
	}
	g := path.NewGrid()
	for y := 0; y < m.PathingHeight; y++ {
		for x := 0; x < m.PathingWidth; x++ {
			pf, _ := m.PathingAt(x, y)
			var f path.Flags
			if pf&mapdata.PathWalkable != 0 {
				f |= path.Walkable
			}
			if pf&mapdata.PathBuildable != 0 {
				f |= path.Buildable
			}
			// PathWater carries neither walk nor build (loader-enforced); its
			// cells stay impassable to ground units, which is the intent.
			g.SetFlags(int32(x), int32(y), f)

			c, _ := m.CliffAt(x, y)
			if c.Ramp {
				g.SetRamp(int32(x), int32(y), c.Level)
			} else {
				g.SetCliffLevel(int32(x), int32(y), c.Level)
			}
		}
	}
	return g, nil
}
