package shell

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

type TerrainTool string

const (
	TerrainToolSculpt TerrainTool = "sculpt"
	TerrainToolCliff  TerrainTool = "cliff"
	TerrainToolPaint  TerrainTool = "paint"
)

type PaintBrush struct {
	Layer    int
	Size     int
	Strength int
}

type PaintBrushSnapshot struct {
	Layer    int                    `json:"layer"`
	Size     int                    `json:"size"`
	Strength int                    `json:"strength"`
	Label    string                 `json:"label"`
	Palette  []PaintLayerSnapshot   `json:"palette"`
	Weights  sourceform.SplatWeight `json:"weights"`
}

type PaintLayerSnapshot struct {
	Layer  int    `json:"layer"`
	Name   string `json:"name"`
	Color  string `json:"color"`
	Active bool   `json:"active"`
}

func DefaultPaintBrush() PaintBrush {
	return PaintBrush{Layer: 0, Size: 0, Strength: 255}
}

func (a *App) PaintSnapshot() PaintBrushSnapshot {
	p := a.ensurePaintBrush()
	weights, _ := sourceform.SplatWeightForLayer(p.Layer)
	return PaintBrushSnapshot{
		Layer:    p.Layer,
		Size:     p.Size,
		Strength: p.Strength,
		Label:    p.label(),
		Palette:  paintPalette(p.Layer),
		Weights:  weights,
	}
}

func (a *App) SetPaintLayer(layer int) error {
	if layer < 0 || layer > 3 {
		return fmt.Errorf("editor paint: layer %d outside 0..3", layer)
	}
	a.paint = a.ensurePaintBrush()
	a.paint.Layer = layer
	a.terrainTool = TerrainToolPaint
	a.errText = ""
	a.status = fmt.Sprintf("Paint: %s", a.paint.label())
	return nil
}

func (a *App) SetPaintStrength(strength int) error {
	if strength <= 0 || strength > 255 {
		return fmt.Errorf("editor paint: strength %d outside 1..255", strength)
	}
	a.paint = a.ensurePaintBrush()
	a.paint.Strength = strength
	a.terrainTool = TerrainToolPaint
	a.errText = ""
	a.status = fmt.Sprintf("Paint: %s", a.paint.label())
	return nil
}

func (a *App) SetPaintSize(size int) error {
	if size < 0 || size > 8 {
		return fmt.Errorf("editor paint: size %d outside 0..8", size)
	}
	a.paint = a.ensurePaintBrush()
	a.paint.Size = size
	a.terrainTool = TerrainToolPaint
	a.errText = ""
	a.status = fmt.Sprintf("Paint: %s", a.paint.label())
	return nil
}

func (a *App) ApplyPaintBrush(x, y int) error {
	return a.ApplyPaintStroke([][2]int{{x, y}})
}

func (a *App) ApplyPaintStroke(points [][2]int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	if len(points) == 0 {
		return fmt.Errorf("editor paint: stroke requires at least one point")
	}
	paint := a.ensurePaintBrush()
	commands, err := a.paintStrokeCommands(points, paint)
	if err != nil {
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
	stroke, err := NewCompositeCommand(fmt.Sprintf("paint:layer=%d/points=%d/size=%d/strength=%d", paint.Layer, len(points), paint.Size, paint.Strength), commands)
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
		a.status = fmt.Sprintf("Paint layer %d stroke: no cells changed", paint.Layer)
		return nil
	}
	a.terrainTool = TerrainToolPaint
	a.status = fmt.Sprintf("Paint layer %d stroke applied (%d point(s))", paint.Layer, len(points))
	return nil
}

func (a *App) paintStrokeCommands(points [][2]int, paint PaintBrush) ([]Command, error) {
	type key struct{ x, y int }
	before := map[key]sourceform.SplatWeight{}
	after := cloneSplatGrid(a.world.Splat)
	for _, center := range points {
		cells, err := brushFootprint(a.world, center[0], center[1], paint.Size)
		if err != nil {
			return nil, err
		}
		for _, p := range cells {
			k := key{x: p[0], y: p[1]}
			if _, ok := before[k]; !ok {
				before[k] = a.world.Splat[p[1]][p[0]]
			}
			next, err := paintSplatWeight(after[p[1]][p[0]], paint.Layer, paint.Strength)
			if err != nil {
				return nil, err
			}
			after[p[1]][p[0]] = next
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
		commands = append(commands, splatCellCommand{x: k.x, y: k.y, before: before[k], after: after[k.y][k.x]})
	}
	return commands, nil
}

func paintSplatWeight(before sourceform.SplatWeight, layer, strength int) (sourceform.SplatWeight, error) {
	if layer < 0 || layer > 3 {
		return sourceform.SplatWeight{}, fmt.Errorf("editor paint: layer %d outside 0..3", layer)
	}
	if strength <= 0 || strength > 255 {
		return sourceform.SplatWeight{}, fmt.Errorf("editor paint: strength %d outside 1..255", strength)
	}
	weights := []int{int(before.A), int(before.B), int(before.C), int(before.D)}
	remaining := 255 - strength
	sum := 0
	for i := range weights {
		weights[i] = weights[i] * remaining / 255
		sum += weights[i]
	}
	weights[layer] += strength
	sum += strength
	if delta := 255 - sum; delta != 0 {
		weights[layer] += delta
	}
	out := sourceform.SplatWeight{A: uint8(weights[0]), B: uint8(weights[1]), C: uint8(weights[2]), D: uint8(weights[3])}
	if int(out.A)+int(out.B)+int(out.C)+int(out.D) != 255 {
		return sourceform.SplatWeight{}, fmt.Errorf("editor paint: normalized weights %v do not sum to 255", weights)
	}
	return out, nil
}

func (a *App) SimRelevantHash() string {
	if a.world == nil {
		return ""
	}
	body, _ := json.Marshal(struct {
		Height   [][]int             `json:"height"`
		Cliff    [][]string          `json:"cliff"`
		Entities []sourceform.Entity `json:"entities"`
		Metadata sourceform.Metadata `json:"metadata"`
		Terrain  sourceform.Terrain  `json:"terrain"`
	}{
		Height:   a.world.Height,
		Cliff:    cliffGridStrings(a.world.Cliff),
		Entities: a.world.Entities,
		Metadata: a.world.Metadata,
		Terrain:  a.world.Terrain,
	})
	sum := sha256.Sum256(body)
	return hex.EncodeToString(sum[:])
}

func (a *App) ensurePaintBrush() PaintBrush {
	p := a.paint
	if p.Strength == 0 {
		p = DefaultPaintBrush()
	}
	return p
}

func (a *App) ensureTerrainTool() TerrainTool {
	switch a.terrainTool {
	case TerrainToolSculpt, TerrainToolCliff, TerrainToolPaint:
		return a.terrainTool
	default:
		return TerrainToolSculpt
	}
}

func paintPalette(active int) []PaintLayerSnapshot {
	colors := []string{"#4a8a53", "#8f7d4f", "#6d8791", "#9a5d4f"}
	out := make([]PaintLayerSnapshot, 4)
	for i := range out {
		out[i] = PaintLayerSnapshot{
			Layer:  i,
			Name:   fmt.Sprintf("layer %s", string(rune('A'+i))),
			Color:  colors[i],
			Active: i == active,
		}
	}
	return out
}

func (p PaintBrush) label() string {
	name := string(rune('A' + p.Layer))
	if p.Layer < 0 || p.Layer > 3 {
		name = "?"
	}
	return strings.Join([]string{fmt.Sprintf("layer %s", name), fmt.Sprintf("size %d", p.Size), fmt.Sprintf("strength %d", p.Strength)}, " / ")
}
