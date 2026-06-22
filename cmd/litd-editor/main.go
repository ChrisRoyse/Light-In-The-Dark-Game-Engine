// Command litd-editor is the M8 editor application shell (#125).
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/cmd/litd-editor/shell"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
)

func main() {
	autotest := flag.Bool("autotest", false, "run scripted FSV sequence and write mode screenshots")
	headless := flag.Bool("headless", false, "write one shell screenshot and exit without opening the G3N window")
	capture := flag.Bool("capture", false, "open the G3N window, capture one framebuffer screenshot, then exit")
	outDir := flag.String("out", "artifacts/litd-editor", "output directory for screenshots/source project")
	newPath := flag.String("new", "", "create/open a source-form project at this directory")
	openPath := flag.String("open", "", "open a source-form project directory")
	shotPath := flag.String("shot", "", "screenshot path for F12, -capture, or -headless")
	flag.Parse()

	table, err := locale.Load(os.DirFS("data"), "en")
	if err != nil {
		fatalf("load locale: %v", err)
	}
	app := shell.New(table)
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
	if err := shell.RunWindowCaptureSequence(app, steps); err != nil {
		return err
	}
	if err := writeBrushDump(app, outDir); err != nil {
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
