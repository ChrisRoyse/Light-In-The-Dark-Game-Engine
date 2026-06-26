package terrain

import (
	litmapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/g3n/engine/math32"
)

// Cliff-level + ramp transition geometry (#75; terrain.md §2.1, §6.1–6.2).
//
// Cliff levels are discrete sim data on the square pathing grid (terrain.md §5:
// pathing cell = terrain cell / 4). Adjacent cells at different levels are
// separated by a near-vertical wall; cells flagged as ramps slope between two
// levels. This file stitches that transition geometry from the cliff-level
// array — the renderer only draws what the sim data dictates.
//
// Two complementary pieces:
//   - Walls: for every shared edge between two cells of different level, a
//     vertical quad spans the height gap. Perpendicular walls meeting at a
//     corner share the corner's vertical column, so inner/outer corners and
//     checkerboards come out watertight with no special-casing.
//   - A marching-squares case table classifies each pathing *vertex* (the 2×2
//     cell neighborhood, thresholded per level) into flat / wall / outer-corner
//     / inner-corner / saddle. The classification drives art-piece selection
//     and is the coverage the FSV checks; the wall geometry above realizes it.
//   - Ramps: a ramp cell's top is a slope from its low level to its high level
//     across the ramp run, grid-aligned, replacing the wall on that edge.
//
// Heights: cliff level L sits at world Y = L*CliffLevelHeight, matching the
// terrace heights the map's height array encodes (level 0 → 0, level 1 → 512
// in the reference test map). Pure float32, no map iteration, no RNG: same
// field → bit-identical mesh, headless or rendered.

// CliffLevelHeight is the world-Y rise per cliff level (terrain.md §2.1;
// grounded in the reference map's height terraces 0 and 512).
const CliffLevelHeight float32 = 512

// PathCellSize is the world size of one pathing cell (CellSize / PathingScale).
const PathCellSize = CellSize / litmapdata.PathingScale // 128 / 4 = 32

// CliffCase is the marching-squares classification of a pathing vertex at one
// level threshold.
type CliffCase uint8

const (
	CaseFlat        CliffCase = iota // all 4 cells on the same side — no transition
	CaseWall                         // two adjacent cells high — a straight wall
	CaseOuterCorner                  // one cell high — convex corner
	CaseInnerCorner                  // three cells high — concave corner
	CaseSaddle                       // two diagonal cells high — ambiguous saddle
	cliffCaseCount
)

// CliffField is the read-only cliff data the generator consumes. *litmapdata.Map
// satisfies it; tests pass synthetic fields with known transition cases.
type CliffField interface {
	PathDims() (w, h int)
	// LevelAt returns the cliff level and ramp flag at pathing cell (x,y).
	// ok is false out of bounds (treated as the off-map void, below all levels).
	LevelAt(x, y int) (level int, ramp bool, ok bool)
}

// MapCliffField adapts a loaded map to CliffField.
type MapCliffField struct{ m *litmapdata.Map }

// NewMapCliffField wraps m. Returns nil for a nil map.
func NewMapCliffField(m *litmapdata.Map) *MapCliffField {
	if m == nil {
		return nil
	}
	return &MapCliffField{m: m}
}

func (f *MapCliffField) PathDims() (int, int) { return f.m.PathingWidth, f.m.PathingHeight }

func (f *MapCliffField) LevelAt(x, y int) (int, bool, bool) {
	c, ok := f.m.CliffAt(x, y)
	if !ok {
		return 0, false, false
	}
	return int(c.Level), c.Ramp, true
}

// CliffMesh is the generated transition geometry plus per-case coverage counts.
type CliffMesh struct {
	Positions []math32.Vector3
	Indices   []uint32
	// CaseCounts[c] is how many pathing vertices classified as case c (across
	// all level thresholds). Coverage proof for the FSV.
	CaseCounts [cliffCaseCount]int
	halfW      float32
	halfH      float32
}

// level returns the cliff level at (x,y), or -1 for the off-map void so that
// edges along the map border still raise a wall down to the void.
func levelOf(f CliffField, x, y int) (lvl int, ramp bool) {
	l, r, ok := f.LevelAt(x, y)
	if !ok {
		return -1, false
	}
	return l, r
}

// BuildCliffs generates the cliff/ramp transition mesh for f.
func BuildCliffs(f CliffField) *CliffMesh {
	if f == nil {
		return &CliffMesh{}
	}
	pw, ph := f.PathDims()
	out := &CliffMesh{
		halfW: float32(pw) * PathCellSize * 0.5,
		halfH: float32(ph) * PathCellSize * 0.5,
	}

	// Walls along +X and +Z shared edges between in-bounds cells of differing
	// level. Ramp edges are skipped here and handled by the ramp surface.
	for y := 0; y < ph; y++ {
		for x := 0; x < pw; x++ {
			la, ra := levelOf(f, x, y)
			if ra {
				out.addRamp(f, x, y, la)
				continue
			}
			// +X neighbor
			if x+1 < pw {
				lb, rb := levelOf(f, x+1, y)
				if !rb && lb != la {
					out.addWallEdgeX(x+1, y, la, lb)
				}
			}
			// +Z neighbor
			if y+1 < ph {
				lb, rb := levelOf(f, x, y+1)
				if !rb && lb != la {
					out.addWallEdgeZ(x, y+1, la, lb)
				}
			}
		}
	}

	out.classifyCases(f)
	return out
}

// worldX/worldZ map a pathing-grid line index to a world coordinate.
func (cm *CliffMesh) worldX(px int) float32 { return float32(px)*PathCellSize - cm.halfW }
func (cm *CliffMesh) worldZ(py int) float32 { return float32(py)*PathCellSize - cm.halfH }

// addWallEdgeX emits a vertical wall on the grid line x=edgeX between the cells
// to its left (level la) and right (level lb), spanning worldZ over row y.
func (cm *CliffMesh) addWallEdgeX(edgeX, y, la, lb int) {
	wx := cm.worldX(edgeX)
	z0, z1 := cm.worldZ(y), cm.worldZ(y+1)
	cm.addVerticalQuad(
		math32.Vector3{X: wx, Z: z0},
		math32.Vector3{X: wx, Z: z1},
		la, lb,
	)
}

// addWallEdgeZ emits a vertical wall on the grid line z=edgeZ between the cells
// above (level la) and below (level lb), spanning worldX over column x.
func (cm *CliffMesh) addWallEdgeZ(x, edgeZ, la, lb int) {
	wz := cm.worldZ(edgeZ)
	x0, x1 := cm.worldX(x), cm.worldX(x+1)
	cm.addVerticalQuad(
		math32.Vector3{X: x0, Z: wz},
		math32.Vector3{X: x1, Z: wz},
		la, lb,
	)
}

// addVerticalQuad adds a vertical wall quad along the segment a→b (XZ only),
// from the lower level's top to the higher level's top. The void (-1) sits at
// Y=0 so border walls still close. The two distinct XZ endpoints guarantee the
// quad is exactly vertical (FSV checks this).
func (cm *CliffMesh) addVerticalQuad(a, b math32.Vector3, la, lb int) {
	hi, lo := la, lb
	if lo > hi {
		hi, lo = lb, la
	}
	yTop := float32(hi) * CliffLevelHeight
	yBot := float32(maxInt(lo, 0)) * CliffLevelHeight
	base := uint32(len(cm.Positions))
	cm.Positions = append(cm.Positions,
		math32.Vector3{X: a.X, Y: yBot, Z: a.Z}, // 0 bottom-a
		math32.Vector3{X: b.X, Y: yBot, Z: b.Z}, // 1 bottom-b
		math32.Vector3{X: b.X, Y: yTop, Z: b.Z}, // 2 top-b
		math32.Vector3{X: a.X, Y: yTop, Z: a.Z}, // 3 top-a
	)
	cm.Indices = append(cm.Indices, base, base+1, base+2, base, base+2, base+3)
}

// addRamp emits a sloped top surface for a ramp cell at level..level+1. The
// slope runs along whichever axis has the lower/higher neighbor (the ramp run),
// grid-aligned to the pathing cell, so a unit ordered up it walks the slope.
func (cm *CliffMesh) addRamp(f CliffField, x, y, level int) {
	lo := float32(level) * CliffLevelHeight
	hi := float32(level+1) * CliffLevelHeight
	x0, x1 := cm.worldX(x), cm.worldX(x+1)
	z0, z1 := cm.worldZ(y), cm.worldZ(y+1)

	// Determine run direction: the high level is toward the neighbor at
	// level+1 (or higher). Default to +X if ambiguous, deterministically.
	highPosX := neighborLevel(f, x+1, y) >= level+1
	highNegX := neighborLevel(f, x-1, y) >= level+1
	highPosZ := neighborLevel(f, x, y+1) >= level+1
	highNegZ := neighborLevel(f, x, y-1) >= level+1

	// Corner heights (a=x0z0, b=x1z0, c=x1z1, d=x0z1): the edge toward the
	// higher neighbor rises to hi, the opposite edge stays at lo.
	var ya, yb, yc, yd = lo, lo, lo, lo
	switch {
	case highPosX || highNegX:
		// Slope along X. High edge at x1 if +X is high, else x0.
		if highPosX {
			yb, yc = hi, hi // x1 corners high
		} else {
			ya, yd = hi, hi // x0 corners high
		}
	case highPosZ || highNegZ:
		// Slope along Z. High edge at z1 if +Z is high, else z0.
		if highPosZ {
			yd, yc = hi, hi // z1 corners high
		} else {
			ya, yb = hi, hi // z0 corners high
		}
	default:
		// No higher neighbor found: flat ramp tread at the low level.
	}

	base := uint32(len(cm.Positions))
	cm.Positions = append(cm.Positions,
		math32.Vector3{X: x0, Y: ya, Z: z0}, // a
		math32.Vector3{X: x1, Y: yb, Z: z0}, // b
		math32.Vector3{X: x1, Y: yc, Z: z1}, // c
		math32.Vector3{X: x0, Y: yd, Z: z1}, // d
	)
	cm.Indices = append(cm.Indices, base, base+1, base+2, base, base+2, base+3)
}

// neighborLevel returns the cliff level at (x,y), or -1 off-map.
func neighborLevel(f CliffField, x, y int) int {
	l, _, ok := f.LevelAt(x, y)
	if !ok {
		return -1
	}
	return l
}

// classifyCases runs the marching-squares case table over every pathing vertex
// for every level threshold present, accumulating coverage counts.
func (cm *CliffMesh) classifyCases(f CliffField) {
	pw, ph := f.PathDims()
	maxLevel := 0
	for y := 0; y < ph; y++ {
		for x := 0; x < pw; x++ {
			if l, _, ok := f.LevelAt(x, y); ok && l > maxLevel {
				maxLevel = l
			}
		}
	}
	// Vertices sit between cells: vertex (vx,vy) touches cells (vx-1..vx, vy-1..vy).
	for thr := 1; thr <= maxLevel; thr++ {
		for vy := 0; vy <= ph; vy++ {
			for vx := 0; vx <= pw; vx++ {
				// 4 cells around the vertex, in bit order TL,TR,BR,BL.
				tl := cellHigh(f, vx-1, vy-1, thr)
				tr := cellHigh(f, vx, vy-1, thr)
				br := cellHigh(f, vx, vy, thr)
				bl := cellHigh(f, vx-1, vy, thr)
				code := b2i(tl)<<3 | b2i(tr)<<2 | b2i(br)<<1 | b2i(bl)
				cm.CaseCounts[caseOf(code)]++
			}
		}
	}
}

func cellHigh(f CliffField, x, y, thr int) bool {
	l, _, ok := f.LevelAt(x, y)
	return ok && l >= thr
}

func b2i(b bool) int {
	if b {
		return 1
	}
	return 0
}

// caseOf maps a 4-bit high/low corner code to its marching-squares case.
func caseOf(code int) CliffCase {
	switch code {
	case 0b0000, 0b1111:
		return CaseFlat
	case 0b1010, 0b0101:
		return CaseSaddle
	case 0b1000, 0b0100, 0b0010, 0b0001:
		return CaseOuterCorner
	case 0b1110, 0b0111, 0b1011, 0b1101:
		return CaseInnerCorner
	default: // 0b1100,0b0110,0b0011,0b1001 — two adjacent
		return CaseWall
	}
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

// TriangleCount returns the number of triangles in the cliff mesh.
func (cm *CliffMesh) TriangleCount() int { return len(cm.Indices) / 3 }
