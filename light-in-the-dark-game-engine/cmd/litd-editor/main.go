// Command litd-editor is the M8 editor application shell (#125).
package main

import (
	"archive/zip"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"image/png"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"time"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/cmd/litd-editor/shell"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/editor/sourceform"
)

func main() {
	autotest := flag.Bool("autotest", false, "run scripted FSV sequence and write mode screenshots")
	headless := flag.Bool("headless", false, "write one shell screenshot and exit without opening the G3N window")
	capture := flag.Bool("capture", false, "open the G3N window, capture one framebuffer screenshot, then exit")
	outDir := flag.String("out", "artifacts/litd-editor", "output directory for screenshots/source project")
	newPath := flag.String("new", "", "create/open a source-form project at this directory")
	openPath := flag.String("open", "", "open a source-form project directory")
	shotPath := flag.String("shot", "", "screenshot path for F12, -capture, or -headless")
	mode := flag.String("mode", "", "initial editor mode: terrain, objects, metadata, triggers, or faction")
	flag.Parse()

	table, err := locale.Load(os.DirFS("data"), "en")
	if err != nil {
		fatalf("load locale: %v", err)
	}
	app := shell.New(table)
	if err := app.LoadObjectPalette(os.DirFS("data")); err != nil {
		fatalf("load object palette: %v", err)
	}
	if *autotest {
		var err error
		if *capture {
			err = runWindowAutotest(app, *outDir)
		} else {
			err = runAutotest(app, *outDir)
		}
		if err != nil {
			fatalf("autotest: %v", err)
		}
		return
	}
	switch {
	case *newPath != "":
		if err := app.NewProject(*newPath); err != nil {
			fatalf("new project: %v", err)
		}
	case *openPath != "":
		if err := app.OpenProject(*openPath); err != nil {
			fmt.Fprintf(os.Stderr, "litd-editor: open: %v\n", err)
		}
	default:
		path := filepath.Join(*outDir, "scratch-world")
		if err := app.NewProject(path); err != nil {
			fatalf("new scratch project: %v", err)
		}
	}
	if *mode != "" {
		if err := app.SwitchMode(shell.Mode(*mode)); err != nil {
			fatalf("mode: %v", err)
		}
	}
	shot := *shotPath
	if shot == "" {
		shot = filepath.Join(*outDir, "litd-editor.png")
	}
	if *headless {
		if err := shell.RenderPNG(shot, app.Snapshot()); err != nil {
			fatalf("screenshot: %v", err)
		}
		fmt.Printf("event: screenshot saved path=%s\n", shot)
		fmt.Printf("state: %s\n", app.Snapshot().JSON())
		return
	}
	if err := shell.RunWindow(app, shell.WindowOptions{ShotPath: shot, CaptureFrame: *capture, ExitAfterShot: *capture}); err != nil {
		fatalf("window: %v", err)
	}
}

func runAutotest(app *shell.App, outDir string) error {
	worldDir := filepath.Join(outDir, "autotest-world")
	if err := os.RemoveAll(worldDir); err != nil {
		return err
	}
	if err := app.NewProject(worldDir); err != nil {
		return err
	}
	if err := renderState(outDir, "01-terrain-clean.png", app); err != nil {
		return err
	}
	if err := app.EditTerrainHeight(1, 1, 7); err != nil {
		return err
	}
	if err := app.SwitchMode(shell.ModeObjects); err != nil {
		return err
	}
	if err := renderState(outDir, "02-objects-dirty.png", app); err != nil {
		return err
	}
	if err := app.SwitchMode(shell.ModeMetadata); err != nil {
		return err
	}
	if err := renderState(outDir, "03-metadata-dirty.png", app); err != nil {
		return err
	}
	if err := app.OpenProject(filepath.Join(outDir, "missing.litdworld")); err != nil {
		// Expected edge: shell retains current project and exposes a dialog.
	}
	if err := renderState(outDir, "04-open-error.png", app); err != nil {
		return err
	}
	nextDir := filepath.Join(outDir, "cancelled-new-world")
	if err := app.NewProject(nextDir); err != nil {
		return err
	}
	if err := renderState(outDir, "05-new-confirm.png", app); err != nil {
		return err
	}
	app.CancelConfirm()
	if err := renderState(outDir, "06-new-cancel-preserves-dirty.png", app); err != nil {
		return err
	}
	if err := runBrushFSV(app, outDir); err != nil {
		return err
	}
	if err := runPaintFSV(app, outDir); err != nil {
		return err
	}
	cliffState := newCliffFSVState()
	if err := runCliffFSV(app, outDir, cliffState); err != nil {
		return err
	}
	objectState := newObjectFSVState()
	if err := runObjectFSV(app, outDir, objectState); err != nil {
		return err
	}
	metadataState := newMetadataFSVState()
	if err := runMetadataFSV(app, outDir, metadataState); err != nil {
		return err
	}
	if err := runArchiveRoundTripFSV(app, outDir); err != nil {
		return err
	}
	if err := runMinimapFSV(app, outDir); err != nil {
		return err
	}
	if err := runPlaytestFSV(app, outDir); err != nil {
		return err
	}
	if err := runM8EndToEndFSV(app, outDir); err != nil {
		return err
	}
	body, _ := json.MarshalIndent(app.Snapshot(), "", "  ")
	if err := os.WriteFile(filepath.Join(outDir, "final-state.json"), append(body, '\n'), 0o644); err != nil {
		return err
	}
	fmt.Printf("state: %s\n", app.Snapshot().JSON())
	return nil
}

func runWindowAutotest(app *shell.App, outDir string) error {
	worldDir := filepath.Join(outDir, "autotest-world")
	if err := os.RemoveAll(worldDir); err != nil {
		return err
	}
	if err := app.NewProject(worldDir); err != nil {
		return err
	}
	nextDir := filepath.Join(outDir, "cancelled-new-world")
	steps := []shell.WindowCaptureStep{
		{Name: "terrain-clean", ShotPath: filepath.Join(outDir, "01-terrain-clean.png")},
		{
			Name:     "objects-dirty",
			ShotPath: filepath.Join(outDir, "02-objects-dirty.png"),
			BeforeCapture: func() error {
				if err := app.EditTerrainHeight(1, 1, 7); err != nil {
					return err
				}
				return app.SwitchMode(shell.ModeObjects)
			},
		},
		{
			Name:     "metadata-dirty",
			ShotPath: filepath.Join(outDir, "03-metadata-dirty.png"),
			BeforeCapture: func() error {
				return app.SwitchMode(shell.ModeMetadata)
			},
		},
		{
			Name:     "open-error",
			ShotPath: filepath.Join(outDir, "04-open-error.png"),
			BeforeCapture: func() error {
				_ = app.OpenProject(filepath.Join(outDir, "missing.litdworld"))
				return nil
			},
		},
		{
			Name:     "new-confirm",
			ShotPath: filepath.Join(outDir, "05-new-confirm.png"),
			BeforeCapture: func() error {
				return app.NewProject(nextDir)
			},
		},
		{
			Name:     "new-cancel-preserves-dirty",
			ShotPath: filepath.Join(outDir, "06-new-cancel-preserves-dirty.png"),
			BeforeCapture: func() error {
				app.CancelConfirm()
				return nil
			},
		},
	}
	for _, step := range brushWindowSteps(app, outDir) {
		steps = append(steps, step)
	}
	var paintBeforeHash, paintAfterHash string
	var paintStack shell.StackSnapshot
	for _, step := range paintWindowSteps(app, outDir, &paintBeforeHash, &paintAfterHash, &paintStack) {
		steps = append(steps, step)
	}
	cliffState := newCliffFSVState()
	for _, step := range cliffWindowSteps(app, outDir, cliffState) {
		steps = append(steps, step)
	}
	objectState := newObjectFSVState()
	for _, step := range objectWindowSteps(app, outDir, objectState) {
		steps = append(steps, step)
	}
	metadataState := newMetadataFSVState()
	for _, step := range metadataWindowSteps(app, outDir, metadataState) {
		steps = append(steps, step)
	}
	if err := shell.RunWindowCaptureSequence(app, steps); err != nil {
		return err
	}
	if err := writeBrushDump(app, outDir); err != nil {
		return err
	}
	if err := writePaintDump(app, outDir, paintBeforeHash, paintAfterHash, &paintStack); err != nil {
		return err
	}
	if err := writeCliffDump(app, outDir, cliffState); err != nil {
		return err
	}
	if err := writeObjectDump(app, outDir, objectState); err != nil {
		return err
	}
	if err := writeMetadataDump(app, outDir, metadataState); err != nil {
		return err
	}
	if err := runArchiveRoundTripFSV(app, outDir); err != nil {
		return err
	}
	if err := runMinimapFSV(app, outDir); err != nil {
		return err
	}
	if err := runPlaytestFSV(app, outDir); err != nil {
		return err
	}
	if err := runM8EndToEndFSV(app, outDir); err != nil {
		return err
	}
	body, _ := json.MarshalIndent(app.Snapshot(), "", "  ")
	if err := os.WriteFile(filepath.Join(outDir, "final-state.json"), append(body, '\n'), 0o644); err != nil {
		return err
	}
	fmt.Printf("state: %s\n", app.Snapshot().JSON())
	return nil
}

type editorBrushStep struct {
	Name  string
	Apply func() error
}

func brushSteps(app *shell.App) []editorBrushStep {
	return []editorBrushStep{
		{
			Name: "brush-raise",
			Apply: func() error {
				if err := app.SwitchMode(shell.ModeTerrain); err != nil {
					return err
				}
				if err := app.SetBrushSize(0); err != nil {
					return err
				}
				if err := app.SetBrushStrength(2); err != nil {
					return err
				}
				if err := app.SetTerrainBrush(shell.BrushRaise); err != nil {
					return err
				}
				return app.ApplyTerrainBrush(2, 2)
			},
		},
		{
			Name: "brush-lower",
			Apply: func() error {
				if err := app.SetBrushStrength(1); err != nil {
					return err
				}
				if err := app.SetTerrainBrush(shell.BrushLower); err != nil {
					return err
				}
				return app.ApplyTerrainBrush(2, 2)
			},
		},
		{
			Name: "brush-level",
			Apply: func() error {
				if err := app.SetBrushSize(1); err != nil {
					return err
				}
				app.SetBrushLevelTarget(5)
				if err := app.SetTerrainBrush(shell.BrushLevel); err != nil {
					return err
				}
				return app.ApplyTerrainBrush(4, 4)
			},
		},
		{
			Name: "brush-ramp",
			Apply: func() error {
				if err := app.SetTerrainBrush(shell.BrushRamp); err != nil {
					return err
				}
				if err := app.SetBrushStrength(1); err != nil {
					return err
				}
				if err := app.SetBrushRampDirection(shell.RampEast); err != nil {
					return err
				}
				return app.ApplyTerrainBrush(4, 6)
			},
		},
	}
}

func runBrushFSV(app *shell.App, outDir string) error {
	for i, step := range brushSteps(app) {
		if err := step.Apply(); err != nil {
			return err
		}
		if err := renderState(outDir, fmt.Sprintf("%02d-%s.png", i+7, step.Name), app); err != nil {
			return err
		}
	}
	return writeBrushDump(app, outDir)
}

type editorPaintStep struct {
	Name  string
	Apply func() error
}

func paintSteps(app *shell.App) []editorPaintStep {
	return []editorPaintStep{
		{
			Name: "paint-layer-b",
			Apply: func() error {
				if err := app.SwitchMode(shell.ModeTerrain); err != nil {
					return err
				}
				if err := app.SetPaintSize(0); err != nil {
					return err
				}
				if err := app.SetPaintStrength(255); err != nil {
					return err
				}
				if err := app.SetPaintLayer(1); err != nil {
					return err
				}
				return app.ApplyPaintBrush(2, 2)
			},
		},
		{
			Name: "paint-layer-c-overlap",
			Apply: func() error {
				if err := app.SetPaintLayer(2); err != nil {
					return err
				}
				return app.ApplyPaintBrush(2, 2)
			},
		},
		{
			Name: "paint-boundary",
			Apply: func() error {
				if err := app.SetPaintLayer(3); err != nil {
					return err
				}
				if err := app.SetPaintSize(1); err != nil {
					return err
				}
				return app.ApplyPaintBrush(4, 4)
			},
		},
		{
			Name: "paint-border",
			Apply: func() error {
				if err := app.SetPaintLayer(1); err != nil {
					return err
				}
				if err := app.SetPaintSize(2); err != nil {
					return err
				}
				return app.ApplyPaintBrush(0, 0)
			},
		},
	}
}

func runPaintFSV(app *shell.App, outDir string) error {
	beforeHash := app.SimRelevantHash()
	for i, step := range paintSteps(app) {
		if err := step.Apply(); err != nil {
			return err
		}
		if err := renderState(outDir, fmt.Sprintf("%02d-%s.png", i+11, step.Name), app); err != nil {
			return err
		}
	}
	return writePaintDump(app, outDir, beforeHash, "", nil)
}

func brushWindowSteps(app *shell.App, outDir string) []shell.WindowCaptureStep {
	steps := brushSteps(app)
	out := make([]shell.WindowCaptureStep, 0, len(steps))
	for i, step := range steps {
		step := step
		out = append(out, shell.WindowCaptureStep{
			Name:     step.Name,
			ShotPath: filepath.Join(outDir, fmt.Sprintf("%02d-%s.png", i+7, step.Name)),
			BeforeCapture: func() error {
				return step.Apply()
			},
		})
	}
	return out
}

func paintWindowSteps(app *shell.App, outDir string, beforeHash, afterHash *string, stack *shell.StackSnapshot) []shell.WindowCaptureStep {
	steps := paintSteps(app)
	out := make([]shell.WindowCaptureStep, 0, len(steps))
	for i, step := range steps {
		step := step
		out = append(out, shell.WindowCaptureStep{
			Name:     step.Name,
			ShotPath: filepath.Join(outDir, fmt.Sprintf("%02d-%s.png", i+11, step.Name)),
			BeforeCapture: func() error {
				if beforeHash != nil && *beforeHash == "" {
					*beforeHash = app.SimRelevantHash()
				}
				if err := step.Apply(); err != nil {
					return err
				}
				if afterHash != nil {
					*afterHash = app.SimRelevantHash()
				}
				if stack != nil {
					*stack = app.StackSnapshot()
				}
				return nil
			},
		})
	}
	return out
}

type cliffFSVState struct {
	Pathable      map[string]bool                      `json:"pathable"`
	StepCliffRows map[string][][]string                `json:"stepCliffRows"`
	StepFlags     map[string][]shell.CliffFlagSnapshot `json:"stepFlags"`
}

func newCliffFSVState() *cliffFSVState {
	return &cliffFSVState{
		Pathable:      map[string]bool{},
		StepCliffRows: map[string][][]string{},
		StepFlags:     map[string][]shell.CliffFlagSnapshot{},
	}
}

func (s *cliffFSVState) capture(name string, app *shell.App) {
	if s == nil {
		return
	}
	snap := app.Snapshot()
	s.StepCliffRows[name] = snap.World.CliffRows
	s.StepFlags[name] = snap.CliffFlags
}

type editorCliffStep struct {
	Name  string
	Apply func() error
}

func cliffSteps(app *shell.App, state *cliffFSVState) []editorCliffStep {
	recordPath := func(name string, ax, ay, bx, by int) error {
		ok, err := app.CliffStepLegal(ax, ay, bx, by)
		if err != nil {
			return err
		}
		if state != nil {
			state.Pathable[name] = ok
		}
		return nil
	}
	return []editorCliffStep{
		{
			Name: "cliff-unit-flag",
			Apply: func() error {
				if err := app.SwitchMode(shell.ModeTerrain); err != nil {
					return err
				}
				if err := app.SetTerrainBrush(shell.BrushCliffRaise); err != nil {
					return err
				}
				if err := app.SetBrushSize(0); err != nil {
					return err
				}
				if err := app.SetBrushStrength(1); err != nil {
					return err
				}
				if err := app.ApplyTerrainBrush(1, 1); err != nil {
					return err
				}
				state.capture("unit-flag", app)
				return nil
			},
		},
		{
			Name: "cliff-plateau",
			Apply: func() error {
				if err := app.SetTerrainBrush(shell.BrushCliffRaise); err != nil {
					return err
				}
				if err := app.SetBrushSize(1); err != nil {
					return err
				}
				if err := app.SetBrushStrength(1); err != nil {
					return err
				}
				if err := app.ApplyTerrainBrush(4, 4); err != nil {
					return err
				}
				state.capture("plateau", app)
				return nil
			},
		},
		{
			Name: "cliff-ramp",
			Apply: func() error {
				if err := app.SetTerrainBrush(shell.BrushRamp); err != nil {
					return err
				}
				if err := app.SetBrushStrength(1); err != nil {
					return err
				}
				if err := app.SetBrushRampDirection(shell.RampEast); err != nil {
					return err
				}
				if err := app.ApplyTerrainBrush(3, 4); err != nil {
					return err
				}
				if err := recordPath("low_to_ramp_before_lower", 2, 4, 3, 4); err != nil {
					return err
				}
				if err := recordPath("ramp_to_high_before_lower", 3, 4, 4, 4); err != nil {
					return err
				}
				state.capture("ramp", app)
				return nil
			},
		},
		{
			Name: "cliff-lower-invalidates-ramp",
			Apply: func() error {
				if err := app.SetTerrainBrush(shell.BrushCliffLower); err != nil {
					return err
				}
				if err := app.SetBrushSize(0); err != nil {
					return err
				}
				if err := app.SetBrushStrength(1); err != nil {
					return err
				}
				if err := app.ApplyTerrainBrush(3, 4); err != nil {
					return err
				}
				if err := recordPath("low_to_center_after_lower", 2, 4, 3, 4); err != nil {
					return err
				}
				if err := recordPath("center_to_high_after_lower", 3, 4, 4, 4); err != nil {
					return err
				}
				state.capture("lower-invalidates-ramp", app)
				return nil
			},
		},
		{
			Name: "cliff-border",
			Apply: func() error {
				if err := app.SetTerrainBrush(shell.BrushCliffLower); err != nil {
					return err
				}
				if err := app.SetBrushSize(2); err != nil {
					return err
				}
				if err := app.SetBrushStrength(64); err != nil {
					return err
				}
				if err := app.ApplyTerrainBrush(0, 0); err != nil {
					return err
				}
				state.capture("border", app)
				return nil
			},
		},
	}
}

func runCliffFSV(app *shell.App, outDir string, state *cliffFSVState) error {
	for i, step := range cliffSteps(app, state) {
		if err := step.Apply(); err != nil {
			return err
		}
		if err := renderState(outDir, fmt.Sprintf("%02d-%s.png", i+15, step.Name), app); err != nil {
			return err
		}
	}
	return writeCliffDump(app, outDir, state)
}

func cliffWindowSteps(app *shell.App, outDir string, state *cliffFSVState) []shell.WindowCaptureStep {
	steps := cliffSteps(app, state)
	out := make([]shell.WindowCaptureStep, 0, len(steps))
	for i, step := range steps {
		step := step
		out = append(out, shell.WindowCaptureStep{
			Name:     step.Name,
			ShotPath: filepath.Join(outDir, fmt.Sprintf("%02d-%s.png", i+15, step.Name)),
			BeforeCapture: func() error {
				return step.Apply()
			},
		})
	}
	return out
}

func writeBrushDump(app *shell.App, outDir string) error {
	lowRamp, err := app.CliffStepLegal(3, 6, 4, 6)
	if err != nil {
		return err
	}
	rampHigh, err := app.CliffStepLegal(4, 6, 5, 6)
	if err != nil {
		return err
	}
	snap := app.Snapshot()
	dump := struct {
		HeightRows [][]int                    `json:"heightRows"`
		CliffRows  [][]string                 `json:"cliffRows"`
		Brush      shell.TerrainBrushSnapshot `json:"brush"`
		Stack      shell.StackSnapshot        `json:"stack"`
		Pathable   map[string]bool            `json:"pathable"`
	}{
		HeightRows: snap.World.HeightRows,
		CliffRows:  snap.World.CliffRows,
		Brush:      snap.Brush,
		Stack:      snap.Stack,
		Pathable: map[string]bool{
			"low_to_ramp":  lowRamp,
			"ramp_to_high": rampHigh,
		},
	}
	body, err := json.MarshalIndent(dump, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(outDir, "brush-grid-dump.json"), append(body, '\n'), 0o644)
}

func writeCliffDump(app *shell.App, outDir string, state *cliffFSVState) error {
	if state == nil {
		state = newCliffFSVState()
	}
	snap := app.Snapshot()
	pathable := map[string]bool{}
	for k, v := range state.Pathable {
		pathable[k] = v
	}
	for name, q := range map[string][4]int{
		"final_low_to_center":   {2, 4, 3, 4},
		"final_center_to_high":  {3, 4, 4, 4},
		"final_brush_ramp_low":  {3, 6, 4, 6},
		"final_brush_ramp_high": {4, 6, 5, 6},
	} {
		ok, err := app.CliffStepLegal(q[0], q[1], q[2], q[3])
		if err != nil {
			return err
		}
		pathable[name] = ok
	}
	dump := struct {
		CliffRows     [][]string                           `json:"cliffRows"`
		CliffFlags    []shell.CliffFlagSnapshot            `json:"cliffFlags"`
		StepCliffRows map[string][][]string                `json:"stepCliffRows"`
		StepFlags     map[string][]shell.CliffFlagSnapshot `json:"stepFlags"`
		Brush         shell.TerrainBrushSnapshot           `json:"brush"`
		Stack         shell.StackSnapshot                  `json:"stack"`
		Pathable      map[string]bool                      `json:"pathable"`
		Cells         map[string]string                    `json:"cells"`
	}{
		CliffRows:     snap.World.CliffRows,
		CliffFlags:    snap.CliffFlags,
		StepCliffRows: state.StepCliffRows,
		StepFlags:     state.StepFlags,
		Brush:         snap.Brush,
		Stack:         snap.Stack,
		Pathable:      pathable,
		Cells: map[string]string{
			"unit_1_1":       snap.World.CliffRows[1][1],
			"low_2_4":        snap.World.CliffRows[4][2],
			"ramp_3_4":       snap.World.CliffRows[4][3],
			"plateau_4_4":    snap.World.CliffRows[4][4],
			"border_0_0":     snap.World.CliffRows[0][0],
			"border_1_0":     snap.World.CliffRows[0][1],
			"brush_ramp_4_6": snap.World.CliffRows[6][4],
			"brush_high_5_6": snap.World.CliffRows[6][5],
		},
	}
	body, err := json.MarshalIndent(dump, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(outDir, "cliff-grid-dump.json"), append(body, '\n'), 0o644)
}

func writePaintDump(app *shell.App, outDir, beforeHash, afterHash string, stackOverride *shell.StackSnapshot) error {
	if beforeHash == "" {
		return fmt.Errorf("paint FSV missing pre-paint sim-relevant hash")
	}
	if afterHash == "" {
		afterHash = app.SimRelevantHash()
	}
	snap := app.Snapshot()
	stack := snap.Stack
	if stackOverride != nil {
		stack = *stackOverride
	}
	dump := struct {
		SplatRows             [][][]int                `json:"splatRows"`
		Paint                 shell.PaintBrushSnapshot `json:"paint"`
		Stack                 shell.StackSnapshot      `json:"stack"`
		SimRelevantHashBefore string                   `json:"simRelevantHashBefore"`
		SimRelevantHashAfter  string                   `json:"simRelevantHashAfter"`
		Cells                 map[string][]int         `json:"cells"`
	}{
		SplatRows:             snap.World.SplatRows,
		Paint:                 snap.Paint,
		Stack:                 stack,
		SimRelevantHashBefore: beforeHash,
		SimRelevantHashAfter:  afterHash,
		Cells: map[string][]int{
			"overlap_2_2":  snap.World.SplatRows[2][2],
			"boundary_3_4": snap.World.SplatRows[4][3],
			"boundary_4_4": snap.World.SplatRows[4][4],
			"boundary_5_4": snap.World.SplatRows[4][5],
			"border_0_0":   snap.World.SplatRows[0][0],
			"border_1_0":   snap.World.SplatRows[0][1],
		},
	}
	body, err := json.MarshalIndent(dump, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(outDir, "paint-grid-dump.json"), append(body, '\n'), 0o644)
}

const objectFSVCellWorld = sourceform.TerrainCellWorldUnit

type objectFSVState struct {
	Rejected         objectRejectRecord        `json:"rejected"`
	PlacedUnits      []objectPlacementRecord   `json:"placedUnits"`
	PlacedDoodads    []objectPlacementRecord   `json:"placedDoodads"`
	TransformedUnit  objectTransformRecord     `json:"transformedUnit"`
	ScaleClamp       objectScaleClampRecord    `json:"scaleClamp"`
	DeleteUndo       objectDeleteUndoRecord    `json:"deleteUndo"`
	Palette          []shell.ObjectPaletteItem `json:"palette"`
	Stack            shell.StackSnapshot       `json:"stack"`
	SavedSourceFiles map[string]string         `json:"savedSourceFiles"`
}

type objectRejectRecord struct {
	BeforeUnits   int    `json:"beforeUnits"`
	BeforeDoodads int    `json:"beforeDoodads"`
	AfterUnits    int    `json:"afterUnits"`
	AfterDoodads  int    `json:"afterDoodads"`
	Cell          [2]int `json:"cell"`
	Error         string `json:"error"`
	Status        string `json:"status"`
}

type objectPlacementRecord struct {
	Kind     string `json:"kind"`
	ID       uint32 `json:"id"`
	Type     string `json:"type"`
	Owner    int    `json:"owner,omitempty"`
	Cell     [2]int `json:"cell"`
	Pos      [2]int `json:"pos"`
	Rotation int    `json:"rotation"`
	Scale    int    `json:"scale"`
	Override bool   `json:"override,omitempty"`
}

type objectTransformRecord struct {
	Before objectPlacementRecord `json:"before"`
	After  objectPlacementRecord `json:"after"`
}

type objectScaleClampRecord struct {
	DoodadID       uint32                `json:"doodadID"`
	RequestedScale int                   `json:"requestedScale"`
	Before         objectPlacementRecord `json:"before"`
	After          objectPlacementRecord `json:"after"`
}

type objectDeleteUndoRecord struct {
	BeforeDelete      objectPlacementRecord `json:"beforeDelete"`
	AfterDeleteExists bool                  `json:"afterDeleteExists"`
	AfterUndo         objectPlacementRecord `json:"afterUndo"`
	RestoredIdentical bool                  `json:"restoredIdentical"`
}

type editorObjectStep struct {
	Name  string
	Apply func() error
}

func newObjectFSVState() *objectFSVState {
	return &objectFSVState{SavedSourceFiles: map[string]string{}}
}

func objectSteps(app *shell.App, state *objectFSVState) []editorObjectStep {
	return []editorObjectStep{
		{
			Name: "objects-reject-blocked-pathing",
			Apply: func() error {
				px, py := sourceform.TerrainCellCenterPathingCell(7, 7)
				if err := app.EditPathingFlags(px, py, 0); err != nil {
					return err
				}
				if err := app.SwitchMode(shell.ModeObjects); err != nil {
					return err
				}
				before := app.Snapshot()
				_, err := app.PlaceUnitCell("footman", 3, 7, 7, 0, shell.ClampPlacementScale(1000), false)
				after := app.Snapshot()
				if err == nil {
					return fmt.Errorf("object FSV: blocked pathing unit placement unexpectedly succeeded")
				}
				state.Rejected = objectRejectRecord{
					BeforeUnits:   before.World.Entities,
					BeforeDoodads: before.World.Doodads,
					AfterUnits:    after.World.Entities,
					AfterDoodads:  after.World.Doodads,
					Cell:          [2]int{7, 7},
					Error:         err.Error(),
					Status:        after.Status,
				}
				return nil
			},
		},
		{
			Name: "objects-place-units",
			Apply: func() error {
				if err := app.SwitchMode(shell.ModeObjects); err != nil {
					return err
				}
				u1, err := app.PlaceUnitCell("footman", 0, 1, 2, 1024, 1000, false)
				if err != nil {
					return err
				}
				u2, err := app.PlaceUnitCell("archer", 1, 2, 2, 8192, 1250, false)
				if err != nil {
					return err
				}
				u3, err := app.PlaceUnitCell("footman", 2, 7, 7, 16384, 900, true)
				if err != nil {
					return err
				}
				before := entityRecord(mustEntityRecord(app, u2.ID), false)
				if err := app.TransformEntity(u2.ID, [2]int{5 * objectFSVCellWorld, 2 * objectFSVCellWorld}, 24576, 1100); err != nil {
					return err
				}
				after := entityRecord(mustEntityRecord(app, u2.ID), false)
				state.PlacedUnits = []objectPlacementRecord{
					entityRecord(u1, false),
					after,
					entityRecord(u3, true),
				}
				state.TransformedUnit = objectTransformRecord{Before: before, After: after}
				return nil
			},
		},
		{
			Name: "objects-place-doodads",
			Apply: func() error {
				placements := []struct {
					typ      string
					x, y     int
					rotation int
					scale    int
				}{
					{"kaykit-hexagon/tree_single_A.glb", 0, 3, 0, 1000},
					{"kaykit-hexagon/rock_single_A.glb", 1, 3, 4096, 750},
					{"kaykit-hexagon/barrel.glb", 2, 3, 8192, 1250},
					{"kaykit-hexagon/tree_single_A.glb", 3, 3, 32768, 500},
					{"kaykit-hexagon/rock_single_A.glb", 4, 3, 65535, 1500},
				}
				state.PlacedDoodads = state.PlacedDoodads[:0]
				for _, p := range placements {
					d, err := app.PlaceDoodadCell(p.typ, p.x, p.y, p.rotation, p.scale)
					if err != nil {
						return err
					}
					state.PlacedDoodads = append(state.PlacedDoodads, doodadRecord(d))
				}
				return nil
			},
		},
		{
			Name: "objects-scale-clamp",
			Apply: func() error {
				if len(state.PlacedDoodads) == 0 {
					return fmt.Errorf("object FSV: scale clamp requires placed doodads")
				}
				id := state.PlacedDoodads[0].ID
				before := mustDoodadRecord(app, id)
				if err := app.TransformDoodad(id, before.Pos, before.Rotation, -25); err != nil {
					return err
				}
				after := mustDoodadRecord(app, id)
				state.ScaleClamp = objectScaleClampRecord{
					DoodadID:       id,
					RequestedScale: -25,
					Before:         doodadRecord(before),
					After:          doodadRecord(after),
				}
				for i := range state.PlacedDoodads {
					if state.PlacedDoodads[i].ID == id {
						state.PlacedDoodads[i] = doodadRecord(after)
					}
				}
				return nil
			},
		},
		{
			Name: "objects-delete-undo",
			Apply: func() error {
				if len(state.PlacedDoodads) < 3 {
					return fmt.Errorf("object FSV: delete/undo requires at least three doodads")
				}
				id := state.PlacedDoodads[2].ID
				before := mustDoodadRecord(app, id)
				if err := app.DeleteDoodad(id); err != nil {
					return err
				}
				_, err := doodadRecordByID(app.Snapshot(), id)
				existsAfterDelete := err == nil
				if err := app.Undo(); err != nil {
					return err
				}
				after := mustDoodadRecord(app, id)
				state.DeleteUndo = objectDeleteUndoRecord{
					BeforeDelete:      doodadRecord(before),
					AfterDeleteExists: existsAfterDelete,
					AfterUndo:         doodadRecord(after),
					RestoredIdentical: reflect.DeepEqual(before, after),
				}
				return nil
			},
		},
	}
}

func runObjectFSV(app *shell.App, outDir string, state *objectFSVState) error {
	for i, step := range objectSteps(app, state) {
		if err := step.Apply(); err != nil {
			return err
		}
		if err := renderState(outDir, fmt.Sprintf("%02d-%s.png", i+20, step.Name), app); err != nil {
			return err
		}
	}
	return writeObjectDump(app, outDir, state)
}

func objectWindowSteps(app *shell.App, outDir string, state *objectFSVState) []shell.WindowCaptureStep {
	steps := objectSteps(app, state)
	out := make([]shell.WindowCaptureStep, 0, len(steps))
	for i, step := range steps {
		step := step
		out = append(out, shell.WindowCaptureStep{
			Name:     step.Name,
			ShotPath: filepath.Join(outDir, fmt.Sprintf("%02d-%s.png", i+20, step.Name)),
			BeforeCapture: func() error {
				return step.Apply()
			},
		})
	}
	return out
}

func writeObjectDump(app *shell.App, outDir string, state *objectFSVState) error {
	if state == nil {
		state = newObjectFSVState()
	}
	if err := app.Save(); err != nil {
		return err
	}
	snap := app.Snapshot()
	state.Palette = snap.Objects.Palette
	state.Stack = snap.Stack
	state.SavedSourceFiles = map[string]string{}
	for _, rel := range []string{"map/entities.toml", "map/doodads.toml"} {
		body, err := os.ReadFile(filepath.Join(snap.ProjectPath, filepath.FromSlash(rel)))
		if err != nil {
			return err
		}
		state.SavedSourceFiles[rel] = string(body)
	}
	body, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(outDir, "object-placement-dump.json"), append(body, '\n'), 0o644)
}

type metadataFSVState struct {
	HappyPath        metadataHappyRecord     `json:"happyPath"`
	Duplicate        metadataRejectRecord    `json:"duplicate"`
	Unwalkable       metadataRejectRecord    `json:"unwalkable"`
	EmptyName        metadataEmptyNameRecord `json:"emptyName"`
	Stack            shell.StackSnapshot     `json:"stack"`
	SavedSourceFiles map[string]string       `json:"savedSourceFiles"`
	ArchivePath      string                  `json:"archivePath"`
	RawManifest      string                  `json:"rawManifest"`
	Manifest         worldarchive.Manifest   `json:"manifest"`
	ManifestJSON     string                  `json:"manifestJSON"`
}

type metadataHappyRecord struct {
	Name        string                     `json:"name"`
	Description string                     `json:"description"`
	EngineRange string                     `json:"engineRange"`
	Players     sourceform.Players         `json:"players"`
	Tileset     string                     `json:"tileset"`
	SplatSet    string                     `json:"splatSet"`
	Starts      []sourceform.StartLocation `json:"starts"`
}

type metadataRejectRecord struct {
	BeforeStarts []sourceform.StartLocation `json:"beforeStarts"`
	AfterStarts  []sourceform.StartLocation `json:"afterStarts"`
	Error        string                     `json:"error"`
	Status       string                     `json:"status"`
}

type metadataEmptyNameRecord struct {
	BeforeName string `json:"beforeName"`
	AfterName  string `json:"afterName"`
	Error      string `json:"error"`
	Status     string `json:"status"`
}

type editorMetadataStep struct {
	Name  string
	Apply func() error
}

func newMetadataFSVState() *metadataFSVState {
	return &metadataFSVState{SavedSourceFiles: map[string]string{}}
}

func metadataSteps(app *shell.App, state *metadataFSVState) []editorMetadataStep {
	players := sourceform.Players{Min: 1, Max: 8, Suggested: 4}
	return []editorMetadataStep{
		{
			Name: "metadata-starts",
			Apply: func() error {
				if err := app.SwitchMode(shell.ModeMetadata); err != nil {
					return err
				}
				if err := app.SetMapMetadata("Metadata FSV World", "Four player start FSV", ">=0.1.0 <0.2.0", players, "vigil-lowlands", "dawn-splat"); err != nil {
					return err
				}
				for _, start := range []struct {
					player int
					x, y   int
				}{
					{1, 1, 1},
					{2, 2, 1},
					{3, 1, 2},
					{4, 2, 2},
				} {
					if err := app.PutStartLocationCell(start.player, start.x, start.y); err != nil {
						return err
					}
				}
				snap := app.Snapshot()
				state.HappyPath = metadataHappyRecord{
					Name:        snap.World.Name,
					Description: snap.World.Description,
					EngineRange: snap.World.EngineRange,
					Players:     snap.World.Players,
					Tileset:     snap.World.Tileset,
					SplatSet:    snap.World.SplatSet,
					Starts:      append([]sourceform.StartLocation(nil), snap.World.Starts...),
				}
				return nil
			},
		},
		{
			Name: "metadata-duplicate-player",
			Apply: func() error {
				before := app.Snapshot()
				err := app.AddStartLocationCell(2, 3, 3)
				after := app.Snapshot()
				if err == nil {
					return fmt.Errorf("metadata FSV: duplicate player 2 start unexpectedly succeeded")
				}
				state.Duplicate = metadataRejectRecord{
					BeforeStarts: append([]sourceform.StartLocation(nil), before.World.Starts...),
					AfterStarts:  append([]sourceform.StartLocation(nil), after.World.Starts...),
					Error:        err.Error(),
					Status:       after.Status,
				}
				return nil
			},
		},
		{
			Name: "metadata-reject-unbuildable",
			Apply: func() error {
				px, py := sourceform.TerrainCellCenterPathingCell(7, 7)
				if err := app.EditPathingFlags(px, py, 0); err != nil {
					return err
				}
				if err := app.SwitchMode(shell.ModeMetadata); err != nil {
					return err
				}
				before := app.Snapshot()
				err := app.PutStartLocationCell(5, 7, 7)
				after := app.Snapshot()
				if err == nil {
					return fmt.Errorf("metadata FSV: unbuildable start unexpectedly succeeded")
				}
				state.Unwalkable = metadataRejectRecord{
					BeforeStarts: append([]sourceform.StartLocation(nil), before.World.Starts...),
					AfterStarts:  append([]sourceform.StartLocation(nil), after.World.Starts...),
					Error:        err.Error(),
					Status:       after.Status,
				}
				return nil
			},
		},
		{
			Name: "metadata-empty-name",
			Apply: func() error {
				before := app.Snapshot()
				err := app.SetMapMetadata("", "Four player start FSV", ">=0.1.0 <0.2.0", players, "vigil-lowlands", "dawn-splat")
				after := app.Snapshot()
				if err == nil {
					return fmt.Errorf("metadata FSV: empty map name unexpectedly succeeded")
				}
				state.EmptyName = metadataEmptyNameRecord{
					BeforeName: before.World.Name,
					AfterName:  after.World.Name,
					Error:      err.Error(),
					Status:     after.Status,
				}
				return nil
			},
		},
	}
}

func runMetadataFSV(app *shell.App, outDir string, state *metadataFSVState) error {
	for i, step := range metadataSteps(app, state) {
		if err := step.Apply(); err != nil {
			return err
		}
		if err := renderState(outDir, fmt.Sprintf("%02d-%s.png", i+25, step.Name), app); err != nil {
			return err
		}
	}
	return writeMetadataDump(app, outDir, state)
}

func metadataWindowSteps(app *shell.App, outDir string, state *metadataFSVState) []shell.WindowCaptureStep {
	steps := metadataSteps(app, state)
	out := make([]shell.WindowCaptureStep, 0, len(steps))
	for i, step := range steps {
		step := step
		out = append(out, shell.WindowCaptureStep{
			Name:     step.Name,
			ShotPath: filepath.Join(outDir, fmt.Sprintf("%02d-%s.png", i+25, step.Name)),
			BeforeCapture: func() error {
				return step.Apply()
			},
		})
	}
	return out
}

func writeMetadataDump(app *shell.App, outDir string, state *metadataFSVState) error {
	if state == nil {
		state = newMetadataFSVState()
	}
	if err := app.Save(); err != nil {
		return err
	}
	snap := app.Snapshot()
	state.Stack = snap.Stack
	state.SavedSourceFiles = map[string]string{}
	for _, rel := range []string{"world.toml", "map/terrain.toml"} {
		body, err := os.ReadFile(filepath.Join(snap.ProjectPath, filepath.FromSlash(rel)))
		if err != nil {
			return err
		}
		state.SavedSourceFiles[rel] = string(body)
	}
	archivePath := filepath.Join(outDir, "metadata-starts-fsv.litdworld")
	if err := app.ExportArchive(archivePath); err != nil {
		return err
	}
	state.ArchivePath = archivePath
	rawManifest, err := readRawArchiveManifest(archivePath)
	if err != nil {
		return err
	}
	state.RawManifest = rawManifest
	opened, err := worldarchive.Open(archivePath, "")
	if err != nil {
		return err
	}
	state.Manifest = opened.Manifest
	opened.Close()
	manifestJSON, err := json.MarshalIndent(state.Manifest, "", "  ")
	if err != nil {
		return err
	}
	state.ManifestJSON = string(manifestJSON)
	body, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(outDir, "metadata-starts-dump.json"), append(body, '\n'), 0o644)
}

type archiveRoundTripFSVState struct {
	ArchiveA       string              `json:"archiveA"`
	ArchiveB       string              `json:"archiveB"`
	HashesA        []string            `json:"hashesA"`
	HashesB        []string            `json:"hashesB"`
	Load           archiveSnapshot     `json:"load"`
	Corrupt        archiveErrorRecord  `json:"corrupt"`
	EngineRange    archiveErrorRecord  `json:"engineRange"`
	ReadOnlySave   archiveReadOnlySave `json:"readOnlySave"`
	ShippedArchive archiveSnapshot     `json:"shippedArchive"`
}

type archiveSnapshot struct {
	ProjectPath     string                     `json:"projectPath"`
	ArchivePath     string                     `json:"archivePath"`
	ArchiveReadOnly bool                       `json:"archiveReadOnly"`
	Status          string                     `json:"status"`
	Error           string                     `json:"error,omitempty"`
	Dirty           bool                       `json:"dirty"`
	Name            string                     `json:"name"`
	Width           int                        `json:"width"`
	Height          int                        `json:"height"`
	Starts          []sourceform.StartLocation `json:"starts,omitempty"`
}

type archiveErrorRecord struct {
	Before archiveSnapshot `json:"before"`
	Error  string          `json:"error"`
	After  archiveSnapshot `json:"after"`
}

type archiveReadOnlySave struct {
	Target       string `json:"target"`
	BeforeDirty  bool   `json:"beforeDirty"`
	AfterDirty   bool   `json:"afterDirty"`
	BeforeHeight int    `json:"beforeHeight"`
	AfterHeight  int    `json:"afterHeight"`
	Error        string `json:"error"`
	Status       string `json:"status"`
}

func runArchiveRoundTripFSV(app *shell.App, outDir string) error {
	root := filepath.Join(outDir, "archive-roundtrip")
	if err := os.RemoveAll(root); err != nil {
		return err
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		return err
	}
	state := archiveRoundTripFSVState{}

	if err := app.NewProject(filepath.Join(root, "source")); err != nil {
		return err
	}
	players := sourceform.Players{Min: 1, Max: 8, Suggested: 2}
	if err := app.SetMapMetadata("Archive Round Trip FSV", "save load save", ">=0.1.0 <0.2.0", players, "vigil-lowlands", "dawn-splat"); err != nil {
		return err
	}
	if err := app.EditTerrainHeight(1, 1, 5); err != nil {
		return err
	}
	state.ArchiveA = filepath.Join(root, "a.litdworld")
	if err := app.SaveArchive(state.ArchiveA); err != nil {
		return err
	}
	hashesA, err := archiveMemberHashList(state.ArchiveA)
	if err != nil {
		return err
	}
	state.HashesA = hashesA
	if err := app.OpenArchive(state.ArchiveA, filepath.Join(root, "work")); err != nil {
		return err
	}
	state.Load = archiveSnapshotFrom(app.Snapshot())
	if err := renderState(outDir, "29-archive-load.png", app); err != nil {
		return err
	}
	state.ArchiveB = filepath.Join(root, "b.litdworld")
	if err := app.SaveArchive(state.ArchiveB); err != nil {
		return err
	}
	hashesB, err := archiveMemberHashList(state.ArchiveB)
	if err != nil {
		return err
	}
	state.HashesB = hashesB
	if !reflect.DeepEqual(state.HashesA, state.HashesB) {
		return fmt.Errorf("archive FSV: save-load-save member hashes differ")
	}

	corrupt := filepath.Join(root, "corrupt.litdworld")
	if err := rewriteArchiveEntry(state.ArchiveA, corrupt, "world.toml", func(b []byte) []byte {
		if len(b) > 0 {
			b[0] ^= 0x01
		}
		return b
	}); err != nil {
		return err
	}
	beforeCorrupt := archiveSnapshotFrom(app.Snapshot())
	corruptErr := app.OpenArchive(corrupt, filepath.Join(root, "corrupt-work"))
	state.Corrupt = archiveErrorRecord{Before: beforeCorrupt, Error: errorString(corruptErr), After: archiveSnapshotFrom(app.Snapshot())}
	if corruptErr == nil {
		return fmt.Errorf("archive FSV: corrupt member opened without error")
	}

	if err := app.NewProject(filepath.Join(root, "future-source")); err != nil {
		return err
	}
	if err := app.SetMapMetadata("Future Engine FSV", "range edge", ">=99.0.0", players, "vigil-lowlands", "dawn-splat"); err != nil {
		return err
	}
	futureArchive := filepath.Join(root, "future.litdworld")
	if err := app.SaveArchive(futureArchive); err != nil {
		return err
	}
	beforeRange := archiveSnapshotFrom(app.Snapshot())
	rangeErr := app.OpenArchive(futureArchive, filepath.Join(root, "future-work"))
	state.EngineRange = archiveErrorRecord{Before: beforeRange, Error: errorString(rangeErr), After: archiveSnapshotFrom(app.Snapshot())}
	if rangeErr == nil {
		return fmt.Errorf("archive FSV: incompatible engine range opened without error")
	}

	if err := app.NewProject(filepath.Join(root, "readonly-source")); err != nil {
		return err
	}
	if err := app.EditTerrainHeight(1, 1, 9); err != nil {
		return err
	}
	roDir := filepath.Join(root, "readonly")
	if err := os.MkdirAll(roDir, 0o755); err != nil {
		return err
	}
	if err := os.Chmod(roDir, 0o555); err != nil {
		return err
	}
	beforeRO := app.Snapshot()
	roTarget := filepath.Join(roDir, "blocked.litdworld")
	roErr := app.SaveArchive(roTarget)
	afterRO := app.Snapshot()
	_ = os.Chmod(roDir, 0o755)
	state.ReadOnlySave = archiveReadOnlySave{
		Target:       roTarget,
		BeforeDirty:  beforeRO.Dirty,
		AfterDirty:   afterRO.Dirty,
		BeforeHeight: beforeRO.World.HeightCell,
		AfterHeight:  afterRO.World.HeightCell,
		Error:        errorString(roErr),
		Status:       afterRO.Status,
	}
	if roErr == nil {
		return fmt.Errorf("archive FSV: read-only archive save succeeded unexpectedly")
	}

	shipped := filepath.Join("worlds", "firstflame.litdworld")
	if err := app.OpenProject(shipped); err != nil {
		return err
	}
	state.ShippedArchive = archiveSnapshotFrom(app.Snapshot())
	if err := renderState(outDir, "30-m6-archive-open.png", app); err != nil {
		return err
	}

	body, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(outDir, "archive-roundtrip-dump.json"), append(body, '\n'), 0o644)
}

func archiveSnapshotFrom(s shell.Snapshot) archiveSnapshot {
	return archiveSnapshot{
		ProjectPath:     s.ProjectPath,
		ArchivePath:     s.ArchivePath,
		ArchiveReadOnly: s.ArchiveReadOnly,
		Status:          s.Status,
		Error:           s.Error,
		Dirty:           s.Dirty,
		Name:            s.World.Name,
		Width:           s.World.Width,
		Height:          s.World.Height,
		Starts:          append([]sourceform.StartLocation(nil), s.World.Starts...),
	}
}

type minimapFSVState struct {
	Before       minimapSnapshotRecord `json:"before"`
	After        minimapSnapshotRecord `json:"after"`
	Undo         minimapSnapshotRecord `json:"undo"`
	Click        minimapClickRecord    `json:"click"`
	OutsideClick minimapClickRecord    `json:"outsideClick"`
	Small64      minimapSnapshotRecord `json:"small64"`
	Large256     minimapSnapshotRecord `json:"large256"`
}

type minimapSnapshotRecord struct {
	Screenshot   string                     `json:"screenshot"`
	Rect         string                     `json:"rect"`
	SampleCell   [2]int                     `json:"sampleCell"`
	SamplePixel  [2]int                     `json:"samplePixel"`
	SampleRGBA   [4]uint8                   `json:"sampleRGBA"`
	Starts       []sourceform.StartLocation `json:"starts"`
	CameraTarget [2]int                     `json:"cameraTarget"`
	Width        int                        `json:"width"`
	Height       int                        `json:"height"`
	CliffRows    [][]string                 `json:"cliffRows,omitempty"`
	SavedCliff   string                     `json:"savedCliff,omitempty"`
	SavedTerrain string                     `json:"savedTerrain,omitempty"`
}

type minimapClickRecord struct {
	Click       [2]int `json:"click"`
	Before      [2]int `json:"before"`
	After       [2]int `json:"after"`
	Error       string `json:"error,omitempty"`
	ContentRect string `json:"contentRect"`
}

func runMinimapFSV(app *shell.App, outDir string) error {
	root := filepath.Join(outDir, "minimap-fsv")
	if err := os.RemoveAll(root); err != nil {
		return err
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		return err
	}
	state := minimapFSVState{}
	sample := [2]int{5, 4}

	if err := app.NewProjectWithSize(filepath.Join(root, "preview-source"), 8, 8); err != nil {
		return err
	}
	if err := app.PutStartLocationCell(1, 1, 6); err != nil {
		return err
	}
	if err := app.PutStartLocationCell(2, 6, 1); err != nil {
		return err
	}
	before, err := captureMinimapRecord(app, outDir, "31-minimap-before.png", sample)
	if err != nil {
		return err
	}
	if err := app.Save(); err != nil {
		return err
	}
	if err := fillMinimapSourceBytes(app, &before, true); err != nil {
		return err
	}
	state.Before = before

	if err := app.SetTerrainBrush(shell.BrushCliffRaise); err != nil {
		return err
	}
	if err := app.SetBrushSize(1); err != nil {
		return err
	}
	if err := app.SetBrushStrength(1); err != nil {
		return err
	}
	if err := app.ApplyTerrainBrush(4, 4); err != nil {
		return err
	}
	after, err := captureMinimapRecord(app, outDir, "32-minimap-after.png", sample)
	if err != nil {
		return err
	}
	if err := app.Save(); err != nil {
		return err
	}
	if err := fillMinimapSourceBytes(app, &after, true); err != nil {
		return err
	}
	state.After = after

	if state.Before.SampleRGBA == state.After.SampleRGBA {
		return fmt.Errorf("minimap FSV: plateau sample did not change: %v", state.After.SampleRGBA)
	}
	if err := app.Undo(); err != nil {
		return err
	}
	undo, err := captureMinimapRecord(app, outDir, "33-minimap-undo.png", sample)
	if err != nil {
		return err
	}
	if err := app.Save(); err != nil {
		return err
	}
	if err := fillMinimapSourceBytes(app, &undo, true); err != nil {
		return err
	}
	state.Undo = undo
	if state.Before.SampleRGBA != state.Undo.SampleRGBA {
		return fmt.Errorf("minimap FSV: undo sample %v did not restore before %v", state.Undo.SampleRGBA, state.Before.SampleRGBA)
	}

	rect := shell.MinimapContentRect(app.Snapshot())
	click := [2]int{rect.Min.X, rect.Min.Y}
	beforeClick := app.Snapshot().Camera.TargetCell
	clickErr := app.RecenterCameraFromMinimapPixel(click[0], click[1])
	afterClick := app.Snapshot().Camera.TargetCell
	state.Click = minimapClickRecord{Click: click, Before: beforeClick, After: afterClick, Error: errorString(clickErr), ContentRect: fmt.Sprint(rect)}
	if clickErr != nil || afterClick != ([2]int{0, 0}) {
		return fmt.Errorf("minimap FSV: corner click target=%v err=%v, want 0,0 nil", afterClick, clickErr)
	}
	outside := [2]int{rect.Min.X - 1, rect.Min.Y - 1}
	beforeOutside := afterClick
	outsideErr := app.RecenterCameraFromMinimapPixel(outside[0], outside[1])
	afterOutside := app.Snapshot().Camera.TargetCell
	state.OutsideClick = minimapClickRecord{Click: outside, Before: beforeOutside, After: afterOutside, Error: errorString(outsideErr), ContentRect: fmt.Sprint(rect)}
	if outsideErr == nil || afterOutside != beforeOutside {
		return fmt.Errorf("minimap FSV: outside click err=%v before=%v after=%v", outsideErr, beforeOutside, afterOutside)
	}
	if err := app.Save(); err != nil {
		return err
	}

	if err := app.NewProjectWithSize(filepath.Join(root, "small-64"), 64, 64); err != nil {
		return err
	}
	state.Small64, err = captureMinimapRecord(app, outDir, "34-minimap-64.png", [2]int{10, 10})
	if err != nil {
		return err
	}
	if err := fillMinimapSourceBytes(app, &state.Small64, false); err != nil {
		return err
	}

	if err := app.NewProjectWithSize(filepath.Join(root, "large-256"), 256, 256); err != nil {
		return err
	}
	state.Large256, err = captureMinimapRecord(app, outDir, "35-minimap-256.png", [2]int{10, 10})
	if err != nil {
		return err
	}
	if err := fillMinimapSourceBytes(app, &state.Large256, false); err != nil {
		return err
	}

	body, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(outDir, "minimap-dump.json"), append(body, '\n'), 0o644); err != nil {
		return err
	}
	return app.OpenProject(filepath.Join(root, "preview-source"))
}

type playtestFSVState struct {
	ZeroStarts playtestZeroStartsRecord `json:"zeroStarts"`
	Happy      playtestRoundTripRecord  `json:"happy"`
	Repeat     playtestRepeatRecord     `json:"repeat"`
	Killed     playtestKilledRecord     `json:"killed"`
}

type playtestZeroStartsRecord struct {
	BeforeHash string                     `json:"beforeHash"`
	AfterHash  string                     `json:"afterHash"`
	Before     []sourceform.StartLocation `json:"beforeStarts"`
	After      []sourceform.StartLocation `json:"afterStarts"`
	Error      string                     `json:"error"`
	Status     string                     `json:"status"`
	Screenshot string                     `json:"screenshot"`
	Playtest   shell.PlaytestSnapshot     `json:"playtest"`
}

type playtestRoundTripRecord struct {
	PlacedUnit       sourceform.Entity          `json:"placedUnit"`
	EditorBeforeShot string                     `json:"editorBeforeShot"`
	GameShot         string                     `json:"gameShot"`
	EditorAfterShot  string                     `json:"editorAfterShot"`
	BeforeHash       string                     `json:"beforeHash"`
	AfterHash        string                     `json:"afterHash"`
	BeforeDirty      bool                       `json:"beforeDirty"`
	AfterDirty       bool                       `json:"afterDirty"`
	BeforeUnits      []sourceform.Entity        `json:"beforeUnits"`
	AfterUnits       []sourceform.Entity        `json:"afterUnits"`
	BeforeStarts     []sourceform.StartLocation `json:"beforeStarts"`
	AfterStarts      []sourceform.StartLocation `json:"afterStarts"`
	State            json.RawMessage            `json:"state"`
	Playtest         shell.PlaytestSnapshot     `json:"playtest"`
}

type playtestRepeatRecord struct {
	FirstGameShot   string                 `json:"firstGameShot"`
	SecondGameShot  string                 `json:"secondGameShot"`
	DistinctTempDir bool                   `json:"distinctTempDir"`
	First           shell.PlaytestSnapshot `json:"first"`
	Second          shell.PlaytestSnapshot `json:"second"`
}

type playtestKilledRecord struct {
	EditorAfterShot string                 `json:"editorAfterShot"`
	BeforeHash      string                 `json:"beforeHash"`
	AfterHash       string                 `json:"afterHash"`
	Error           string                 `json:"error"`
	Playtest        shell.PlaytestSnapshot `json:"playtest"`
}

func runPlaytestFSV(app *shell.App, outDir string) error {
	root := filepath.Join(outDir, "playtest-fsv")
	if err := os.RemoveAll(root); err != nil {
		return err
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		return err
	}
	state := playtestFSVState{}

	if err := app.NewProjectWithSize(filepath.Join(root, "zero-start-source"), 8, 8); err != nil {
		return err
	}
	app.SetStartLocationsForFSV(nil)
	zeroBefore := app.Snapshot()
	zeroBeforeHash := app.SimRelevantHash()
	zeroRec, zeroErr := app.Playtest(playtestFSVOptions(root, filepath.Join(outDir, "36-playtest-zero-game.png"), 1, 0))
	zeroAfter := app.Snapshot()
	zeroShot := filepath.Join(outDir, "36-playtest-zero-starts.png")
	if err := renderState(outDir, "36-playtest-zero-starts.png", app); err != nil {
		return err
	}
	if zeroErr == nil || !strings.Contains(zeroErr.Error(), "start location") {
		return fmt.Errorf("playtest FSV: zero-start playtest should fail closed, got err=%v", zeroErr)
	}
	if zeroRec.TempDir != "" {
		return fmt.Errorf("playtest FSV: zero-start playtest created temp dir %q", zeroRec.TempDir)
	}
	if zeroBeforeHash != app.SimRelevantHash() || zeroRec.StateHashAfter != zeroBeforeHash {
		return fmt.Errorf("playtest FSV: zero-start hash changed before=%s recAfter=%s after=%s", zeroBeforeHash, zeroRec.StateHashAfter, app.SimRelevantHash())
	}
	state.ZeroStarts = playtestZeroStartsRecord{
		BeforeHash: zeroBeforeHash,
		AfterHash:  app.SimRelevantHash(),
		Before:     append([]sourceform.StartLocation(nil), zeroBefore.World.Starts...),
		After:      append([]sourceform.StartLocation(nil), zeroAfter.World.Starts...),
		Error:      errorString(zeroErr),
		Status:     zeroAfter.Status,
		Screenshot: zeroShot,
		Playtest:   zeroRec,
	}

	happySource := filepath.Join(root, "playtest-source")
	if err := app.NewProjectWithSize(happySource, 8, 8); err != nil {
		return err
	}
	placed, err := app.PlaceUnitCell("archer", 1, 2, 2, 8192, 1250, false)
	if err != nil {
		return err
	}
	if err := app.SwitchMode(shell.ModeObjects); err != nil {
		return err
	}
	before := app.Snapshot()
	beforeHash := app.SimRelevantHash()
	if err := renderState(outDir, "37-playtest-editor-before.png", app); err != nil {
		return err
	}
	happyShot := filepath.Join(outDir, "38-playtest-game.png")
	happyRec, err := app.Playtest(playtestFSVOptions(root, happyShot, 80, 0))
	if err != nil {
		return fmt.Errorf("playtest FSV happy: %w\nstdout=%s\nstderr=%s", err, happyRec.Stdout, happyRec.Stderr)
	}
	after := app.Snapshot()
	afterHash := app.SimRelevantHash()
	if err := renderState(outDir, "39-playtest-editor-after.png", app); err != nil {
		return err
	}
	if beforeHash != afterHash || !before.Dirty || !after.Dirty {
		return fmt.Errorf("playtest FSV: editor dirty state/hash not preserved dirty %v->%v hash %s->%s", before.Dirty, after.Dirty, beforeHash, afterHash)
	}
	if err := assertPathRemoved(happyRec.TempDir); err != nil {
		return err
	}
	stateLine, err := playtestStateLine(happyRec.Stdout)
	if err != nil {
		return err
	}
	state.Happy = playtestRoundTripRecord{
		PlacedUnit:       placed,
		EditorBeforeShot: filepath.Join(outDir, "37-playtest-editor-before.png"),
		GameShot:         happyShot,
		EditorAfterShot:  filepath.Join(outDir, "39-playtest-editor-after.png"),
		BeforeHash:       beforeHash,
		AfterHash:        afterHash,
		BeforeDirty:      before.Dirty,
		AfterDirty:       after.Dirty,
		BeforeUnits:      append([]sourceform.Entity(nil), before.Objects.Units...),
		AfterUnits:       append([]sourceform.Entity(nil), after.Objects.Units...),
		BeforeStarts:     append([]sourceform.StartLocation(nil), before.World.Starts...),
		AfterStarts:      append([]sourceform.StartLocation(nil), after.World.Starts...),
		State:            stateLine,
		Playtest:         happyRec,
	}

	firstShot := filepath.Join(outDir, "40-playtest-repeat-first.png")
	first, err := app.Playtest(playtestFSVOptions(root, firstShot, 2, 0))
	if err != nil {
		return fmt.Errorf("playtest FSV first repeat: %w\nstdout=%s\nstderr=%s", err, first.Stdout, first.Stderr)
	}
	secondShot := filepath.Join(outDir, "41-playtest-repeat-second.png")
	second, err := app.Playtest(playtestFSVOptions(root, secondShot, 2, 0))
	if err != nil {
		return fmt.Errorf("playtest FSV second repeat: %w\nstdout=%s\nstderr=%s", err, second.Stdout, second.Stderr)
	}
	if first.TempDir == second.TempDir {
		return fmt.Errorf("playtest FSV: repeat playtests reused temp dir %q", first.TempDir)
	}
	if err := assertPathRemoved(first.TempDir); err != nil {
		return err
	}
	if err := assertPathRemoved(second.TempDir); err != nil {
		return err
	}
	state.Repeat = playtestRepeatRecord{
		FirstGameShot:   firstShot,
		SecondGameShot:  secondShot,
		DistinctTempDir: true,
		First:           first,
		Second:          second,
	}

	killBeforeHash := app.SimRelevantHash()
	killed, killErr := app.Playtest(playtestFSVOptions(root, filepath.Join(outDir, "42-playtest-killed-game.png"), 1_000_000, 50*time.Millisecond))
	killAfterHash := app.SimRelevantHash()
	if err := renderState(outDir, "42-playtest-killed-after.png", app); err != nil {
		return err
	}
	if killErr == nil || !killed.Killed {
		return fmt.Errorf("playtest FSV: killed playtest should return killed process error, got err=%v killed=%v", killErr, killed.Killed)
	}
	if killBeforeHash != killAfterHash || killed.StateHashAfter != killBeforeHash {
		return fmt.Errorf("playtest FSV: killed playtest changed editor state before=%s recAfter=%s after=%s", killBeforeHash, killed.StateHashAfter, killAfterHash)
	}
	if err := assertPathRemoved(killed.TempDir); err != nil {
		return err
	}
	state.Killed = playtestKilledRecord{
		EditorAfterShot: filepath.Join(outDir, "42-playtest-killed-after.png"),
		BeforeHash:      killBeforeHash,
		AfterHash:       killAfterHash,
		Error:           errorString(killErr),
		Playtest:        killed,
	}

	body, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(outDir, "playtest-dump.json"), append(body, '\n'), 0o644)
}

type m8EndToEndFSVState struct {
	SourceDir                string           `json:"sourceDir"`
	Archive                  string           `json:"archive"`
	ArchiveSHA256            string           `json:"archiveSha256"`
	ReopenedArchive          string           `json:"reopenedArchive"`
	ReopenedArchiveSHA256    string           `json:"reopenedArchiveSha256"`
	SourceHashes             []fileHashRecord `json:"sourceHashes"`
	ArchiveHashes            []string         `json:"archiveHashes"`
	ReopenedArchiveHashes    []string         `json:"reopenedArchiveHashes"`
	ArchiveHashesEqual       bool             `json:"archiveHashesEqual"`
	Screenshots              []screenshotInfo `json:"screenshots"`
	Initial                  m8GameRunRecord  `json:"initial"`
	SecondIndependentLoad    m8GameRunRecord  `json:"secondIndependentLoad"`
	Played                   m8GameRunRecord  `json:"played"`
	Replay                   m8GameRunRecord  `json:"replay"`
	MovementVerified         bool             `json:"movementVerified"`
	CombatResolved           bool             `json:"combatResolved"`
	LocalIndependentHashEdge bool             `json:"localIndependentHashEdge"`
	Notes                    []string         `json:"notes,omitempty"`
}

type fileHashRecord struct {
	Path   string `json:"path"`
	SHA256 string `json:"sha256"`
	Size   int64  `json:"size"`
}

type screenshotInfo struct {
	Path   string `json:"path"`
	SHA256 string `json:"sha256"`
	Size   int64  `json:"size"`
	Width  int    `json:"width"`
	Height int    `json:"height"`
}

type m8GameRunRecord struct {
	Name       string         `json:"name"`
	Command    []string       `json:"command"`
	ExitCode   int            `json:"exitCode"`
	Stdout     string         `json:"stdout"`
	Stderr     string         `json:"stderr"`
	State      m8GameState    `json:"state"`
	Screenshot screenshotInfo `json:"screenshot,omitempty"`
}

type m8GameState struct {
	TimeOfDay float64       `json:"tod"`
	Ticks     int           `json:"ticks"`
	StateHash string        `json:"stateHash"`
	UnitCount int           `json:"unitCount"`
	Alive     int           `json:"alive"`
	Order     m8OrderState  `json:"order"`
	Units     []m8UnitState `json:"units"`
}

type m8OrderState struct {
	Issued bool   `json:"issued"`
	UnitID uint32 `json:"unitId"`
	Before m8Vec2 `json:"before"`
	Target m8Vec2 `json:"target"`
}

type m8Vec2 struct {
	X float64 `json:"x"`
	Y float64 `json:"y"`
}

type m8UnitState struct {
	ID     uint32  `json:"id"`
	Owner  int     `json:"owner"`
	X      float64 `json:"x"`
	Y      float64 `json:"y"`
	Facing float64 `json:"facing"`
	Life   float64 `json:"life"`
	Alive  bool    `json:"alive"`
}

func runM8EndToEndFSV(app *shell.App, outDir string) error {
	root := filepath.Join(outDir, "m8-e2e")
	if err := os.RemoveAll(root); err != nil {
		return err
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		return err
	}
	state := m8EndToEndFSVState{
		SourceDir: filepath.Join(root, "source"),
		Archive:   filepath.Join(root, "editor-e2e.litdworld"),
		Notes: []string{
			"Autotest performs two independent local production-loader processes; the second-machine/OS edge remains a manual runbook step.",
			"Runtime sim pathing currently consumes data/ placement and scripts from the archive; source-form map terrain/cliff bytes are verified as archive SoT.",
		},
	}

	if err := createFreshM8Project(app, state.SourceDir); err != nil {
		return err
	}
	if err := authorM8EndToEndMap(app); err != nil {
		return err
	}
	if err := app.InstallPlayableRuntime(); err != nil {
		return err
	}
	if err := app.Save(); err != nil {
		return err
	}
	if err := renderState(outDir, "43-m8-e2e-authored.png", app); err != nil {
		return err
	}
	shot, err := inspectScreenshot(filepath.Join(outDir, "43-m8-e2e-authored.png"))
	if err != nil {
		return err
	}
	state.Screenshots = append(state.Screenshots, shot)
	hashes, err := sourceFileHashList(state.SourceDir, m8SourceRelPaths())
	if err != nil {
		return err
	}
	state.SourceHashes = hashes

	if err := app.SaveArchive(state.Archive); err != nil {
		return err
	}
	state.ArchiveSHA256, err = sha256Path(state.Archive)
	if err != nil {
		return err
	}
	state.ArchiveHashes, err = archiveMemberHashList(state.Archive)
	if err != nil {
		return err
	}

	reopenDir := filepath.Join(root, "reopened-source")
	if err := app.OpenArchive(state.Archive, reopenDir); err != nil {
		return err
	}
	if err := renderState(outDir, "44-m8-e2e-reopened.png", app); err != nil {
		return err
	}
	shot, err = inspectScreenshot(filepath.Join(outDir, "44-m8-e2e-reopened.png"))
	if err != nil {
		return err
	}
	state.Screenshots = append(state.Screenshots, shot)
	state.ReopenedArchive = filepath.Join(root, "editor-e2e-reopened.litdworld")
	if err := app.SaveArchive(state.ReopenedArchive); err != nil {
		return err
	}
	state.ReopenedArchiveSHA256, err = sha256Path(state.ReopenedArchive)
	if err != nil {
		return err
	}
	state.ReopenedArchiveHashes, err = archiveMemberHashList(state.ReopenedArchive)
	if err != nil {
		return err
	}
	state.ArchiveHashesEqual = reflect.DeepEqual(state.ArchiveHashes, state.ReopenedArchiveHashes)
	if !state.ArchiveHashesEqual {
		return fmt.Errorf("m8 e2e FSV: reopened archive member hashes differ")
	}

	state.Initial, err = runM8GameFSV("initial", state.Archive, filepath.Join(outDir, "45-m8-e2e-initial.png"), 0, false, 0, 0)
	if err != nil {
		return err
	}
	state.Screenshots = append(state.Screenshots, state.Initial.Screenshot)
	state.SecondIndependentLoad, err = runM8GameFSV("second-independent-load", state.Archive, "", 0, false, 0, 0)
	if err != nil {
		return err
	}
	state.Played, err = runM8GameFSV("played", state.Archive, filepath.Join(outDir, "46-m8-e2e-played.png"), 700, true, 128, 0)
	if err != nil {
		return err
	}
	state.Screenshots = append(state.Screenshots, state.Played.Screenshot)
	state.Replay, err = runM8GameFSV("replay", state.Archive, filepath.Join(outDir, "47-m8-e2e-replay.png"), 700, true, 128, 0)
	if err != nil {
		return err
	}
	state.Screenshots = append(state.Screenshots, state.Replay.Screenshot)

	if state.Initial.State.UnitCount != 5 || state.Initial.State.Alive != 5 {
		return fmt.Errorf("m8 e2e FSV: initial unit state count/alive=%d/%d, want 5/5", state.Initial.State.UnitCount, state.Initial.State.Alive)
	}
	state.LocalIndependentHashEdge = state.SecondIndependentLoad.State.StateHash == state.Initial.State.StateHash
	if !state.LocalIndependentHashEdge {
		return fmt.Errorf("m8 e2e FSV: independent local load hash %s, want %s", state.SecondIndependentLoad.State.StateHash, state.Initial.State.StateHash)
	}
	if !state.Played.State.Order.Issued {
		return fmt.Errorf("m8 e2e FSV: played run did not issue deterministic order")
	}
	if state.Played.State.StateHash == state.Initial.State.StateHash {
		return fmt.Errorf("m8 e2e FSV: played hash did not change from initial %s", state.Initial.State.StateHash)
	}
	if state.Replay.State.StateHash != state.Played.State.StateHash {
		return fmt.Errorf("m8 e2e FSV: replay hash %s, want played hash %s", state.Replay.State.StateHash, state.Played.State.StateHash)
	}
	state.MovementVerified = m8OrderMovedCloser(state.Played.State)
	if !state.MovementVerified {
		return fmt.Errorf("m8 e2e FSV: ordered unit did not move closer to target: %+v", state.Played.State.Order)
	}
	state.CombatResolved = m8CombatResolved(state.Initial.State, state.Played.State)
	if !state.CombatResolved {
		return fmt.Errorf("m8 e2e FSV: combat cluster did not change life/alive state")
	}

	body, err := json.MarshalIndent(state, "", "  ")
	if err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(root, "m8-e2e-dump.json"), append(body, '\n'), 0o644); err != nil {
		return err
	}
	fmt.Printf("event: m8-e2e archive=%s hash=%s initial=%s played=%s replay=%s\n", state.Archive, state.ArchiveSHA256, state.Initial.State.StateHash, state.Played.State.StateHash, state.Replay.State.StateHash)
	return nil
}

func authorM8EndToEndMap(app *shell.App) error {
	if err := clearM8DefaultObjects(app); err != nil {
		return err
	}
	players := sourceform.Players{Min: 2, Max: 2, Suggested: 2}
	if err := app.SetMapMetadata("Editor End-to-End FSV", "M8 authored save-load-play verification", ">=0.1.0 <0.2.0", players, "vigil-lowlands", "dawn-splat"); err != nil {
		return err
	}
	if err := app.SwitchMode(shell.ModeTerrain); err != nil {
		return err
	}
	if err := app.SetTerrainBrush(shell.BrushRaise); err != nil {
		return err
	}
	if err := app.SetBrushSize(1); err != nil {
		return err
	}
	if err := app.SetBrushStrength(2); err != nil {
		return err
	}
	if err := app.ApplyTerrainBrush(2, 2); err != nil {
		return err
	}
	if err := app.SetTerrainBrush(shell.BrushLevel); err != nil {
		return err
	}
	app.SetBrushLevelTarget(3)
	if err := app.SetBrushSize(0); err != nil {
		return err
	}
	if err := app.ApplyTerrainBrush(5, 5); err != nil {
		return err
	}
	if err := app.SetTerrainBrush(shell.BrushRamp); err != nil {
		return err
	}
	if err := app.SetBrushRampDirection(shell.RampEast); err != nil {
		return err
	}
	if err := app.SetBrushStrength(1); err != nil {
		return err
	}
	if err := app.ApplyTerrainBrush(3, 4); err != nil {
		return err
	}
	if err := app.SetTerrainBrush(shell.BrushCliffRaise); err != nil {
		return err
	}
	if err := app.SetBrushSize(0); err != nil {
		return err
	}
	if err := app.ApplyTerrainBrush(6, 2); err != nil {
		return err
	}
	if err := app.SetTerrainBrush(shell.BrushCliffLevel); err != nil {
		return err
	}
	app.SetBrushLevelTarget(1)
	if err := app.ApplyTerrainBrush(6, 3); err != nil {
		return err
	}
	if err := app.SetPaintLayer(1); err != nil {
		return err
	}
	if err := app.SetPaintSize(1); err != nil {
		return err
	}
	if err := app.ApplyPaintStroke([][2]int{{1, 1}, {2, 1}, {2, 2}}); err != nil {
		return err
	}
	if err := app.SetPaintLayer(2); err != nil {
		return err
	}
	if err := app.ApplyPaintStroke([][2]int{{5, 5}, {6, 5}, {6, 6}}); err != nil {
		return err
	}
	if _, err := app.PlaceDoodadCell("kaykit-hexagon/tree_single_A.glb", 1, 6, 0, 1000); err != nil {
		return err
	}
	if _, err := app.PlaceDoodadCell("kaykit-hexagon/rock_single_A.glb", 2, 6, 8192, 1000); err != nil {
		return err
	}
	if _, err := app.PlaceDoodadCell("kaykit-hexagon/barrel.glb", 3, 6, 16384, 1000); err != nil {
		return err
	}
	if _, err := app.PlaceUnitCell("footman", 0, 1, 1, 0, 1000, false); err != nil {
		return err
	}
	u1, err := app.PlaceUnitCell("footman", 0, 5, 5, 0, 1000, false)
	if err != nil {
		return err
	}
	u2, err := app.PlaceUnitCell("archer", 0, 5, 6, 0, 1000, false)
	if err != nil {
		return err
	}
	u3, err := app.PlaceUnitCell("footman", 1, 6, 5, 32768, 1000, false)
	if err != nil {
		return err
	}
	u4, err := app.PlaceUnitCell("archer", 1, 6, 6, 32768, 1000, false)
	if err != nil {
		return err
	}
	for _, move := range []struct {
		id  uint32
		pos [2]int
		rot int
	}{
		{id: u1.ID, pos: [2]int{640, 640}, rot: 0},
		{id: u2.ID, pos: [2]int{640, 680}, rot: 0},
		{id: u3.ID, pos: [2]int{690, 640}, rot: 32768},
		{id: u4.ID, pos: [2]int{690, 680}, rot: 32768},
	} {
		if err := app.TransformEntity(move.id, move.pos, move.rot, 1000); err != nil {
			return err
		}
	}
	if err := app.PutStartLocationCell(1, 1, 1); err != nil {
		return err
	}
	if err := app.PutStartLocationCell(2, 6, 6); err != nil {
		return err
	}
	return app.SwitchMode(shell.ModeObjects)
}

func clearM8DefaultObjects(app *shell.App) error {
	snap := app.Snapshot()
	for _, ent := range snap.Objects.Units {
		if err := app.DeleteEntity(ent.ID); err != nil {
			return err
		}
	}
	for _, d := range snap.Objects.Doodads {
		if err := app.DeleteDoodad(d.ID); err != nil {
			return err
		}
	}
	return nil
}

func createFreshM8Project(app *shell.App, dir string) error {
	if err := app.NewProject(dir); err != nil {
		return err
	}
	if err := app.ConfirmPending(); err != nil && !strings.Contains(err.Error(), "no confirmation pending") {
		return err
	}
	return nil
}

func m8SourceRelPaths() []string {
	return []string{
		"world.toml",
		"map/terrain.toml",
		"map/pathing.txt",
		"map/height.txt",
		"map/cliff.txt",
		"map/splat.txt",
		"map/entities.toml",
		"map/doodads.toml",
		"data/combat/damage-table.toml",
		"data/units/editor.toml",
		"data/placement/editor.toml",
		"scripts/main.lua",
	}
}

func runM8GameFSV(name, archive, shot string, ticks int, order bool, dx, dy float64) (m8GameRunRecord, error) {
	args := []string{"run", "./cmd/litd", "-archive", archive, "-autotest", "-ticks", fmt.Sprint(ticks)}
	if order {
		args = append(args, "-autotest-order", "-autotest-order-dx", fmt.Sprintf("%g", dx), "-autotest-order-dy", fmt.Sprintf("%g", dy))
	}
	if shot != "" {
		args = append(args, "-shot", shot)
	}
	cmd := exec.Command("go", args...)
	cmd.Dir = "."
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	exitCode := 0
	if err != nil {
		exitCode = 1
		if exitErr, ok := err.(*exec.ExitError); ok {
			exitCode = exitErr.ExitCode()
		}
	}
	rec := m8GameRunRecord{
		Name:     name,
		Command:  append([]string{"go"}, args...),
		ExitCode: exitCode,
		Stdout:   stdout.String(),
		Stderr:   stderr.String(),
	}
	if err != nil {
		return rec, fmt.Errorf("m8 e2e FSV %s: %w\nstdout=%s\nstderr=%s", name, err, rec.Stdout, rec.Stderr)
	}
	raw, err := playtestStateLine(rec.Stdout)
	if err != nil {
		return rec, err
	}
	if err := json.Unmarshal(raw, &rec.State); err != nil {
		return rec, err
	}
	if shot != "" {
		rec.Screenshot, err = inspectScreenshot(shot)
		if err != nil {
			return rec, err
		}
	}
	return rec, nil
}

func sourceFileHashList(root string, rels []string) ([]fileHashRecord, error) {
	records := make([]fileHashRecord, 0, len(rels))
	for _, rel := range rels {
		abs := filepath.Join(root, rel)
		sum, err := sha256Path(abs)
		if err != nil {
			return nil, err
		}
		st, err := os.Stat(abs)
		if err != nil {
			return nil, err
		}
		records = append(records, fileHashRecord{Path: rel, SHA256: sum, Size: st.Size()})
	}
	return records, nil
}

func inspectScreenshot(path string) (screenshotInfo, error) {
	sum, err := sha256Path(path)
	if err != nil {
		return screenshotInfo{}, err
	}
	st, err := os.Stat(path)
	if err != nil {
		return screenshotInfo{}, err
	}
	f, err := os.Open(path)
	if err != nil {
		return screenshotInfo{}, err
	}
	defer f.Close()
	cfg, err := png.DecodeConfig(f)
	if err != nil {
		return screenshotInfo{}, err
	}
	return screenshotInfo{Path: path, SHA256: sum, Size: st.Size(), Width: cfg.Width, Height: cfg.Height}, nil
}

func sha256Path(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func m8OrderMovedCloser(state m8GameState) bool {
	if !state.Order.Issued || state.Order.UnitID == 0 {
		return false
	}
	for _, unit := range state.Units {
		if unit.ID != state.Order.UnitID {
			continue
		}
		before := m8DistSq(state.Order.Before.X, state.Order.Before.Y, state.Order.Target.X, state.Order.Target.Y)
		after := m8DistSq(unit.X, unit.Y, state.Order.Target.X, state.Order.Target.Y)
		return after < before
	}
	return false
}

func m8CombatResolved(before, after m8GameState) bool {
	if after.Alive < before.Alive || after.UnitCount < before.UnitCount {
		return true
	}
	beforeLife := map[uint32]float64{}
	for _, unit := range before.Units {
		beforeLife[unit.ID] = unit.Life
	}
	for _, unit := range after.Units {
		if life, ok := beforeLife[unit.ID]; ok && unit.Life < life-0.5 {
			return true
		}
	}
	return false
}

func m8DistSq(ax, ay, bx, by float64) float64 {
	dx, dy := ax-bx, ay-by
	return dx*dx + dy*dy
}

func playtestFSVOptions(tempRoot, shot string, ticks int, killAfter time.Duration) shell.PlaytestOptions {
	return shell.PlaytestOptions{
		TempRoot:  tempRoot,
		Dir:       ".",
		ShotPath:  shot,
		Timeout:   45 * time.Second,
		KillAfter: killAfter,
		Command: []string{
			"go", "run", "./cmd/litd",
			"-archive", "{{archive}}",
			"-autotest",
			"-autotest-order",
			"-ticks", fmt.Sprint(ticks),
			"-shot", "{{shot}}",
		},
	}
}

func playtestStateLine(stdout string) (json.RawMessage, error) {
	for _, line := range strings.Split(stdout, "\n") {
		if !strings.HasPrefix(line, "state: ") {
			continue
		}
		raw := json.RawMessage(strings.TrimPrefix(line, "state: "))
		var probe any
		if err := json.Unmarshal(raw, &probe); err != nil {
			return nil, err
		}
		return raw, nil
	}
	return nil, fmt.Errorf("playtest FSV: child stdout had no state line: %s", stdout)
}

func assertPathRemoved(path string) error {
	if path == "" {
		return fmt.Errorf("playtest FSV: empty temp dir path")
	}
	if _, err := os.Stat(path); err == nil {
		return fmt.Errorf("playtest FSV: temp dir still exists: %s", path)
	} else if !os.IsNotExist(err) {
		return err
	}
	return nil
}

func captureMinimapRecord(app *shell.App, outDir, shotName string, sample [2]int) (minimapSnapshotRecord, error) {
	if err := renderState(outDir, shotName, app); err != nil {
		return minimapSnapshotRecord{}, err
	}
	snap := app.Snapshot()
	px, py, ok := shell.MinimapScreenPointForCell(snap, sample[0], sample[1])
	if !ok {
		return minimapSnapshotRecord{}, fmt.Errorf("minimap FSV: sample cell %d,%d outside %dx%d map", sample[0], sample[1], snap.World.Width, snap.World.Height)
	}
	img := shell.RenderImage(snap)
	c := img.RGBAAt(px, py)
	rec := minimapSnapshotRecord{
		Screenshot:   filepath.Join(outDir, shotName),
		Rect:         fmt.Sprint(shell.MinimapContentRect(snap)),
		SampleCell:   sample,
		SamplePixel:  [2]int{px, py},
		SampleRGBA:   [4]uint8{c.R, c.G, c.B, c.A},
		Starts:       append([]sourceform.StartLocation(nil), snap.World.Starts...),
		CameraTarget: snap.Camera.TargetCell,
		Width:        snap.World.Width,
		Height:       snap.World.Height,
	}
	if snap.World.Width <= 16 && snap.World.Height <= 16 {
		rec.CliffRows = snap.World.CliffRows
	}
	return rec, nil
}

func fillMinimapSourceBytes(app *shell.App, rec *minimapSnapshotRecord, includeCliff bool) error {
	if rec == nil {
		return nil
	}
	snap := app.Snapshot()
	if snap.ProjectPath == "" {
		return nil
	}
	if includeCliff {
		body, err := os.ReadFile(filepath.Join(snap.ProjectPath, "map", "cliff.txt"))
		if err != nil {
			return err
		}
		rec.SavedCliff = string(body)
	}
	body, err := os.ReadFile(filepath.Join(snap.ProjectPath, "map", "terrain.toml"))
	if err != nil {
		return err
	}
	rec.SavedTerrain = string(body)
	return nil
}

func archiveMemberHashList(path string) ([]string, error) {
	opened, err := worldarchive.Open(path, "")
	if err != nil {
		return nil, err
	}
	defer opened.Close()
	lines := make([]string, 0, len(opened.Manifest.Files))
	for rel, entry := range opened.Manifest.Files {
		lines = append(lines, fmt.Sprintf("%s %s %d", rel, entry.Hash, entry.Size))
	}
	sort.Strings(lines)
	return lines, nil
}

func rewriteArchiveEntry(src, dst, target string, mutate func([]byte) []byte) error {
	zr, err := zip.OpenReader(src)
	if err != nil {
		return err
	}
	defer zr.Close()
	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	zw := zip.NewWriter(out)
	rewrote := false
	for _, f := range zr.File {
		rc, err := f.Open()
		if err != nil {
			zw.Close()
			out.Close()
			return err
		}
		body, err := io.ReadAll(rc)
		rc.Close()
		if err != nil {
			zw.Close()
			out.Close()
			return err
		}
		w, err := zw.Create(f.Name)
		if err != nil {
			zw.Close()
			out.Close()
			return err
		}
		if f.Name == target {
			body = mutate(body)
			rewrote = true
		}
		if _, err := w.Write(body); err != nil {
			zw.Close()
			out.Close()
			return err
		}
	}
	if err := zw.Close(); err != nil {
		out.Close()
		return err
	}
	if err := out.Close(); err != nil {
		return err
	}
	if !rewrote {
		return fmt.Errorf("archive entry %q not found", target)
	}
	return nil
}

func errorString(err error) string {
	if err == nil {
		return ""
	}
	return err.Error()
}

func readRawArchiveManifest(path string) (string, error) {
	zr, err := zip.OpenReader(path)
	if err != nil {
		return "", err
	}
	defer zr.Close()
	for _, f := range zr.File {
		if f.Name != ".litdworld-manifest" {
			continue
		}
		rc, err := f.Open()
		if err != nil {
			return "", err
		}
		body, err := io.ReadAll(rc)
		rc.Close()
		if err != nil {
			return "", err
		}
		return string(body), nil
	}
	return "", fmt.Errorf("%s has no .litdworld-manifest", path)
}

func entityRecord(ent sourceform.Entity, override bool) objectPlacementRecord {
	return objectPlacementRecord{
		Kind:     "unit",
		ID:       ent.ID,
		Type:     ent.Type,
		Owner:    ent.Player,
		Cell:     [2]int{ent.Pos[0] / objectFSVCellWorld, ent.Pos[1] / objectFSVCellWorld},
		Pos:      ent.Pos,
		Rotation: ent.Rotation,
		Scale:    ent.Scale,
		Override: override,
	}
}

func doodadRecord(d sourceform.Doodad) objectPlacementRecord {
	return objectPlacementRecord{
		Kind:     "doodad",
		ID:       d.ID,
		Type:     d.Type,
		Cell:     [2]int{d.Pos[0] / objectFSVCellWorld, d.Pos[1] / objectFSVCellWorld},
		Pos:      d.Pos,
		Rotation: d.Rotation,
		Scale:    d.Scale,
	}
}

func mustEntityRecord(app *shell.App, id uint32) sourceform.Entity {
	ent, err := entityRecordByID(app.Snapshot(), id)
	if err != nil {
		panic(err)
	}
	return ent
}

func mustDoodadRecord(app *shell.App, id uint32) sourceform.Doodad {
	d, err := doodadRecordByID(app.Snapshot(), id)
	if err != nil {
		panic(err)
	}
	return d
}

func entityRecordByID(snap shell.Snapshot, id uint32) (sourceform.Entity, error) {
	for _, ent := range snap.Objects.Units {
		if ent.ID == id {
			return ent, nil
		}
	}
	return sourceform.Entity{}, fmt.Errorf("entity %d not found", id)
}

func doodadRecordByID(snap shell.Snapshot, id uint32) (sourceform.Doodad, error) {
	for _, d := range snap.Objects.Doodads {
		if d.ID == id {
			return d, nil
		}
	}
	return sourceform.Doodad{}, fmt.Errorf("doodad %d not found", id)
}

func renderState(outDir, name string, app *shell.App) error {
	path := filepath.Join(outDir, name)
	if err := shell.RenderPNG(path, app.Snapshot()); err != nil {
		return err
	}
	fmt.Printf("event: screenshot saved path=%s mode=%s dirty=%v\n", path, app.Snapshot().Mode, app.Snapshot().Dirty)
	return nil
}

func fatalf(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "litd-editor: "+format+"\n", args...)
	os.Exit(1)
}
