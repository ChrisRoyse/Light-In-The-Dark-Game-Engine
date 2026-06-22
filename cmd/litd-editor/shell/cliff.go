package shell

import (
	"fmt"
	"sort"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

const (
	maxCliffLevel              = 126
	editorTerrainCellWorldUnit = 4096
)

type CliffFlagKind string

const (
	CliffFlagRampInvalidated       CliffFlagKind = "ramp-invalidated"
	CliffFlagPlacementChangedCliff CliffFlagKind = "placement-on-changed-cliff"
)

type CliffFlagSnapshot struct {
	Kind       CliffFlagKind `json:"kind"`
	X          int           `json:"x"`
	Y          int           `json:"y"`
	EntityID   uint32        `json:"entityID,omitempty"`
	EntityType string        `json:"entityType,omitempty"`
	Before     string        `json:"before,omitempty"`
	After      string        `json:"after,omitempty"`
	Message    string        `json:"message"`
}

type terrainCellKey struct{ x, y int }

func (a *App) cliffStrokeCommands(points [][2]int, brush TerrainBrush) ([]Command, error) {
	before := map[terrainCellKey]sourceform.CliffCell{}
	changed := map[terrainCellKey]struct{}{}
	after := cloneCliffGrid(a.world.Cliff)
	flags := cloneCliffFlags(a.cliffFlags)

	for _, center := range points {
		cells, err := brushFootprint(a.world, center[0], center[1], brush.Size)
		if err != nil {
			return nil, err
		}
		for _, p := range cells {
			k := terrainCellKey{x: p[0], y: p[1]}
			if _, ok := before[k]; !ok {
				before[k] = a.world.Cliff[p[1]][p[0]]
			}
			current := after[p[1]][p[0]]
			next, invalidated, err := applyCliffBrushCell(current, brush)
			if err != nil {
				return nil, err
			}
			if invalidated {
				flags = appendCliffFlag(flags, CliffFlagSnapshot{
					Kind:    CliffFlagRampInvalidated,
					X:       p[0],
					Y:       p[1],
					Before:  cliffCellLabel(current),
					After:   cliffCellLabel(next),
					Message: fmt.Sprintf("ramp at %d,%d removed by cliff edit", p[0], p[1]),
				})
			}
			if current != next {
				after[p[1]][p[0]] = next
				changed[k] = struct{}{}
			}
		}
	}

	for _, f := range invalidateBrokenRamps(after, before, changed, a.world.Cliff) {
		flags = appendCliffFlag(flags, f)
	}
	flags = appendPlacementFlags(flags, a.world, changed)

	keys := make([]terrainCellKey, 0, len(changed))
	for k := range changed {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool {
		if keys[i].y != keys[j].y {
			return keys[i].y < keys[j].y
		}
		return keys[i].x < keys[j].x
	})

	commands := make([]Command, 0, len(keys)+1)
	for _, k := range keys {
		if _, ok := before[k]; !ok {
			before[k] = a.world.Cliff[k.y][k.x]
		}
		commands = append(commands, cliffCellCommand{x: k.x, y: k.y, before: before[k], after: after[k.y][k.x]})
	}
	commands = append(commands, cliffFlagsCommand{before: cloneCliffFlags(a.cliffFlags), after: flags})
	return commands, nil
}

func applyCliffBrushCell(current sourceform.CliffCell, brush TerrainBrush) (sourceform.CliffCell, bool, error) {
	next := current
	wasRamp := next.Ramp
	next.Ramp = false
	switch brush.Op {
	case BrushCliffRaise:
		next.Level = clampCliffLevel(next.Level + brush.Strength)
	case BrushCliffLower:
		next.Level = clampCliffLevel(next.Level - brush.Strength)
	case BrushCliffLevel:
		next.Level = clampCliffLevel(brush.LevelTarget)
	default:
		return sourceform.CliffCell{}, false, fmt.Errorf("editor brush: %s is not a cliff-level brush", brush.Op)
	}
	return next, wasRamp && current != next, nil
}

func invalidateBrokenRamps(grid [][]sourceform.CliffCell, before map[terrainCellKey]sourceform.CliffCell, changed map[terrainCellKey]struct{}, original [][]sourceform.CliffCell) []CliffFlagSnapshot {
	var flags []CliffFlagSnapshot
	for y, row := range grid {
		for x, cell := range row {
			if !cell.Ramp || cliffRampValidInGrid(grid, x, y) {
				continue
			}
			k := terrainCellKey{x: x, y: y}
			if _, ok := before[k]; !ok {
				before[k] = original[y][x]
			}
			next := cell
			next.Ramp = false
			grid[y][x] = next
			changed[k] = struct{}{}
			flags = append(flags, CliffFlagSnapshot{
				Kind:    CliffFlagRampInvalidated,
				X:       x,
				Y:       y,
				Before:  cliffCellLabel(cell),
				After:   cliffCellLabel(next),
				Message: fmt.Sprintf("ramp at %d,%d no longer touches both cliff levels", x, y),
			})
		}
	}
	return flags
}

func cliffRampValidInGrid(grid [][]sourceform.CliffCell, x, y int) bool {
	if y < 0 || y >= len(grid) || x < 0 || x >= len(grid[y]) {
		return false
	}
	cell := grid[y][x]
	if !cell.Ramp {
		return true
	}
	lo, hi := cell.Level, cell.Level+1
	hasLo, hasHi := false, false
	for _, d := range [][2]int{{1, 0}, {-1, 0}, {0, 1}, {0, -1}} {
		nx, ny := x+d[0], y+d[1]
		if ny < 0 || ny >= len(grid) || nx < 0 || nx >= len(grid[ny]) {
			continue
		}
		nlo, nhi := sourceformCliffSpan(grid[ny][nx])
		if nlo <= lo && lo <= nhi {
			hasLo = true
		}
		if nlo <= hi && hi <= nhi {
			hasHi = true
		}
	}
	return hasLo && hasHi
}

func sourceformCliffSpan(cell sourceform.CliffCell) (int, int) {
	lo, hi := cell.Level, cell.Level
	if cell.Ramp {
		hi++
	}
	return lo, hi
}

func appendPlacementFlags(flags []CliffFlagSnapshot, w *sourceform.World, changed map[terrainCellKey]struct{}) []CliffFlagSnapshot {
	if w == nil || len(changed) == 0 {
		return flags
	}
	for _, ent := range w.Entities {
		x, y, ok := entityTerrainCell(ent, w.Terrain.Width, w.Terrain.Height)
		if !ok {
			continue
		}
		if _, ok := changed[terrainCellKey{x: x, y: y}]; !ok {
			continue
		}
		flags = appendCliffFlag(flags, CliffFlagSnapshot{
			Kind:       CliffFlagPlacementChangedCliff,
			X:          x,
			Y:          y,
			EntityID:   ent.ID,
			EntityType: ent.Type,
			Message:    fmt.Sprintf("placement %d remains on changed cliff cell %d,%d", ent.ID, x, y),
		})
	}
	return flags
}

func entityTerrainCell(ent sourceform.Entity, width, height int) (int, int, bool) {
	if ent.Pos[0] < 0 || ent.Pos[1] < 0 {
		return 0, 0, false
	}
	x := ent.Pos[0] / editorTerrainCellWorldUnit
	y := ent.Pos[1] / editorTerrainCellWorldUnit
	if x < 0 || y < 0 || x >= width || y >= height {
		return 0, 0, false
	}
	return x, y, true
}

func appendCliffFlag(flags []CliffFlagSnapshot, flag CliffFlagSnapshot) []CliffFlagSnapshot {
	for i, existing := range flags {
		if sameCliffFlagIdentity(existing, flag) {
			flags[i] = flag
			return flags
		}
	}
	flags = append(flags, flag)
	sort.Slice(flags, func(i, j int) bool {
		if flags[i].Y != flags[j].Y {
			return flags[i].Y < flags[j].Y
		}
		if flags[i].X != flags[j].X {
			return flags[i].X < flags[j].X
		}
		if flags[i].Kind != flags[j].Kind {
			return flags[i].Kind < flags[j].Kind
		}
		return flags[i].EntityID < flags[j].EntityID
	})
	return flags
}

func sameCliffFlagIdentity(a, b CliffFlagSnapshot) bool {
	return a.Kind == b.Kind && a.X == b.X && a.Y == b.Y && a.EntityID == b.EntityID
}

func cloneCliffGrid(grid [][]sourceform.CliffCell) [][]sourceform.CliffCell {
	out := make([][]sourceform.CliffCell, len(grid))
	for y := range grid {
		out[y] = append([]sourceform.CliffCell(nil), grid[y]...)
	}
	return out
}

func cloneCliffFlags(flags []CliffFlagSnapshot) []CliffFlagSnapshot {
	return append([]CliffFlagSnapshot(nil), flags...)
}

func cliffFlagsEqual(a, b []CliffFlagSnapshot) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func clampCliffLevel(v int) int {
	if v < 0 {
		return 0
	}
	if v > maxCliffLevel {
		return maxCliffLevel
	}
	return v
}
