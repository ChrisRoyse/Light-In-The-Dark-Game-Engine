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
	table       *locale.Table
	world       *sourceform.World
	projectPath string
	mode        Mode
	status      string
	errText     string
	confirm     *Confirm
}

type Confirm struct {
	Kind       string `json:"kind"`
	TargetPath string `json:"targetPath"`
	Title      string `json:"title"`
	Body       string `json:"body"`
}

type Snapshot struct {
	Title       string            `json:"title"`
	ProjectPath string            `json:"projectPath"`
	Mode        Mode              `json:"mode"`
	ModeLabel   string            `json:"modeLabel"`
	Dirty       bool              `json:"dirty"`
	DirtyLabel  string            `json:"dirtyLabel"`
	Status      string            `json:"status"`
	Error       string            `json:"error,omitempty"`
	Confirm     *Confirm          `json:"confirm,omitempty"`
	Labels      map[string]string `json:"labels"`
	World       WorldSnapshot     `json:"world"`
}

type WorldSnapshot struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Width       int    `json:"width"`
	Height      int    `json:"height"`
	Entities    int    `json:"entities"`
	HeightCell  int    `json:"heightCell"`
	SeedPolicy  string `json:"seedPolicy"`
	EngineRange string `json:"engineRange"`
}

func New(table *locale.Table) *App {
	return &App{table: table, mode: ModeTerrain, status: must(table, locale.EditorStatusReady)}
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
	a.status = must(a.table, locale.EditorStatusProjectCreated)
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
		a.status = must(a.table, locale.EditorStatusProjectOpened)
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
	a.errText = ""
	return a.world.SetGridCell(sourceform.GridHeight, x, y, value)
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
	}
	if a.world != nil {
		s.World = WorldSnapshot{
			ID:          a.world.Metadata.ID,
			Name:        a.world.Metadata.Name,
			Width:       a.world.Terrain.Width,
			Height:      a.world.Terrain.Height,
			Entities:    len(a.world.Entities),
			SeedPolicy:  a.world.Metadata.SeedPolicy,
			EngineRange: a.world.Metadata.Engine,
		}
		if len(a.world.Height) > 1 && len(a.world.Height[1]) > 1 {
			s.World.HeightCell = a.world.Height[1][1]
		}
	}
	return s
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
		Cliff:   grid(),
		Splat:   grid(),
		Entities: []sourceform.Entity{
			{ID: 1, Type: "vigil-footman", Player: 0, Pos: [2]int{4096, 4096}, Facing: 0},
		},
	}
}

func must(t *locale.Table, key locale.Key) string {
	if t == nil {
		panic("editor shell: nil locale table")
	}
	return t.Must(key)
}
