package shell

import (
	"errors"
	"fmt"
	"sort"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

type BrushOp string

const (
	BrushRaise BrushOp = "raise"
	BrushLower BrushOp = "lower"
	BrushLevel BrushOp = "level"
	BrushRamp  BrushOp = "ramp"
)

type RampDirection string

const (
	RampEast  RampDirection = "east"
	RampWest  RampDirection = "west"
	RampNorth RampDirection = "north"
	RampSouth RampDirection = "south"
)

type TerrainBrush struct {
	Op            BrushOp
	Size          int
	Strength      int
	LevelTarget   int
	RampDirection RampDirection
}

type TerrainBrushSnapshot struct {
	Op            BrushOp       `json:"op"`
	Size          int           `json:"size"`
	Strength      int           `json:"strength"`
	LevelTarget   int           `json:"levelTarget"`
	RampDirection RampDirection `json:"rampDirection"`
	Label         string        `json:"label"`
}

type TerrainStroke struct {
	app          *App
	brush        TerrainBrush
	points       [][2]int
	seen         map[[2]int]struct{}
	heightBefore map[brushCell]int
	cliffBefore  map[brushCell]sourceform.CliffCell
	closed       bool
}

type brushCell struct {
	x, y int
}

func DefaultTerrainBrush() TerrainBrush {
	return TerrainBrush{Op: BrushRaise, Size: 1, Strength: 1, LevelTarget: 0, RampDirection: RampEast}
}

func (a *App) BrushSnapshot() TerrainBrushSnapshot {
	b := a.ensureBrush()
	return TerrainBrushSnapshot{
		Op:            b.Op,
		Size:          b.Size,
		Strength:      b.Strength,
		LevelTarget:   b.LevelTarget,
		RampDirection: b.RampDirection,
		Label:         b.label(),
	}
}

func (a *App) SetTerrainBrush(op BrushOp) error {
	if err := validateBrushOp(op); err != nil {
		return err
	}
	a.brush = a.ensureBrush()
	a.brush.Op = op
	a.errText = ""
	a.status = fmt.Sprintf("Brush: %s", a.brush.label())
	return nil
}

func (a *App) SetBrushSize(size int) error {
	if size < 0 || size > 8 {
		return fmt.Errorf("editor brush: size %d outside 0..8", size)
	}
	a.brush = a.ensureBrush()
	a.brush.Size = size
	a.errText = ""
	a.status = fmt.Sprintf("Brush: %s", a.brush.label())
	return nil
}

func (a *App) SetBrushStrength(strength int) error {
	if strength <= 0 || strength > 64 {
		return fmt.Errorf("editor brush: strength %d outside 1..64", strength)
	}
	a.brush = a.ensureBrush()
	a.brush.Strength = strength
	a.errText = ""
	a.status = fmt.Sprintf("Brush: %s", a.brush.label())
	return nil
}

func (a *App) SetBrushLevelTarget(level int) {
	a.brush = a.ensureBrush()
	a.brush.LevelTarget = level
	a.errText = ""
	a.status = fmt.Sprintf("Brush: %s", a.brush.label())
}

func (a *App) SetBrushRampDirection(dir RampDirection) error {
	if _, _, err := rampDelta(dir); err != nil {
		return err
	}
	a.brush = a.ensureBrush()
	a.brush.RampDirection = dir
	a.errText = ""
	a.status = fmt.Sprintf("Brush: %s", a.brush.label())
	return nil
}

func (a *App) ApplyTerrainBrush(x, y int) error {
	return a.ApplyTerrainStroke([][2]int{{x, y}})
}

func (a *App) BeginTerrainStroke(x, y int) (*TerrainStroke, error) {
	if a.world == nil {
		return nil, errors.New("editor shell: no project loaded")
	}
	brush := a.ensureBrush()
	if err := validateBrushOp(brush.Op); err != nil {
		return nil, err
	}
	stroke := &TerrainStroke{
		app:          a,
		brush:        brush,
		seen:         map[[2]int]struct{}{},
		heightBefore: map[brushCell]int{},
		cliffBefore:  map[brushCell]sourceform.CliffCell{},
	}
	if err := stroke.AddPoint(x, y); err != nil {
		return nil, err
	}
	return stroke, nil
}

func (a *App) ApplyTerrainStroke(points [][2]int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	if len(points) == 0 {
		return fmt.Errorf("editor brush: stroke requires at least one point")
	}
	brush := a.ensureBrush()
	if err := validateBrushOp(brush.Op); err != nil {
		return err
	}
	var commands []Command
	var err error
	switch brush.Op {
	case BrushRaise, BrushLower, BrushLevel:
		commands, err = a.heightStrokeCommands(points, brush)
	case BrushRamp:
		if len(points) != 1 {
			return fmt.Errorf("editor brush: ramp stroke requires exactly one point")
		}
		x, y := points[0][0], points[0][1]
		commands, err = a.rampBrushCommands(x, y, brush)
	default:
		err = fmt.Errorf("editor brush: unknown op %q", brush.Op)
	}
	if err != nil {
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
	stroke, err := NewCompositeCommand(fmt.Sprintf("stroke:%s/points=%d/size=%d/strength=%d", brush.Op, len(points), brush.Size, brush.Strength), commands)
	if err != nil {
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
	if err := a.executeCommand(stroke); err != nil {
		a.status = err.Error()
		return err
	}
	if stroke.Noop() {
		a.status = fmt.Sprintf("Brush %s stroke: no cells changed", brush.Op)
		return nil
	}
	a.status = fmt.Sprintf("Brush %s stroke applied (%d point(s))", brush.Op, len(points))
	return nil
}

func (s *TerrainStroke) AddPoint(x, y int) error {
	if s == nil || s.app == nil {
		return errors.New("editor brush: nil live stroke")
	}
	if s.closed {
		return errors.New("editor brush: live stroke already ended")
	}
	point := [2]int{x, y}
	if _, ok := s.seen[point]; ok {
		return nil
	}
	if s.brush.Op == BrushRamp && len(s.points) > 0 {
		return nil
	}
	commands, err := s.app.terrainStrokePointCommands(x, y, s.brush)
	if err != nil {
		s.app.errText = err.Error()
		s.app.status = s.app.errText
		return err
	}
	live, err := NewCompositeCommand(fmt.Sprintf("live:%s/%d,%d", s.brush.Op, x, y), commands)
	if err != nil {
		s.app.errText = err.Error()
		s.app.status = s.app.errText
		return err
	}
	if err := live.Apply(s.app); err != nil {
		s.app.errText = err.Error()
		s.app.status = s.app.errText
		return err
	}
	s.rememberBefore(commands)
	s.points = append(s.points, point)
	s.seen[point] = struct{}{}
	s.app.errText = ""
	if live.Noop() {
		s.app.status = fmt.Sprintf("Brush %s stroke live (%d point(s), no cells changed)", s.brush.Op, len(s.points))
	} else {
		s.app.status = fmt.Sprintf("Brush %s stroke live (%d point(s))", s.brush.Op, len(s.points))
	}
	return nil
}

func (s *TerrainStroke) End() error {
	if s == nil || s.app == nil {
		return errors.New("editor brush: nil live stroke")
	}
	if s.closed {
		return errors.New("editor brush: live stroke already ended")
	}
	s.closed = true
	commands, err := s.finalCommands()
	if err != nil {
		s.app.errText = err.Error()
		s.app.status = s.app.errText
		return err
	}
	stroke, err := NewCompositeCommand(fmt.Sprintf("stroke:%s/points=%d/size=%d/strength=%d", s.brush.Op, len(s.points), s.brush.Size, s.brush.Strength), commands)
	if err != nil {
		s.app.errText = err.Error()
		s.app.status = s.app.errText
		return err
	}
	if err := s.app.recordAppliedCommand(stroke); err != nil {
		s.app.status = err.Error()
		return err
	}
	if stroke.Noop() {
		s.app.status = fmt.Sprintf("Brush %s stroke: no cells changed", s.brush.Op)
		return nil
	}
	s.app.status = fmt.Sprintf("Brush %s stroke applied (%d point(s))", s.brush.Op, len(s.points))
	return nil
}

func (s *TerrainStroke) rememberBefore(commands []Command) {
	for _, cmd := range commands {
		switch c := cmd.(type) {
		case gridCellCommand:
			if c.kind != sourceform.GridHeight {
				continue
			}
			k := brushCell{x: c.x, y: c.y}
			if _, ok := s.heightBefore[k]; !ok {
				s.heightBefore[k] = c.before
			}
		case cliffCellCommand:
			k := brushCell{x: c.x, y: c.y}
			if _, ok := s.cliffBefore[k]; !ok {
				s.cliffBefore[k] = c.before
			}
		}
	}
}

func (s *TerrainStroke) finalCommands() ([]Command, error) {
	commands := make([]Command, 0, len(s.heightBefore)+len(s.cliffBefore))
	for _, k := range sortedBrushCells(s.heightBefore) {
		after, err := gridCellValue(s.app.world, sourceform.GridHeight, k.x, k.y)
		if err != nil {
			return nil, err
		}
		commands = append(commands, gridCellCommand{kind: sourceform.GridHeight, x: k.x, y: k.y, before: s.heightBefore[k], after: after})
	}
	for _, k := range sortedBrushCells(s.cliffBefore) {
		after, err := cliffCellValue(s.app.world, k.x, k.y)
		if err != nil {
			return nil, err
		}
		commands = append(commands, cliffCellCommand{x: k.x, y: k.y, before: s.cliffBefore[k], after: after})
	}
	return commands, nil
}

func sortedBrushCells[V any](cells map[brushCell]V) []brushCell {
	keys := make([]brushCell, 0, len(cells))
	for k := range cells {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool {
		if keys[i].y != keys[j].y {
			return keys[i].y < keys[j].y
		}
		return keys[i].x < keys[j].x
	})
	return keys
}

func (a *App) terrainStrokePointCommands(x, y int, brush TerrainBrush) ([]Command, error) {
	switch brush.Op {
	case BrushRaise, BrushLower, BrushLevel:
		return a.heightStrokeCommands([][2]int{{x, y}}, brush)
	case BrushRamp:
		return a.rampBrushCommands(x, y, brush)
	default:
		return nil, fmt.Errorf("editor brush: unknown op %q", brush.Op)
	}
}

func (a *App) heightStrokeCommands(points [][2]int, brush TerrainBrush) ([]Command, error) {
	type key struct{ x, y int }
	before := map[key]int{}
	after := cloneIntGrid(a.world.Height)
	for _, center := range points {
		cells, err := brushFootprint(a.world, center[0], center[1], brush.Size)
		if err != nil {
			return nil, err
		}
		for _, p := range cells {
			k := key{x: p[0], y: p[1]}
			if _, ok := before[k]; !ok {
				before[k] = a.world.Height[p[1]][p[0]]
			}
			current := after[p[1]][p[0]]
			switch brush.Op {
			case BrushRaise:
				after[p[1]][p[0]] = current + brush.Strength
			case BrushLower:
				after[p[1]][p[0]] = current - brush.Strength
			case BrushLevel:
				after[p[1]][p[0]] = brush.LevelTarget
			default:
				return nil, fmt.Errorf("editor brush: %s is not a height brush", brush.Op)
			}
		}
	}
	keys := make([]key, 0, len(before))
	for k := range before {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool {
		if keys[i].y != keys[j].y {
			return keys[i].y < keys[j].y
		}
		return keys[i].x < keys[j].x
	})
	commands := make([]Command, 0, len(keys))
	for _, k := range keys {
		commands = append(commands, gridCellCommand{kind: sourceform.GridHeight, x: k.x, y: k.y, before: before[k], after: after[k.y][k.x]})
	}
	return commands, nil
}

func (a *App) rampBrushCommands(cx, cy int, brush TerrainBrush) ([]Command, error) {
	if brush.Strength != 1 {
		return nil, fmt.Errorf("editor brush: ramp strength %d would span non-adjacent cliff levels; want 1", brush.Strength)
	}
	dx, dy, err := rampDelta(brush.RampDirection)
	if err != nil {
		return nil, err
	}
	lowX, lowY := cx-dx, cy-dy
	highX, highY := cx+dx, cy+dy
	low, err := cliffCellValue(a.world, lowX, lowY)
	if err != nil {
		return nil, fmt.Errorf("editor brush: ramp low side: %w", err)
	}
	center, err := cliffCellValue(a.world, cx, cy)
	if err != nil {
		return nil, fmt.Errorf("editor brush: ramp center: %w", err)
	}
	high, err := cliffCellValue(a.world, highX, highY)
	if err != nil {
		return nil, fmt.Errorf("editor brush: ramp high side: %w", err)
	}
	highBefore := high
	if low.Ramp || high.Ramp {
		return nil, fmt.Errorf("editor brush: ramp endpoints must be plain cliff cells")
	}
	delta := high.Level - low.Level
	if delta == 0 {
		high.Level = low.Level + 1
	} else if delta != 1 {
		return nil, fmt.Errorf("editor brush: ramp endpoints differ by %d cliff levels; want 0 or 1", delta)
	}
	if low.Level < 0 || low.Level+1 > 126 {
		return nil, fmt.Errorf("editor brush: ramp base level %d outside valid 0..125", low.Level)
	}
	targets := []struct {
		x, y   int
		before sourceform.CliffCell
		after  sourceform.CliffCell
	}{
		{x: lowX, y: lowY, before: low, after: sourceform.CliffCell{Level: low.Level}},
		{x: cx, y: cy, before: center, after: sourceform.CliffCell{Level: low.Level, Ramp: true}},
		{x: highX, y: highY, before: highBefore, after: sourceform.CliffCell{Level: low.Level + 1}},
	}
	commands := make([]Command, 0, len(targets))
	for _, t := range targets {
		commands = append(commands, cliffCellCommand{x: t.x, y: t.y, before: t.before, after: t.after})
	}
	return commands, nil
}

func brushFootprint(w *sourceform.World, cx, cy, size int) ([][2]int, error) {
	if w == nil {
		return nil, errors.New("editor shell: no project loaded")
	}
	if size < 0 {
		return nil, fmt.Errorf("editor brush: size %d outside 0..8", size)
	}
	if _, err := gridCellValue(w, sourceform.GridHeight, cx, cy); err != nil {
		return nil, err
	}
	points := make([][2]int, 0, (size*2+1)*(size*2+1))
	limit := size * size
	for y := cy - size; y <= cy+size; y++ {
		for x := cx - size; x <= cx+size; x++ {
			dx, dy := x-cx, y-cy
			if dx*dx+dy*dy > limit {
				continue
			}
			if y < 0 || y >= w.Terrain.Height || x < 0 || x >= w.Terrain.Width {
				continue
			}
			points = append(points, [2]int{x, y})
		}
	}
	if len(points) == 0 {
		return nil, fmt.Errorf("editor brush: footprint at (%d,%d) touched no cells", cx, cy)
	}
	return points, nil
}

func (a *App) ensureBrush() TerrainBrush {
	b := a.brush
	if b.Op == "" {
		b = DefaultTerrainBrush()
	}
	return b
}

func validateBrushOp(op BrushOp) error {
	switch op {
	case BrushRaise, BrushLower, BrushLevel, BrushRamp:
		return nil
	default:
		return fmt.Errorf("editor brush: unknown op %q", op)
	}
}

func rampDelta(dir RampDirection) (int, int, error) {
	switch dir {
	case RampEast:
		return 1, 0, nil
	case RampWest:
		return -1, 0, nil
	case RampNorth:
		return 0, -1, nil
	case RampSouth:
		return 0, 1, nil
	default:
		return 0, 0, fmt.Errorf("editor brush: unknown ramp direction %q", dir)
	}
}

func (b TerrainBrush) label() string {
	parts := []string{string(b.Op), fmt.Sprintf("size %d", b.Size), fmt.Sprintf("strength %d", b.Strength)}
	if b.Op == BrushLevel {
		parts = append(parts, fmt.Sprintf("target %d", b.LevelTarget))
	}
	if b.Op == BrushRamp {
		parts = append(parts, string(b.RampDirection))
	}
	return strings.Join(parts, " / ")
}
