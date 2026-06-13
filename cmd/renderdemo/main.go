// Command renderdemo renders deterministic primitive scenes for render-stat FSV.
//
// Usage:
//
//	renderdemo -scene counted -autotest -shot artifacts/stats-hud.png -dump artifacts/stats.json
//	renderdemo -hud -res 1920x1080 -autotest -shot artifacts/canvas.png -dump artifacts/canvas.json
//
// Scenes are synthetic and hand-countable. Each scene includes one GUI label
// so screenshots show a stats line; world counts remain separated in the JSON
// via opaque/transparent/gui buckets.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	lithud "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render/hud"
	"github.com/g3n/engine/app"
	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/gui"
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/renderer"
	"github.com/g3n/engine/texture"
	"github.com/g3n/engine/window"
)

const (
	defaultWidth  = 960
	defaultHeight = 540
)

type sceneSpec struct {
	name     string
	expected litrender.FrameStats
}

type resolutionFlag struct {
	W, H int
	set  bool
}

func (r *resolutionFlag) String() string {
	if r == nil || r.W == 0 || r.H == 0 {
		return ""
	}
	return fmt.Sprintf("%dx%d", r.W, r.H)
}

func (r *resolutionFlag) Set(s string) error {
	before := *r
	widthText, heightText, ok := strings.Cut(s, "x")
	if !ok || widthText == "" || heightText == "" || strings.Contains(heightText, "x") {
		return fmt.Errorf("resolution must be WIDTHxHEIGHT, got %q", s)
	}
	w, werr := strconv.Atoi(widthText)
	h, herr := strconv.Atoi(heightText)
	if werr != nil || herr != nil || w <= 0 || h <= 0 {
		*r = before
		return fmt.Errorf("resolution must be WIDTHxHEIGHT, got %q", s)
	}
	r.W, r.H, r.set = w, h, true
	return nil
}

type canvasRegion struct {
	name   string
	anchor lithud.Anchor
	ref    lithud.RefRect
	color  math32.Color4
}

type canvasRegionDump struct {
	Name   string         `json:"name"`
	Anchor string         `json:"anchor"`
	Kind   string         `json:"kind,omitempty"`
	Parent string         `json:"parent,omitempty"`
	Atlas  string         `json:"atlas,omitempty"`
	CellsX int            `json:"cellsX,omitempty"`
	CellsY int            `json:"cellsY,omitempty"`
	Ref    lithud.RefRect `json:"ref"`
	Rect   lithud.Rect    `json:"rect"`
}

type canvasSnapshot struct {
	Width   int                `json:"width"`
	Height  int                `json:"height"`
	UIScale float64            `json:"uiScale"`
	Scale   float64            `json:"scale"`
	Rects   []canvasRegionDump `json:"rects"`
}

type canvasDump struct {
	Mode   string          `json:"mode"`
	Before *canvasSnapshot `json:"before,omitempty"`
	After  canvasSnapshot  `json:"after"`
	HUD    hudRuntimeDump  `json:"hud,omitempty"`
	OK     bool            `json:"ok"`
	Errors []string        `json:"errors,omitempty"`
}

type hudRuntimeDump struct {
	AtlasPath              string              `json:"atlasPath"`
	WidgetPanels           int                 `json:"widgetPanels"`
	Labels                 int                 `json:"labels"`
	ExpectedGUIDrawCalls   int                 `json:"expectedGuiDrawCalls"`
	DrawCallBudget         int                 `json:"drawCallBudget"`
	ActualGUIDrawCalls     int                 `json:"actualGuiDrawCalls"`
	GUIStateChanges        int                 `json:"guiStateChanges"`
	WorstUpdateMicrosFrame float64             `json:"worstUpdateMicrosPerFrame"`
	UpdateScenarios        lithud.FSVScenarios `json:"updateScenarios"`
}

func main() {
	res := resolutionFlag{W: defaultWidth, H: defaultHeight}
	resizeFrom := resolutionFlag{}
	sceneName := flag.String("scene", "counted", "scene to render: empty, single, counted, culled, shared, twomats, transparent")
	shotPath := flag.String("shot", "artifacts/stats-hud.png", "screenshot output path")
	dumpPath := flag.String("dump", "artifacts/stats.json", "stats JSON output path")
	autotest := flag.Bool("autotest", false, "exit non-zero if dumped counters do not match the hand count")
	hudMode := flag.Bool("hud", false, "render the HUD virtual-canvas FSV fixture")
	uiScale := flag.Float64("uiscale", 1, "HUD user UI scale multiplier; clamped to [0.75,1.5]")
	flag.Var(&res, "res", "window resolution WIDTHxHEIGHT")
	flag.Var(&resizeFrom, "resize-from", "optional pre-resize WIDTHxHEIGHT to include in HUD canvas dump")
	flag.Parse()
	if *hudMode && !res.set {
		res = resolutionFlag{W: 1366, H: 768, set: true}
	}

	a := app.App(res.W, res.H, "LitD render stats demo")
	scene := core.NewNode()
	cam := buildCamera(res.W, res.H)

	var spec sceneSpec
	var canvasFSV canvasDump
	if *hudMode {
		var err error
		canvasFSV, err = buildCanvasHUD(scene, res, *uiScale, resizeFrom)
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: %v\n", err)
			os.Exit(1)
		}
	} else {
		buildLights(scene)
		var err error
		spec, err = buildScene(scene, *sceneName)
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: %v\n", err)
			os.Exit(1)
		}
		addStatsHUD(scene, spec)
	}

	a.Subscribe(window.OnWindowSize, func(string, interface{}) {
		w, h := a.GetSize()
		a.Gls().Viewport(0, 0, int32(w), int32(h))
		cam.SetAspect(float32(w) / float32(h))
	})
	a.Gls().Viewport(0, 0, int32(res.W), int32(res.H))
	a.Gls().ClearColor(0.03, 0.04, 0.05, 1)

	a.Run(func(rend *renderer.Renderer, _ time.Duration) {
		a.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
		if err := rend.Render(scene, cam); err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: render: %v\n", err)
			os.Exit(1)
		}
		stats := litrender.ReadFrameStats(rend)
		if *hudMode {
			canvasFSV.recordFrameStats(stats)
		}
		if *shotPath != "" {
			if err := screenshot(a, *shotPath); err != nil {
				fmt.Fprintf(os.Stderr, "renderdemo: screenshot: %v\n", err)
				os.Exit(1)
			}
		}
		if *dumpPath != "" {
			if *hudMode {
				if err := writeJSONFile(*dumpPath, canvasFSV); err != nil {
					fmt.Fprintf(os.Stderr, "renderdemo: dump: %v\n", err)
					os.Exit(1)
				}
			} else {
				if err := litrender.DumpFrameStatsFile(*dumpPath, stats); err != nil {
					fmt.Fprintf(os.Stderr, "renderdemo: dump: %v\n", err)
					os.Exit(1)
				}
			}
		}

		if *hudMode {
			out, _ := json.Marshal(canvasFSV)
			fmt.Printf("canvas: %s shot=%s dump=%s\n", out, *shotPath, *dumpPath)
			if *autotest && !canvasFSV.OK {
				os.Exit(2)
			}
			os.Exit(0)
		}

		pass := stats == spec.expected
		actualJSON, _ := json.Marshal(stats)
		expectedJSON, _ := json.Marshal(spec.expected)
		fmt.Printf("stats: scene=%s actual=%s expected=%s pass=%v shot=%s dump=%s\n",
			spec.name, actualJSON, expectedJSON, pass, *shotPath, *dumpPath)
		if *autotest && !pass {
			os.Exit(2)
		}
		os.Exit(0)
	})
}

func buildCamera(width, height int) *camera.Camera {
	cam := camera.New(float32(width) / float32(height))
	cam.SetPosition(0, 4, 8)
	cam.LookAt(&math32.Vector3{X: 0, Y: 0, Z: 0}, &math32.Vector3{X: 0, Y: 1, Z: 0})
	return cam
}

func buildLights(scene *core.Node) {
	scene.Add(light.NewAmbient(&math32.Color{R: 1, G: 1, B: 1}, 0.55))
	sun := light.NewDirectional(&math32.Color{R: 1, G: 1, B: 1}, 0.75)
	sun.SetPosition(3, 8, 5)
	scene.Add(sun)
}

func buildScene(scene *core.Node, name string) (sceneSpec, error) {
	geom := geometry.NewBox(0.8, 0.8, 0.8)
	blue := material.NewStandard(&math32.Color{R: 0.20, G: 0.45, B: 0.95})
	red := material.NewStandard(&math32.Color{R: 0.95, G: 0.24, B: 0.18})

	switch name {
	case "empty":
		return sceneSpec{name: name, expected: expectedStats(0, 0, 0, 0, 0, 0)}, nil
	case "single":
		addMesh(scene, geom, blue, 0, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(1, 0, 1, 0, 1, 0)}, nil
	case "counted":
		for i := -2; i <= 2; i++ {
			addMesh(scene, geom, blue, float32(i), 0, 0)
		}
		return sceneSpec{name: name, expected: expectedStats(5, 0, 5, 0, 1, 0)}, nil
	case "culled":
		addMesh(scene, geom, blue, 0, 0, 0)
		addMesh(scene, geom, blue, 1000, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(1, 1, 1, 0, 1, 0)}, nil
	case "shared":
		addMesh(scene, geom, blue, -0.6, 0, 0)
		addMesh(scene, geom, blue, 0.6, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(2, 0, 2, 0, 1, 0)}, nil
	case "twomats":
		addMesh(scene, geom, blue, -0.6, 0, 0)
		addMesh(scene, geom, red, 0.6, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(2, 0, 2, 0, 2, 0)}, nil
	case "transparent":
		blue.SetTransparent(true)
		blue.SetOpacity(0.65)
		addMesh(scene, geom, blue, 0, 0, 0)
		return sceneSpec{name: name, expected: expectedStats(1, 0, 0, 1, 0, 1)}, nil
	default:
		return sceneSpec{}, fmt.Errorf("unknown scene %q", name)
	}
}

func addMesh(scene *core.Node, geom geometry.IGeometry, mat material.IMaterial, x, y, z float32) {
	mesh := graphic.NewMesh(geom, mat)
	mesh.SetPosition(x, y, z)
	scene.Add(mesh)
}

func addStatsHUD(scene *core.Node, spec sceneSpec) {
	text := fmt.Sprintf("scene=%s world=%d/%d draw=%d gui=%d state=%d",
		spec.name,
		spec.expected.VisibleGraphics,
		spec.expected.CulledGraphics,
		spec.expected.OpaqueDrawCalls+spec.expected.TransparentDrawCalls,
		spec.expected.GUIDrawCalls,
		spec.expected.StateChanges,
	)
	label := gui.NewLabel(text)
	label.SetPosition(14, 28)
	scene.Add(label)
}

func buildCanvasHUD(scene *core.Node, res resolutionFlag, uiScale float64, resizeFrom resolutionFlag) (canvasDump, error) {
	canvas, err := lithud.NewCanvas(res.W, res.H, uiScale)
	if err != nil {
		return canvasDump{}, err
	}
	hud := lithud.NewDefaultHUD(canvas)
	after := canvasSnapshotFor(canvas, hud.Widgets())
	scenarios := hud.RunFSVScenarios()
	dump := canvasDump{
		Mode:  "hud-full",
		After: after,
		HUD: hudRuntimeDump{
			AtlasPath:              lithud.DefaultAtlasPath,
			WidgetPanels:           hud.PanelDrawCalls(),
			Labels:                 hud.LabelDrawCalls(),
			ExpectedGUIDrawCalls:   hud.ExpectedGUIDrawCalls(),
			DrawCallBudget:         lithud.DefaultHUDDrawCallCap,
			WorstUpdateMicrosFrame: worstUpdateMicrosFrame(scenarios),
			UpdateScenarios:        scenarios,
		},
	}
	if resizeFrom.set {
		beforeCanvas, err := lithud.NewCanvas(resizeFrom.W, resizeFrom.H, uiScale)
		if err != nil {
			return canvasDump{}, fmt.Errorf("resize-from: %w", err)
		}
		beforeHUD := lithud.NewDefaultHUD(beforeCanvas)
		before := canvasSnapshotFor(beforeCanvas, beforeHUD.Widgets())
		dump.Before = &before
	}
	dump.OK, dump.Errors = validateCanvasSnapshot(after)
	atlasTex, err := texture.NewTexture2DFromImage(lithud.DefaultAtlasPath)
	if err != nil {
		return canvasDump{}, fmt.Errorf("ui atlas: %w", err)
	}
	drawCanvasHUD(scene, after, &hud, atlasTex, dump.OK)
	return dump, nil
}

func canvasSnapshotFor(canvas lithud.Canvas, widgets []lithud.Widget) canvasSnapshot {
	rects := make([]canvasRegionDump, 0, len(widgets))
	for _, widget := range widgets {
		rects = append(rects, canvasRegionDump{
			Name:   widget.Name,
			Anchor: widget.Anchor.String(),
			Kind:   widget.Kind.String(),
			Parent: widget.Parent,
			Atlas:  widget.AtlasRegion,
			CellsX: widget.CellsX,
			CellsY: widget.CellsY,
			Ref:    widget.Ref,
			Rect:   widget.Rect,
		})
	}
	return canvasSnapshot{
		Width:   canvas.Width,
		Height:  canvas.Height,
		UIScale: canvas.UIScale,
		Scale:   canvas.Scale,
		Rects:   rects,
	}
}

func validateCanvasSnapshot(s canvasSnapshot) (bool, []string) {
	var errs []string
	for i, r := range s.Rects {
		if !r.Rect.Inside(s.Width, s.Height) {
			errs = append(errs, fmt.Sprintf("%s offscreen %+v", r.Name, r.Rect))
		}
		if r.Parent != "" {
			parent, ok := snapshotRect(s.Rects, r.Parent)
			if !ok || !r.Rect.InsideRect(parent) {
				errs = append(errs, fmt.Sprintf("%s outside parent %s %+v", r.Name, r.Parent, r.Rect))
			}
			continue
		}
		for j := 0; j < i; j++ {
			if s.Rects[j].Parent == "" && r.Rect.Overlaps(s.Rects[j].Rect) {
				errs = append(errs, fmt.Sprintf("%s overlaps %s", r.Name, s.Rects[j].Name))
			}
		}
	}
	return len(errs) == 0, errs
}

func snapshotRect(rects []canvasRegionDump, name string) (lithud.Rect, bool) {
	for _, r := range rects {
		if r.Name == name {
			return r.Rect, true
		}
	}
	return lithud.Rect{}, false
}

func drawCanvasHUD(scene *core.Node, snap canvasSnapshot, hud *lithud.DefaultHUD, atlasTex *texture.Texture2D, ok bool) {
	for _, region := range snap.Rects {
		rect := region.Rect
		panel := gui.NewPanel(float32(rect.W), float32(rect.H))
		color := hudColor(region)
		panel.SetColor4(&color)
		panel.Material().AddTexture(atlasTex)
		panel.SetPosition(float32(rect.X), float32(rect.Y))
		scene.Add(panel)
	}

	for _, region := range snap.Rects {
		if region.Parent != "" {
			continue
		}
		rect := region.Rect
		label := gui.NewLabel(hudLabel(region.Name, hud, ok))
		y := rect.Y + 22
		if rect.H < 34 {
			y = rect.Y + rect.H - 12
		}
		label.SetPosition(float32(rect.X+6), float32(y))
		scene.Add(label)
	}
}

func hudColor(region canvasRegionDump) math32.Color4 {
	switch region.Kind {
	case "icon-grid":
		return math32.Color4{R: 0.20, G: 0.24, B: 0.34, A: 0.92}
	case "progress-bar":
		if region.Name == "mana-bar" {
			return math32.Color4{R: 0.16, G: 0.30, B: 0.62, A: 0.95}
		}
		return math32.Color4{R: 0.18, G: 0.56, B: 0.24, A: 0.95}
	default:
		switch region.Name {
		case "resource-bar":
			return math32.Color4{R: 0.34, G: 0.23, B: 0.50, A: 0.92}
		case "minimap":
			return math32.Color4{R: 0.18, G: 0.42, B: 0.27, A: 0.92}
		case "portrait":
			return math32.Color4{R: 0.36, G: 0.29, B: 0.16, A: 0.92}
		case "info-panel":
			return math32.Color4{R: 0.17, G: 0.33, B: 0.46, A: 0.92}
		case "command-card":
			return math32.Color4{R: 0.42, G: 0.18, B: 0.18, A: 0.92}
		default:
			return math32.Color4{R: 0.12, G: 0.30, B: 0.52, A: 0.92}
		}
	}
}

func hudLabel(name string, hud *lithud.DefaultHUD, ok bool) string {
	switch name {
	case "resource-bar":
		return hud.Resource.String()
	case "portrait":
		return hud.Vitals.String()
	case "info-panel":
		return hud.Selection.String()
	case "command-card":
		return hud.Queue.String()
	case "control-groups":
		return hud.Groups.String()
	case "menu-cluster":
		return fmt.Sprintf("HUD ok=%v", ok)
	default:
		return name
	}
}

func worstUpdateMicrosFrame(s lithud.FSVScenarios) float64 {
	worst := perFrameMicros(s.Static100)
	for _, v := range []float64{perFrameMicros(s.ResourceChurn), perFrameMicros(s.SelectionChurn)} {
		if v > worst {
			worst = v
		}
	}
	return worst
}

func perFrameMicros(s lithud.ScenarioStats) float64 {
	if s.Frames == 0 {
		return 0
	}
	return float64(s.UpdateMicros) / float64(s.Frames)
}

func (d *canvasDump) recordFrameStats(stats litrender.FrameStats) {
	d.HUD.ActualGUIDrawCalls = stats.GUIDrawCalls
	d.HUD.GUIStateChanges = stats.GUIStates
	if stats.GUIDrawCalls > d.HUD.DrawCallBudget {
		d.OK = false
		d.Errors = append(d.Errors, fmt.Sprintf("gui draw calls %d exceed budget %d", stats.GUIDrawCalls, d.HUD.DrawCallBudget))
	}
	if d.HUD.WorstUpdateMicrosFrame > 1000 {
		d.OK = false
		d.Errors = append(d.Errors, fmt.Sprintf("hud update %.3fus/frame exceeds 1000us", d.HUD.WorstUpdateMicrosFrame))
	}
}

func expectedStats(visible, culled, opaqueDraws, transparentDraws, opaqueStates, transparentStates int) litrender.FrameStats {
	worldDraws := opaqueDraws + transparentDraws
	return litrender.FrameStats{
		GraphicMaterials:     visible,
		Lights:               worldDraws * 2,
		Panels:               1,
		Others:               1,
		VisibleGraphics:      visible,
		CulledGraphics:       culled,
		DrawCalls:            worldDraws + 1,
		OpaqueDrawCalls:      opaqueDraws,
		TransparentDrawCalls: transparentDraws,
		GUIDrawCalls:         1,
		StateChanges:         opaqueStates + transparentStates + 1,
		OpaqueStates:         opaqueStates,
		TransparentStates:    transparentStates,
		GUIStates:            1,
	}
}

func screenshot(a *app.Application, path string) error {
	w, h := a.GetFramebufferSize()
	data := a.Gls().ReadPixels(0, 0, w, h, gls.RGBA, gls.UNSIGNED_BYTE)
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

func writeJSONFile(path string, v interface{}) error {
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
	enc := json.NewEncoder(f)
	enc.SetIndent("", "  ")
	return enc.Encode(v)
}
