// Package shell contains the editor application state machine (#125). It is
// deliberately independent of sim/render internals: project IO goes through the
// source-form and archive packages, and user-visible text comes from locale
// tables.
package shell

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

type Mode string

const (
	ModeTerrain  Mode = "terrain"
	ModeObjects  Mode = "objects"
	ModeMetadata Mode = "metadata"
)

var allModes = []Mode{ModeTerrain, ModeObjects, ModeMetadata}

type App struct {
	table           *locale.Table
	world           *sourceform.World
	projectPath     string
	mode            Mode
	status          string
	errText         string
	confirm         *Confirm
	commands        *CommandStack
	brush           TerrainBrush
	terrainTool     TerrainTool
	paint           PaintBrush
	cliffFlags      []CliffFlagSnapshot
	objectPalette   []ObjectPaletteItem
	objectSelection ObjectSelection
}

type Confirm struct {
	Kind       string `json:"kind"`
	TargetPath string `json:"targetPath"`
	Title      string `json:"title"`
	Body       string `json:"body"`
}

type Snapshot struct {
	Title       string               `json:"title"`
	ProjectPath string               `json:"projectPath"`
	Mode        Mode                 `json:"mode"`
	ModeLabel   string               `json:"modeLabel"`
	Dirty       bool                 `json:"dirty"`
	DirtyLabel  string               `json:"dirtyLabel"`
	Status      string               `json:"status"`
	Error       string               `json:"error,omitempty"`
	Confirm     *Confirm             `json:"confirm,omitempty"`
	Labels      map[string]string    `json:"labels"`
	World       WorldSnapshot        `json:"world"`
	Stack       StackSnapshot        `json:"stack"`
	Brush       TerrainBrushSnapshot `json:"brush"`
	TerrainTool TerrainTool          `json:"terrainTool"`
	Paint       PaintBrushSnapshot   `json:"paint"`
	CliffFlags  []CliffFlagSnapshot  `json:"cliffFlags,omitempty"`
	Objects     ObjectSnapshot       `json:"objects"`
}

type WorldSnapshot struct {
	ID          string     `json:"id"`
	Name        string     `json:"name"`
	Width       int        `json:"width"`
	Height      int        `json:"height"`
	Entities    int        `json:"entities"`
	Doodads     int        `json:"doodads"`
	HeightCell  int        `json:"heightCell"`
	CliffCell   string     `json:"cliffCell"`
	SplatCell   string     `json:"splatCell"`
	SeedPolicy  string     `json:"seedPolicy"`
	EngineRange string     `json:"engineRange"`
	HeightRows  [][]int    `json:"heightRows,omitempty"`
	CliffRows   [][]string `json:"cliffRows,omitempty"`
	SplatRows   [][][]int  `json:"splatRows,omitempty"`
}

func New(table *locale.Table) *App {
	return &App{
		table:       table,
		mode:        ModeTerrain,
		status:      must(table, locale.EditorStatusReady),
		commands:    NewCommandStack(CommandStackLimit),
		brush:       DefaultTerrainBrush(),
		terrainTool: TerrainToolSculpt,
		paint:       DefaultPaintBrush(),
	}
}

func Modes() []Mode { return append([]Mode(nil), allModes...) }

func (a *App) NewProject(dir string) error {
	if a.world != nil && a.world.Dirty() {
		a.confirm = &Confirm{
			Kind:       "new-project",
			TargetPath: dir,
			Title:      must(a.table, locale.EditorConfirmNewTitle),
			Body:       must(a.table, locale.EditorConfirmNewBody),
		}
		a.errText = ""
		a.status = a.confirm.Title
		return nil
	}
	return a.createProject(dir)
}

func (a *App) ConfirmPending() error {
	if a.confirm == nil {
		return errors.New("editor shell: no confirmation pending")
	}
	c := *a.confirm
	a.confirm = nil
	switch c.Kind {
	case "new-project":
		return a.createProject(c.TargetPath)
	default:
		return fmt.Errorf("editor shell: unknown confirmation kind %q", c.Kind)
	}
}

func (a *App) CancelConfirm() {
	a.confirm = nil
	a.status = must(a.table, locale.EditorConfirmCancel)
}

func (a *App) createProject(dir string) error {
	if dir == "" {
		return errors.New("editor shell: new project requires a directory")
	}
	w := defaultWorld(must(a.table, locale.EditorProjectUntitled))
	if err := w.Save(dir); err != nil {
		a.errText = fmt.Sprintf("%s: %v", must(a.table, locale.EditorErrorOpen), err)
		return err
	}
	a.world = w
	a.projectPath = dir
	a.mode = ModeTerrain
	a.errText = ""
	a.confirm = nil
	a.cliffFlags = nil
	a.status = must(a.table, locale.EditorStatusProjectCreated)
	a.resetCommandStack()
	return nil
}

func (a *App) OpenProject(path string) error {
	if path == "" {
		return a.openError(path, errors.New("empty path"))
	}
	st, err := os.Stat(path)
	if err != nil {
		return a.openError(path, err)
	}
	if st.IsDir() {
		w, err := sourceform.Load(path)
		if err != nil {
			return a.openError(path, err)
		}
		a.world = w
		a.projectPath = path
		a.mode = ModeTerrain
		a.errText = ""
		a.confirm = nil
		a.cliffFlags = nil
		a.status = must(a.table, locale.EditorStatusProjectOpened)
		a.resetCommandStack()
		return nil
	}
	if filepath.Ext(path) == ".litdworld" {
		arc, err := worldarchive.Open(path, "")
		if err != nil {
			return a.openError(path, err)
		}
		arc.Close()
		return a.openError(path, errors.New("archive verified but is read-only in the editor shell; unpack to source form before editing"))
	}
	return a.openError(path, fmt.Errorf("unsupported project path %q", path))
}

func (a *App) openError(path string, err error) error {
	a.errText = fmt.Sprintf("%s: %s: %v", must(a.table, locale.EditorErrorOpen), path, err)
	a.status = a.errText
	return err
}

func (a *App) SwitchMode(mode Mode) error {
	switch mode {
	case ModeTerrain, ModeObjects, ModeMetadata:
		a.mode = mode
		a.errText = ""
		return nil
	default:
		return fmt.Errorf("editor shell: unknown mode %q", mode)
	}
}

func (a *App) EditTerrainHeight(x, y, value int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	before, err := gridCellValue(a.world, sourceform.GridHeight, x, y)
	if err != nil {
		return err
	}
	return a.executeCommand(gridCellCommand{kind: sourceform.GridHeight, x: x, y: y, before: before, after: value})
}

func (a *App) MoveEntity(id uint32, pos [2]int, facing int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	beforePos, beforeRotation, beforeScale, err := entityTransform(a.world, id)
	if err != nil {
		return err
	}
	return a.executeCommand(entityMoveCommand{id: id, beforePos: beforePos, beforeRotation: beforeRotation, beforeScale: beforeScale, afterPos: pos, afterRotation: facing, afterScale: beforeScale})
}

func (a *App) TransformEntity(id uint32, pos [2]int, rotation, scale int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	beforePos, beforeRotation, beforeScale, err := entityTransform(a.world, id)
	if err != nil {
		return err
	}
	scale = ClampPlacementScale(scale)
	return a.executeCommand(entityMoveCommand{id: id, beforePos: beforePos, beforeRotation: beforeRotation, beforeScale: beforeScale, afterPos: pos, afterRotation: rotation, afterScale: scale})
}

func (a *App) SetMetadataName(name string) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.executeCommand(metadataNameCommand{before: a.world.Metadata.Name, after: name})
}

func (a *App) Undo() error {
	return a.ensureCommandStack().Undo(a)
}

func (a *App) Redo() error {
	return a.ensureCommandStack().Redo(a)
}

func (a *App) StackSnapshot() StackSnapshot {
	return a.ensureCommandStack().Snapshot()
}

func (a *App) CliffStepLegal(ax, ay, bx, by int) (bool, error) {
	if a.world == nil {
		return false, errors.New("editor shell: no project loaded")
	}
	return a.world.CliffStepLegal(ax, ay, bx, by)
}

func (a *App) Save() error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	if err := a.world.Save(a.projectPath); err != nil {
		a.errText = err.Error()
		return err
	}
	a.status = must(a.table, locale.EditorDirtyClean)
	a.errText = ""
	return nil
}

func (a *App) Snapshot() Snapshot {
	labels := map[string]string{
		"new":               must(a.table, locale.EditorActionNew),
		"open":              must(a.table, locale.EditorActionOpen),
		"save":              must(a.table, locale.EditorActionSave),
		"export":            must(a.table, locale.EditorActionExport),
		"cancel":            must(a.table, locale.EditorConfirmCancel),
		"proceed":           must(a.table, locale.EditorConfirmProceed),
		"terrain":           must(a.table, locale.EditorModeTerrain),
		"objects":           must(a.table, locale.EditorModeObjects),
		"metadata":          must(a.table, locale.EditorModeMetadata),
		"panelTerrain":      must(a.table, locale.EditorPanelTerrain),
		"panelObjects":      must(a.table, locale.EditorPanelObjects),
		"panelMetadata":     must(a.table, locale.EditorPanelMetadata),
		"hintTerrain":       must(a.table, locale.EditorHintTerrain),
		"hintObjects":       must(a.table, locale.EditorHintObjects),
		"hintMetadata":      must(a.table, locale.EditorHintMetadata),
		"statusPrefix":      must(a.table, locale.EditorStatusPrefix),
		"fieldCell":         must(a.table, locale.EditorFieldCell),
		"fieldEntities":     must(a.table, locale.EditorFieldEntities),
		"fieldDoodads":      must(a.table, locale.EditorFieldDoodads),
		"fieldPalette":      must(a.table, locale.EditorFieldPalette),
		"fieldSelection":    must(a.table, locale.EditorFieldSelection),
		"fieldOverride":     must(a.table, locale.EditorFieldOverride),
		"fieldBrush":        must(a.table, locale.EditorFieldBrush),
		"fieldCliff":        must(a.table, locale.EditorFieldCliff),
		"fieldSplat":        must(a.table, locale.EditorFieldSplat),
		"fieldTool":         must(a.table, locale.EditorFieldTool),
		"fieldPaint":        must(a.table, locale.EditorFieldPaint),
		"fieldFlags":        must(a.table, locale.EditorFieldFlags),
		"fieldID":           must(a.table, locale.EditorFieldID),
		"fieldName":         must(a.table, locale.EditorFieldName),
		"fieldEngine":       must(a.table, locale.EditorFieldEngine),
		"fieldSeedPolicy":   must(a.table, locale.EditorFieldSeedPolicy),
		"fieldPath":         must(a.table, locale.EditorFieldPath),
		"scopeNoTriggerGUI": must(a.table, locale.EditorScopeNoTriggerGUI),
	}
	dirty := a.world != nil && a.world.Dirty()
	dirtyLabel := must(a.table, locale.EditorDirtyClean)
	if dirty {
		dirtyLabel = must(a.table, locale.EditorDirtyUnsaved)
	}
	titleName := must(a.table, locale.EditorProjectUntitled)
	if a.world != nil && a.world.Metadata.Name != "" {
		titleName = a.world.Metadata.Name
	}
	title := fmt.Sprintf("%s - %s", must(a.table, locale.EditorTitle), titleName)
	if dirty {
		title += " *"
	}
	s := Snapshot{
		Title:       title,
		ProjectPath: a.projectPath,
		Mode:        a.mode,
		ModeLabel:   must(a.table, locale.EditorModeLabel),
		Dirty:       dirty,
		DirtyLabel:  dirtyLabel,
		Status:      a.status,
		Error:       a.errText,
		Confirm:     a.confirm,
		Labels:      labels,
		Stack:       a.StackSnapshot(),
		Brush:       a.BrushSnapshot(),
		TerrainTool: a.ensureTerrainTool(),
		Paint:       a.PaintSnapshot(),
		CliffFlags:  cloneCliffFlags(a.cliffFlags),
		Objects:     a.ObjectSnapshot(),
	}
	if a.world != nil {
		s.World = WorldSnapshot{
			ID:          a.world.Metadata.ID,
			Name:        a.world.Metadata.Name,
			Width:       a.world.Terrain.Width,
			Height:      a.world.Terrain.Height,
			Entities:    len(a.world.Entities),
			Doodads:     len(a.world.Doodads),
			SeedPolicy:  a.world.Metadata.SeedPolicy,
			EngineRange: a.world.Metadata.Engine,
			HeightRows:  cloneIntGrid(a.world.Height),
			CliffRows:   cliffGridStrings(a.world.Cliff),
			SplatRows:   splatGridWeights(a.world.Splat),
		}
		if len(a.world.Height) > 1 && len(a.world.Height[1]) > 1 {
			s.World.HeightCell = a.world.Height[1][1]
		}
		if len(a.world.Cliff) > 1 && len(a.world.Cliff[1]) > 1 {
			s.World.CliffCell = cliffCellLabel(a.world.Cliff[1][1])
		}
		if len(a.world.Splat) > 1 && len(a.world.Splat[1]) > 1 {
			s.World.SplatCell = splatCellLabel(a.world.Splat[1][1])
		}
	}
	return s
}

func (a *App) executeCommand(cmd Command) error {
	if err := a.ensureCommandStack().Execute(a, cmd); err != nil {
		a.errText = err.Error()
		return err
	}
	a.errText = ""
	return nil
}

func (a *App) recordAppliedCommand(cmd Command) error {
	if err := a.ensureCommandStack().RecordApplied(cmd); err != nil {
		a.errText = err.Error()
		return err
	}
	a.errText = ""
	return nil
}

func (a *App) ensureCommandStack() *CommandStack {
	if a.commands == nil {
		a.commands = NewCommandStack(CommandStackLimit)
	}
	return a.commands
}

func (a *App) resetCommandStack() {
	a.commands = NewCommandStack(CommandStackLimit)
}

func (a *App) setGridCellDirect(kind sourceform.GridKind, x, y, value int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.SetGridCell(kind, x, y, value)
}

func (a *App) setCliffCellDirect(x, y int, cell sourceform.CliffCell) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.SetCliffCell(x, y, cell)
}

func (a *App) setCliffFlagsDirect(flags []CliffFlagSnapshot) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	a.cliffFlags = cloneCliffFlags(flags)
	return nil
}

func (a *App) setSplatCellDirect(x, y int, cell sourceform.SplatWeight) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.SetSplatCell(x, y, cell)
}

func (a *App) moveEntityDirect(id uint32, pos [2]int, rotation, scale int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.SetEntityTransform(id, pos, rotation, scale)
}

func (a *App) addEntityDirect(ent sourceform.Entity) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.AddEntity(ent)
}

func (a *App) deleteEntityDirect(id uint32) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.DeleteEntity(id)
}

func (a *App) addDoodadDirect(d sourceform.Doodad) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.AddDoodad(d)
}

func (a *App) moveDoodadDirect(id uint32, pos [2]int, rotation, scale int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.SetDoodadTransform(id, pos, rotation, scale)
}

func (a *App) deleteDoodadDirect(id uint32) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.DeleteDoodad(id)
}

func (a *App) setMetadataNameDirect(name string) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.SetMetadataName(name)
}

func gridCellValue(w *sourceform.World, kind sourceform.GridKind, x, y int) (int, error) {
	if w == nil {
		return 0, errors.New("editor shell: no project loaded")
	}
	var grid [][]int
	switch kind {
	case sourceform.GridHeight:
		grid = w.Height
	case sourceform.GridCliff:
		cell, err := w.CliffCell(x, y)
		if err != nil {
			return 0, err
		}
		return cell.Level, nil
	case sourceform.GridSplat:
		cell, err := w.SplatCell(x, y)
		if err != nil {
			return 0, err
		}
		return splatDominantLayer(cell), nil
	default:
		return 0, fmt.Errorf("editor shell: unknown grid %q", kind)
	}
	if y < 0 || y >= len(grid) || x < 0 || len(grid) == 0 || x >= len(grid[y]) {
		return 0, fmt.Errorf("editor shell: %s cell (%d,%d) outside %dx%d grid", kind, x, y, w.Terrain.Width, w.Terrain.Height)
	}
	return grid[y][x], nil
}

func cliffCellValue(w *sourceform.World, x, y int) (sourceform.CliffCell, error) {
	if w == nil {
		return sourceform.CliffCell{}, errors.New("editor shell: no project loaded")
	}
	return w.CliffCell(x, y)
}

func splatCellValue(w *sourceform.World, x, y int) (sourceform.SplatWeight, error) {
	if w == nil {
		return sourceform.SplatWeight{}, errors.New("editor shell: no project loaded")
	}
	return w.SplatCell(x, y)
}

func entityTransform(w *sourceform.World, id uint32) ([2]int, int, int, error) {
	if w == nil {
		return [2]int{}, 0, 0, errors.New("editor shell: no project loaded")
	}
	for _, ent := range w.Entities {
		if ent.ID == id {
			return ent.Pos, ent.Rotation, ent.Scale, nil
		}
	}
	return [2]int{}, 0, 0, fmt.Errorf("editor shell: entity id %d not found", id)
}

func (s Snapshot) JSON() string {
	body, _ := json.Marshal(s)
	return string(body)
}

func defaultWorld(name string) *sourceform.World {
	grid := func() [][]int {
		rows := make([][]int, 8)
		for y := range rows {
			rows[y] = make([]int, 8)
		}
		return rows
	}
	cliffGrid := func() [][]sourceform.CliffCell {
		rows := make([][]sourceform.CliffCell, 8)
		for y := range rows {
			rows[y] = make([]sourceform.CliffCell, 8)
		}
		return rows
	}
	splatGrid := func() [][]sourceform.SplatWeight {
		rows := make([][]sourceform.SplatWeight, 8)
		for y := range rows {
			rows[y] = make([]sourceform.SplatWeight, 8)
			for x := range rows[y] {
				rows[y][x] = sourceform.SplatWeight{A: 255}
			}
		}
		return rows
	}
	return &sourceform.World{
		Metadata: sourceform.Metadata{
			Format:      1,
			ID:          "untitled-world",
			Name:        name,
			Description: "loc:world.desc",
			Authors:     []string{"Light in the Dark Editor"},
			Engine:      ">=0.1 <0.2",
			Players:     sourceform.Players{Min: 1, Max: 2, Suggested: 1},
			SeedPolicy:  "host",
		},
		Terrain: sourceform.Terrain{Width: 8, Height: 8, Tileset: "vigil-lowlands"},
		Height:  grid(),
		Cliff:   cliffGrid(),
		Splat:   splatGrid(),
		Entities: []sourceform.Entity{
			{ID: 1, Type: "footman", Player: 0, Pos: [2]int{4096, 4096}, Rotation: 0, Scale: sourceform.PlacementScaleDefault},
		},
	}
}

func cloneIntGrid(grid [][]int) [][]int {
	out := make([][]int, len(grid))
	for y := range grid {
		out[y] = append([]int(nil), grid[y]...)
	}
	return out
}

func cliffGridStrings(grid [][]sourceform.CliffCell) [][]string {
	out := make([][]string, len(grid))
	for y := range grid {
		out[y] = make([]string, len(grid[y]))
		for x, cell := range grid[y] {
			out[y][x] = cliffCellLabel(cell)
		}
	}
	return out
}

func cloneSplatGrid(grid [][]sourceform.SplatWeight) [][]sourceform.SplatWeight {
	out := make([][]sourceform.SplatWeight, len(grid))
	for y := range grid {
		out[y] = append([]sourceform.SplatWeight(nil), grid[y]...)
	}
	return out
}

func splatGridWeights(grid [][]sourceform.SplatWeight) [][][]int {
	out := make([][][]int, len(grid))
	for y := range grid {
		out[y] = make([][]int, len(grid[y]))
		for x, cell := range grid[y] {
			out[y][x] = []int{int(cell.A), int(cell.B), int(cell.C), int(cell.D)}
		}
	}
	return out
}

func cliffCellLabel(cell sourceform.CliffCell) string {
	if cell.Ramp {
		return fmt.Sprintf("r%d", cell.Level)
	}
	return fmt.Sprint(cell.Level)
}

func splatCellLabel(cell sourceform.SplatWeight) string {
	return fmt.Sprintf("%d,%d,%d,%d", cell.A, cell.B, cell.C, cell.D)
}

func splatDominantLayer(cell sourceform.SplatWeight) int {
	weights := []uint8{cell.A, cell.B, cell.C, cell.D}
	best := 0
	for i := 1; i < len(weights); i++ {
		if weights[i] > weights[best] {
			best = i
		}
	}
	return best
}

func must(t *locale.Table, key locale.Key) string {
	if t == nil {
		panic("editor shell: nil locale table")
	}
	return t.Must(key)
}
