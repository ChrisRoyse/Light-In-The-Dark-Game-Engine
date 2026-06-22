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
	"path"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	mapdata "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/mapdata"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldpack"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/buildinfo"
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
	archivePath     string
	archiveReadOnly bool
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
	Title           string               `json:"title"`
	ProjectPath     string               `json:"projectPath"`
	ArchivePath     string               `json:"archivePath,omitempty"`
	ArchiveReadOnly bool                 `json:"archiveReadOnly,omitempty"`
	Mode            Mode                 `json:"mode"`
	ModeLabel       string               `json:"modeLabel"`
	Dirty           bool                 `json:"dirty"`
	DirtyLabel      string               `json:"dirtyLabel"`
	Status          string               `json:"status"`
	Error           string               `json:"error,omitempty"`
	Confirm         *Confirm             `json:"confirm,omitempty"`
	Labels          map[string]string    `json:"labels"`
	World           WorldSnapshot        `json:"world"`
	Stack           StackSnapshot        `json:"stack"`
	Brush           TerrainBrushSnapshot `json:"brush"`
	TerrainTool     TerrainTool          `json:"terrainTool"`
	Paint           PaintBrushSnapshot   `json:"paint"`
	CliffFlags      []CliffFlagSnapshot  `json:"cliffFlags,omitempty"`
	Objects         ObjectSnapshot       `json:"objects"`
}

type WorldSnapshot struct {
	ID          string                     `json:"id"`
	Name        string                     `json:"name"`
	Description string                     `json:"description"`
	Width       int                        `json:"width"`
	Height      int                        `json:"height"`
	Tileset     string                     `json:"tileset"`
	SplatSet    string                     `json:"splatSet"`
	Entities    int                        `json:"entities"`
	Doodads     int                        `json:"doodads"`
	Players     sourceform.Players         `json:"players"`
	Starts      []sourceform.StartLocation `json:"starts,omitempty"`
	HeightCell  int                        `json:"heightCell"`
	CliffCell   string                     `json:"cliffCell"`
	SplatCell   string                     `json:"splatCell"`
	SeedPolicy  string                     `json:"seedPolicy"`
	EngineRange string                     `json:"engineRange"`
	HeightRows  [][]int                    `json:"heightRows,omitempty"`
	CliffRows   [][]string                 `json:"cliffRows,omitempty"`
	SplatRows   [][][]int                  `json:"splatRows,omitempty"`
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
	a.archivePath = ""
	a.archiveReadOnly = false
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
		a.archivePath = ""
		a.archiveReadOnly = false
		a.mode = ModeTerrain
		a.errText = ""
		a.confirm = nil
		a.cliffFlags = nil
		a.status = must(a.table, locale.EditorStatusProjectOpened)
		a.resetCommandStack()
		return nil
	}
	if filepath.Ext(path) == ".litdworld" {
		return a.OpenArchive(path, "")
	}
	return a.openError(path, fmt.Errorf("unsupported project path %q", path))
}

// OpenArchive verifies a .litdworld, then opens it in the editor. Archives that
// contain source-form files are unpacked into workDir and become editable;
// runtime M6 archives open as a verified read-only projection for inspection.
func (a *App) OpenArchive(archivePath, workDir string) error {
	if archivePath == "" {
		return a.openError(archivePath, errors.New("empty archive path"))
	}
	arc, err := worldarchive.Open(archivePath, EditorEngineVersion())
	if err != nil {
		return a.openError(archivePath, err)
	}
	defer arc.Close()

	if archiveHasSourceForm(arc.Manifest) {
		dir, err := prepareArchiveWorkDir(archivePath, workDir)
		if err != nil {
			return a.openError(archivePath, err)
		}
		if err := worldpack.Unpack(archivePath, dir); err != nil {
			return a.openError(archivePath, err)
		}
		w, err := sourceform.Load(dir)
		if err != nil {
			return a.openError(archivePath, err)
		}
		a.world = w
		a.projectPath = dir
		a.archivePath = archivePath
		a.archiveReadOnly = false
		a.mode = ModeTerrain
		a.errText = ""
		a.confirm = nil
		a.cliffFlags = nil
		a.status = fmt.Sprintf("Archive opened: %s", archivePath)
		a.resetCommandStack()
		return nil
	}

	w, err := runtimeArchiveProjection(arc)
	if err != nil {
		return a.openError(archivePath, err)
	}
	a.world = w
	a.projectPath = archivePath
	a.archivePath = archivePath
	a.archiveReadOnly = true
	a.mode = ModeMetadata
	a.errText = ""
	a.confirm = nil
	a.cliffFlags = nil
	a.status = fmt.Sprintf("Archive opened read-only: %s", archivePath)
	a.resetCommandStack()
	return nil
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

func (a *App) SetMapMetadata(name, description, engineRange string, players sourceform.Players, tileset, splatSet string) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	before := mapMetadataState{
		Name:        a.world.Metadata.Name,
		Description: a.world.Metadata.Description,
		EngineRange: a.world.Metadata.Engine,
		Players:     a.world.Metadata.Players,
		Tileset:     a.world.Terrain.Tileset,
		SplatSet:    a.world.Terrain.Biome,
	}
	after := mapMetadataState{
		Name:        name,
		Description: description,
		EngineRange: engineRange,
		Players:     players,
		Tileset:     tileset,
		SplatSet:    splatSet,
	}
	if err := a.executeCommand(mapMetadataCommand{before: before, after: after}); err != nil {
		a.status = err.Error()
		return err
	}
	a.status = fmt.Sprintf("Metadata updated: %s", name)
	return nil
}

func (a *App) PutStartLocationCell(player, x, y int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	if err := validateObjectCell(a.world, x, y); err != nil {
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
	ok, err := a.UnitPlacementWalkableCell(x, y)
	if err != nil {
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
	if !ok {
		err := fmt.Errorf("editor metadata: start location rejected at unwalkable cell %d,%d", x, y)
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
	before, hadBefore := a.world.StartLocationByPlayer(player)
	after := sourceform.StartLocation{Player: player, Cell: [2]int{x, y}}
	if err := a.executeCommand(startLocationCommand{before: before, hadBefore: hadBefore, after: after, hasAfter: true}); err != nil {
		a.status = err.Error()
		return err
	}
	a.mode = ModeMetadata
	a.status = fmt.Sprintf("Start location player %d set to %d,%d", player, x, y)
	return nil
}

func (a *App) AddStartLocationCell(player, x, y int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	if _, exists := a.world.StartLocationByPlayer(player); exists {
		err := fmt.Errorf("editor metadata: duplicate start location for player %d", player)
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
	return a.PutStartLocationCell(player, x, y)
}

func (a *App) RemoveStartLocation(player int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	before, ok := a.world.StartLocationByPlayer(player)
	if !ok {
		return fmt.Errorf("editor metadata: start location for player %d not found", player)
	}
	return a.executeCommand(startLocationCommand{before: before, hadBefore: true})
}

func (a *App) ExportArchive(outPath string) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	if a.archiveReadOnly {
		err := errors.New("editor shell: archive opened read-only; source-form payload required for archive save")
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
	opts := sourceform.ExportOptions{
		EngineRange: a.world.Metadata.Engine,
		Hosting: worldpack.Hosting{
			Author:      strings.Join(a.world.Metadata.Authors, ", "),
			Title:       a.world.Metadata.Name,
			Description: a.world.Metadata.Description,
			Players: worldpack.Players{
				Min:       a.world.Metadata.Players.Min,
				Max:       a.world.Metadata.Players.Max,
				Suggested: a.world.Metadata.Players.Suggested,
			},
			Tileset:  a.world.Terrain.Tileset,
			SplatSet: a.world.Terrain.Biome,
		},
	}
	for _, start := range a.world.Terrain.StartLocations {
		opts.Hosting.StartLocations = append(opts.Hosting.StartLocations, worldpack.StartLocation{Player: start.Player, Cell: start.Cell})
	}
	if err := a.world.ExportArchive(outPath, opts); err != nil {
		a.errText = err.Error()
		return err
	}
	a.status = fmt.Sprintf("Exported archive: %s", outPath)
	a.errText = ""
	return nil
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
	if a.archivePath != "" {
		return a.SaveArchive(a.archivePath)
	}
	if err := a.world.Save(a.projectPath); err != nil {
		a.errText = err.Error()
		return err
	}
	a.status = must(a.table, locale.EditorDirtyClean)
	a.errText = ""
	return nil
}

// SaveArchive saves the current source-form state and packs it as a .litdworld.
// It preflights the output path before writing source-form bytes so a refused
// archive write keeps the in-memory dirty state visible to the user.
func (a *App) SaveArchive(outPath string) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	if a.archiveReadOnly {
		err := errors.New("editor shell: archive opened read-only; source-form payload required for archive save")
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
	if err := ensureArchiveWritable(outPath); err != nil {
		a.errText = fmt.Sprintf("save archive %s: %v", outPath, err)
		a.status = a.errText
		return err
	}
	if err := a.ExportArchive(outPath); err != nil {
		a.status = a.errText
		return err
	}
	a.archivePath = outPath
	a.status = fmt.Sprintf("Saved archive: %s", outPath)
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
		"fieldDescription":  must(a.table, locale.EditorFieldDescription),
		"fieldEngine":       must(a.table, locale.EditorFieldEngine),
		"fieldPlayers":      must(a.table, locale.EditorFieldPlayers),
		"fieldTileset":      must(a.table, locale.EditorFieldTileset),
		"fieldSplatSet":     must(a.table, locale.EditorFieldSplatSet),
		"fieldStarts":       must(a.table, locale.EditorFieldStarts),
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
		Title:           title,
		ProjectPath:     a.projectPath,
		ArchivePath:     a.archivePath,
		ArchiveReadOnly: a.archiveReadOnly,
		Mode:            a.mode,
		ModeLabel:       must(a.table, locale.EditorModeLabel),
		Dirty:           dirty,
		DirtyLabel:      dirtyLabel,
		Status:          a.status,
		Error:           a.errText,
		Confirm:         a.confirm,
		Labels:          labels,
		Stack:           a.StackSnapshot(),
		Brush:           a.BrushSnapshot(),
		TerrainTool:     a.ensureTerrainTool(),
		Paint:           a.PaintSnapshot(),
		CliffFlags:      cloneCliffFlags(a.cliffFlags),
		Objects:         a.ObjectSnapshot(),
	}
	if a.world != nil {
		s.World = WorldSnapshot{
			ID:          a.world.Metadata.ID,
			Name:        a.world.Metadata.Name,
			Description: a.world.Metadata.Description,
			Width:       a.world.Terrain.Width,
			Height:      a.world.Terrain.Height,
			Tileset:     a.world.Terrain.Tileset,
			SplatSet:    a.world.Terrain.Biome,
			Entities:    len(a.world.Entities),
			Doodads:     len(a.world.Doodads),
			Players:     a.world.Metadata.Players,
			Starts:      append([]sourceform.StartLocation(nil), a.world.Terrain.StartLocations...),
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
	if a.archiveReadOnly {
		err := errors.New("editor shell: archive opened read-only; source-form payload required for editing")
		a.errText = err.Error()
		a.status = a.errText
		return err
	}
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

func (a *App) setMapMetadataDirect(state mapMetadataState) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.SetMapMetadata(state.Name, state.Description, state.EngineRange, state.Players, state.Tileset, state.SplatSet)
}

func (a *App) putStartLocationDirect(start sourceform.StartLocation) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.PutStartLocation(start)
}

func (a *App) removeStartLocationDirect(player int) error {
	if a.world == nil {
		return errors.New("editor shell: no project loaded")
	}
	return a.world.RemoveStartLocation(player)
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
			Engine:      DefaultEngineRange(),
			Players:     sourceform.Players{Min: 1, Max: 8, Suggested: 2},
			SeedPolicy:  "host",
		},
		Terrain: sourceform.Terrain{
			Width:   8,
			Height:  8,
			Tileset: "vigil-lowlands",
			Biome:   "vigil-lowlands",
			StartLocations: []sourceform.StartLocation{
				{Player: 1, Cell: [2]int{1, 1}},
				{Player: 2, Cell: [2]int{6, 6}},
			},
		},
		Height: grid(),
		Cliff:  cliffGrid(),
		Splat:  splatGrid(),
		Entities: []sourceform.Entity{
			{ID: 1, Type: "footman", Player: 0, Pos: [2]int{4096, 4096}, Rotation: 0, Scale: sourceform.PlacementScaleDefault},
		},
	}
}

func DefaultEngineRange() string {
	v := strings.TrimPrefix(buildinfo.Get().Version, "v")
	parts := strings.Split(v, ".")
	if len(parts) == 3 {
		major, majorErr := strconv.Atoi(parts[0])
		minor, minorErr := strconv.Atoi(parts[1])
		patch, patchErr := strconv.Atoi(parts[2])
		if majorErr == nil && minorErr == nil && patchErr == nil && major >= 0 && minor >= 0 && patch >= 0 {
			return fmt.Sprintf(">=%d.%d.%d <%d.%d.0", major, minor, patch, major, minor+1)
		}
	}
	return ">=0.1.0 <0.2.0"
}

// EditorEngineVersion returns the concrete semver used by the editor archive
// loader. Unstamped dev builds use 0.1.0 so local FSV exercises the same
// fail-closed engine-range guard as released builds.
func EditorEngineVersion() string {
	v := strings.TrimPrefix(buildinfo.Get().Version, "v")
	parts := strings.Split(v, ".")
	if len(parts) != 3 {
		return "0.1.0"
	}
	for _, p := range parts {
		n, err := strconv.Atoi(p)
		if err != nil || n < 0 {
			return "0.1.0"
		}
	}
	return v
}

func archiveHasSourceForm(man worldarchive.Manifest) bool {
	for _, rel := range []string{"world.toml", "map/terrain.toml", "map/height.txt", "map/cliff.txt", "map/splat.txt", "map/entities.toml", "map/doodads.toml"} {
		if _, ok := man.Files[rel]; !ok {
			return false
		}
	}
	return true
}

func prepareArchiveWorkDir(archivePath, workDir string) (string, error) {
	if workDir == "" {
		base := strings.TrimSuffix(filepath.Base(archivePath), filepath.Ext(archivePath))
		dir, err := os.MkdirTemp("", "litd-editor-"+sanitizeWorldID(base)+"-")
		if err != nil {
			return "", err
		}
		return dir, nil
	}
	if entries, err := os.ReadDir(workDir); err == nil {
		if len(entries) > 0 {
			return "", fmt.Errorf("archive work directory %q is not empty", workDir)
		}
		return workDir, nil
	} else if !errors.Is(err, os.ErrNotExist) {
		return "", err
	}
	return workDir, nil
}

func runtimeArchiveProjection(arc *worldarchive.Archive) (*sourceform.World, error) {
	dirs := archiveMapDirs(arc.Manifest)
	var lastErr error
	for _, dir := range dirs {
		m, err := mapdata.Load(arc.FS(), dir)
		if err != nil {
			lastErr = err
			continue
		}
		return projectRuntimeMap(arc.Manifest, m), nil
	}
	if lastErr != nil {
		return nil, fmt.Errorf("archive has no editable source-form payload and no loadable runtime map: %w", lastErr)
	}
	return nil, errors.New("archive has no editable source-form payload and no data/maps/* runtime map")
}

func archiveMapDirs(man worldarchive.Manifest) []string {
	seen := map[string]bool{}
	for rel := range man.Files {
		if !strings.HasPrefix(rel, "data/maps/") || path.Base(rel) != "terrain.toml" {
			continue
		}
		dir := path.Dir(rel)
		if dir == "." || dir == "data/maps" {
			continue
		}
		seen[dir] = true
	}
	out := make([]string, 0, len(seen))
	for dir := range seen {
		out = append(out, dir)
	}
	sortStrings(out)
	return out
}

func projectRuntimeMap(man worldarchive.Manifest, m *mapdata.Map) *sourceform.World {
	starts := make([]sourceform.StartLocation, 0, len(m.Starts()))
	for _, start := range m.Starts() {
		starts = append(starts, sourceform.StartLocation{
			Player: int(start.Player) + 1,
			Cell:   [2]int{start.X / mapdata.PathingScale, start.Y / mapdata.PathingScale},
		})
	}
	players := sourceform.Players{Min: 1, Max: maxInt(1, len(starts)), Suggested: maxInt(1, len(starts))}
	if man.Players != (worldarchive.Players{}) {
		players = sourceform.Players{Min: man.Players.Min, Max: man.Players.Max, Suggested: man.Players.Suggested}
	}
	tileset := man.Tileset
	if tileset == "" {
		tileset = m.Biome
	}
	splatSet := man.SplatSet
	if splatSet == "" {
		splatSet = m.Biome
	}
	return &sourceform.World{
		Metadata: sourceform.Metadata{
			Format:      1,
			ID:          sanitizeWorldID(path.Base(m.Path)),
			Name:        firstNonEmpty(man.Title, path.Base(m.Path)),
			Description: firstNonEmpty(man.Description, "runtime archive projection"),
			Authors:     []string{firstNonEmpty(man.Author, "unknown")},
			Engine:      man.EngineRange,
			Players:     players,
			SeedPolicy:  "host",
		},
		Terrain: sourceform.Terrain{
			Width:          m.Width,
			Height:         m.Height,
			Tileset:        tileset,
			Biome:          splatSet,
			StartLocations: starts,
		},
		Height:  runtimeHeightGrid(m),
		Cliff:   runtimeCliffGrid(m),
		Splat:   runtimeSplatGrid(m),
		Doodads: runtimeDoodads(m),
	}
}

func runtimeHeightGrid(m *mapdata.Map) [][]int {
	rows := make([][]int, m.Height)
	for y := 0; y < m.Height; y++ {
		rows[y] = make([]int, m.Width)
		for x := 0; x < m.Width; x++ {
			if v, ok := m.HeightAtVertex(x, y); ok {
				rows[y][x] = int(v)
			}
		}
	}
	return rows
}

func runtimeCliffGrid(m *mapdata.Map) [][]sourceform.CliffCell {
	rows := make([][]sourceform.CliffCell, m.Height)
	for y := 0; y < m.Height; y++ {
		rows[y] = make([]sourceform.CliffCell, m.Width)
		for x := 0; x < m.Width; x++ {
			if c, ok := m.CliffAt(x*mapdata.PathingScale, y*mapdata.PathingScale); ok {
				rows[y][x] = sourceform.CliffCell{Level: int(c.Level), Ramp: c.Ramp}
			}
		}
	}
	return rows
}

func runtimeSplatGrid(m *mapdata.Map) [][]sourceform.SplatWeight {
	rows := make([][]sourceform.SplatWeight, m.Height)
	for y := 0; y < m.Height; y++ {
		rows[y] = make([]sourceform.SplatWeight, m.Width)
		for x := 0; x < m.Width; x++ {
			if s, ok := m.SplatAt(x, y); ok {
				rows[y][x] = sourceform.SplatWeight{A: s.A, B: s.B, C: s.C, D: s.D}
			}
		}
	}
	return rows
}

func runtimeDoodads(m *mapdata.Map) []sourceform.Doodad {
	doodads := m.Doodads()
	out := make([]sourceform.Doodad, 0, len(doodads))
	for _, d := range doodads {
		out = append(out, sourceform.Doodad{
			ID:       d.ID,
			Type:     d.Asset,
			Pos:      [2]int{d.X, d.Y},
			Rotation: int(d.Rotation),
			Scale:    sourceform.PlacementScaleDefault,
		})
	}
	return out
}

func ensureArchiveWritable(outPath string) error {
	if strings.TrimSpace(outPath) == "" {
		return errors.New("archive path is empty")
	}
	if st, err := os.Stat(outPath); err == nil {
		if st.IsDir() {
			return fmt.Errorf("%q is a directory", outPath)
		}
		if st.Mode().Perm()&0o222 == 0 {
			return fmt.Errorf("%q is read-only", outPath)
		}
	} else if !errors.Is(err, os.ErrNotExist) {
		return err
	}
	parent := filepath.Dir(outPath)
	st, err := os.Stat(parent)
	if err != nil {
		return err
	}
	if !st.IsDir() {
		return fmt.Errorf("archive parent %q is not a directory", parent)
	}
	if st.Mode().Perm()&0o222 == 0 {
		return fmt.Errorf("archive parent %q is read-only", parent)
	}
	return nil
}

func sanitizeWorldID(s string) string {
	s = strings.ToLower(strings.TrimSpace(s))
	var b strings.Builder
	lastHyphen := false
	for _, r := range s {
		ok := r >= 'a' && r <= 'z' || r >= '0' && r <= '9'
		if ok {
			b.WriteRune(r)
			lastHyphen = false
			continue
		}
		if !lastHyphen && b.Len() > 0 {
			b.WriteByte('-')
			lastHyphen = true
		}
	}
	out := strings.Trim(b.String(), "-")
	if out == "" {
		return "archive-world"
	}
	return out
}

func firstNonEmpty(values ...string) string {
	for _, v := range values {
		if strings.TrimSpace(v) != "" {
			return v
		}
	}
	return ""
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func sortStrings(values []string) {
	for i := 1; i < len(values); i++ {
		for j := i; j > 0 && values[j] < values[j-1]; j-- {
			values[j], values[j-1] = values[j-1], values[j]
		}
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
