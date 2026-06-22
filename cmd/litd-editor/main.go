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
	if err := shell.RunWindowCaptureSequence(app, steps); err != nil {
		return err
	}
	body, _ := json.MarshalIndent(app.Snapshot(), "", "  ")
	if err := os.WriteFile(filepath.Join(outDir, "final-state.json"), append(body, '\n'), 0o644); err != nil {
		return err
	}
	fmt.Printf("state: %s\n", app.Snapshot().JSON())
	return nil
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
