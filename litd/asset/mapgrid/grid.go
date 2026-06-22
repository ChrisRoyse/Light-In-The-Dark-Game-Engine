// Package mapgrid converts immutable mapdata terrain into the sim pathing grid.
package mapgrid

import (
	"fmt"

	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim/path"
)

// GridFromMap returns a sim pathing grid baked from m. The map's
// PathingWidth x PathingHeight cells occupy the grid origin; cells outside the
// map keep NewGrid's zero value (no Walkable bit, impassable border).
func GridFromMap(m *mapdata.Map) (*path.Grid, error) {
	if m == nil {
		return nil, fmt.Errorf("mapgrid: nil map")
	}
	if m.PathingWidth > path.GridSize || m.PathingHeight > path.GridSize {
		return nil, fmt.Errorf("mapgrid: map pathing %dx%d exceeds sim grid %d",
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
