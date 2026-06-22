package shell

import (
	"encoding/json"
	"fmt"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"time"

	g3napp "github.com/g3n/engine/app"
	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/gui"
	"github.com/g3n/engine/renderer"
	"github.com/g3n/engine/texture"
	"github.com/g3n/engine/window"
)

type WindowOptions struct {
	ShotPath      string
	CaptureFrame  bool
	ExitAfterShot bool
}

type WindowCaptureStep struct {
	Name          string
	ShotPath      string
	BeforeCapture func() error
}

type windowSurface struct {
	win   *g3napp.Application
	scene *core.Node
	cam   *camera.Camera
	panel *gui.Image
}

func RunWindow(state *App, opts WindowOptions) error {
	if state == nil {
		return fmt.Errorf("editor window: nil shell state")
	}
	if opts.ShotPath == "" {
		opts.ShotPath = filepath.Join("artifacts", "litd-editor", "window.png")
	}

	surface := newWindowSurface(state)

	var shotPending bool
	var textureDirty bool
	var lastErr error
	refresh := func() {
		surface.refresh(state)
		textureDirty = false
	}
	markDirty := func(err error) {
		if err != nil {
			lastErr = err
		}
		textureDirty = true
	}
	newTarget := func() string {
		snap := state.Snapshot()
		if snap.ProjectPath == "" {
			return filepath.Join(os.TempDir(), "litd-editor-new-world")
		}
		return filepath.Join(filepath.Dir(snap.ProjectPath), "litd-editor-new-world")
	}

	surface.win.Subscribe(window.OnKeyDown, func(_ string, ev interface{}) {
		kev := ev.(*window.KeyEvent)
		switch kev.Key {
		case window.Key1:
			markDirty(state.SwitchMode(ModeTerrain))
		case window.Key2:
			markDirty(state.SwitchMode(ModeObjects))
		case window.Key3:
			markDirty(state.SwitchMode(ModeMetadata))
		case window.Key4:
			markDirty(state.SetPaintLayer(0))
		case window.Key5:
			markDirty(state.SetPaintLayer(1))
		case window.Key6:
			markDirty(state.SetPaintLayer(2))
		case window.Key7:
			markDirty(state.SetPaintLayer(3))
		case window.KeyQ:
			markDirty(state.SetTerrainBrush(BrushRaise))
		case window.KeyW:
			markDirty(state.SetTerrainBrush(BrushLower))
		case window.KeyE:
			markDirty(state.SetTerrainBrush(BrushLevel))
		case window.KeyR:
			markDirty(state.SetTerrainBrush(BrushRamp))
		case window.KeyLeftBracket:
			b := state.BrushSnapshot()
			markDirty(state.SetBrushSize(b.Size - 1))
		case window.KeyRightBracket:
			b := state.BrushSnapshot()
			markDirty(state.SetBrushSize(b.Size + 1))
		case window.KeyMinus:
			b := state.BrushSnapshot()
			markDirty(state.SetBrushStrength(b.Strength - 1))
		case window.KeyEqual:
			b := state.BrushSnapshot()
			markDirty(state.SetBrushStrength(b.Strength + 1))
		case window.KeyLeft:
			markDirty(state.SetBrushRampDirection(RampWest))
		case window.KeyRight:
			markDirty(state.SetBrushRampDirection(RampEast))
		case window.KeyUp:
			markDirty(state.SetBrushRampDirection(RampNorth))
		case window.KeyDown:
			markDirty(state.SetBrushRampDirection(RampSouth))
		case window.KeyN:
			markDirty(state.NewProject(newTarget()))
		case window.KeyS:
			markDirty(state.Save())
		case window.KeyZ:
			if kev.Mods&window.ModControl != 0 {
				markDirty(state.Undo())
			}
		case window.KeyY:
			if kev.Mods&window.ModControl != 0 {
				markDirty(state.Redo())
			}
		case window.KeyEnter:
			if state.Snapshot().Confirm != nil {
				markDirty(state.ConfirmPending())
			}
		case window.KeyEscape:
			if state.Snapshot().Confirm != nil {
				state.CancelConfirm()
				textureDirty = true
			}
		case window.KeyF12:
			shotPending = true
		}
	})
	var activeStroke *TerrainStroke
	surface.win.Subscribe(window.OnMouseDown, func(_ string, ev interface{}) {
		mev := ev.(*window.MouseEvent)
		if mev.Button != window.MouseButtonLeft || state.Snapshot().Mode != ModeTerrain {
			return
		}
		x, y, ok := terrainCellAt(state.Snapshot(), mev.Xpos, mev.Ypos)
		if ok {
			var err error
			if state.Snapshot().TerrainTool == TerrainToolPaint {
				err = state.ApplyPaintBrush(x, y)
			} else {
				var stroke *TerrainStroke
				stroke, err = state.BeginTerrainStroke(x, y)
				if err == nil {
					activeStroke = stroke
				}
			}
			markDirty(err)
		}
	})
	surface.win.Subscribe(window.OnCursor, func(_ string, ev interface{}) {
		if activeStroke == nil || state.Snapshot().Mode != ModeTerrain {
			return
		}
		cev := ev.(*window.CursorEvent)
		x, y, ok := terrainCellAt(state.Snapshot(), cev.Xpos, cev.Ypos)
		if ok {
			markDirty(activeStroke.AddPoint(x, y))
		}
	})
	surface.win.Subscribe(window.OnMouseUp, func(_ string, ev interface{}) {
		mev := ev.(*window.MouseEvent)
		if mev.Button != window.MouseButtonLeft || activeStroke == nil {
			return
		}
		stroke := activeStroke
		activeStroke = nil
		markDirty(stroke.End())
	})

	if opts.CaptureFrame {
		shotPending = true
	}
	surface.win.Run(func(rend *renderer.Renderer, _ time.Duration) {
		if textureDirty {
			refresh()
		}
		if err := surface.render(rend); err != nil {
			fmt.Fprintf(os.Stderr, "litd-editor: render: %v\n", err)
			os.Exit(1)
		}
		if shotPending {
			shotPending = false
			if err := screenshot(surface.win, opts.ShotPath); err != nil {
				fmt.Fprintf(os.Stderr, "litd-editor: screenshot: %v\n", err)
				os.Exit(1)
			}
			fmt.Printf("event: screenshot saved path=%s mode=%s dirty=%v\n", opts.ShotPath, state.Snapshot().Mode, state.Snapshot().Dirty)
			if opts.ExitAfterShot {
				body, _ := json.Marshal(state.Snapshot())
				fmt.Printf("state: %s\n", body)
				surface.win.Exit()
			}
		}
	})
	return lastErr
}

func terrainCellAt(snap Snapshot, xpos, ypos float32) (int, int, bool) {
	x := int(xpos) - terrainGridX
	y := int(ypos) - terrainGridY
	if x < 0 || y < 0 {
		return 0, 0, false
	}
	cx, ox := x/terrainGridStepX, x%terrainGridStepX
	cy, oy := y/terrainGridStepY, y%terrainGridStepY
	if ox >= terrainGridCellW || oy >= terrainGridCellH {
		return 0, 0, false
	}
	if cx < 0 || cy < 0 || cx >= snap.World.Width || cy >= snap.World.Height || cx >= 8 || cy >= 8 {
		return 0, 0, false
	}
	return cx, cy, true
}

func RunWindowCaptureSequence(state *App, steps []WindowCaptureStep) error {
	if state == nil {
		return fmt.Errorf("editor window: nil shell state")
	}
	if len(steps) == 0 {
		return fmt.Errorf("editor window: capture sequence requires at least one step")
	}
	surface := newWindowSurface(state)
	var runErr error
	current := 0

	prepare := func(step WindowCaptureStep) bool {
		if step.ShotPath == "" {
			runErr = fmt.Errorf("editor window: capture step %q has empty shot path", step.Name)
			return false
		}
		if step.BeforeCapture != nil {
			if err := step.BeforeCapture(); err != nil {
				runErr = err
				return false
			}
		}
		surface.refresh(state)
		return true
	}
	if !prepare(steps[current]) {
		return runErr
	}
	surface.win.Run(func(rend *renderer.Renderer, _ time.Duration) {
		if err := surface.render(rend); err != nil {
			runErr = err
			surface.win.Exit()
			return
		}
		step := steps[current]
		if err := screenshot(surface.win, step.ShotPath); err != nil {
			runErr = err
			surface.win.Exit()
			return
		}
		fmt.Printf("event: screenshot saved path=%s step=%s mode=%s dirty=%v\n", step.ShotPath, step.Name, state.Snapshot().Mode, state.Snapshot().Dirty)
		current++
		if current >= len(steps) {
			surface.win.Exit()
			return
		}
		if !prepare(steps[current]) {
			surface.win.Exit()
		}
	})
	return runErr
}

func newWindowSurface(state *App) *windowSurface {
	win := g3napp.App(ShotWidth, ShotHeight, state.Snapshot().Title)
	scene := core.NewNode()
	cam := camera.New(float32(ShotWidth) / float32(ShotHeight))
	scene.Add(cam)

	panel := gui.NewImageFromRGBA(RenderImage(state.Snapshot()))
	panel.SetPosition(0, 0)
	scene.Add(panel)
	gui.Manager().Set(scene)

	win.Subscribe(window.OnWindowSize, func(string, interface{}) {
		w, h := win.GetSize()
		win.Gls().Viewport(0, 0, int32(w), int32(h))
		if h > 0 {
			cam.SetAspect(float32(w) / float32(h))
		}
	})
	win.Gls().Viewport(0, 0, ShotWidth, ShotHeight)
	win.Gls().ClearColor(0.09, 0.11, 0.12, 1)
	return &windowSurface{win: win, scene: scene, cam: cam, panel: panel}
}

func (s *windowSurface) refresh(state *App) {
	prev := s.panel.SetTexture(texture.NewTexture2DFromRGBA(RenderImage(state.Snapshot())))
	prev.Dispose()
}

func (s *windowSurface) render(rend *renderer.Renderer) error {
	s.win.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
	return rend.Render(s.scene, s.cam)
}

func screenshot(win *g3napp.Application, path string) error {
	w, h := win.GetFramebufferSize()
	data := win.Gls().ReadPixels(0, 0, w, h, gls.RGBA, gls.UNSIGNED_BYTE)
	img := image.NewRGBA(image.Rect(0, 0, w, h))
	row := w * 4
	for y := 0; y < h; y++ {
		copy(img.Pix[y*img.Stride:y*img.Stride+row], data[(h-1-y)*row:(h-y)*row])
	}
	if dir := filepath.Dir(path); dir != "." {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
	}
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return png.Encode(f, img)
}
