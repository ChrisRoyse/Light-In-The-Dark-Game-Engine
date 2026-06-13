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

	litlocale "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	lithud "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render/hud"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
	"github.com/g3n/engine/app"
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

type renderDemoDump struct {
	litrender.FrameStats
	Scene  string                  `json:"scene"`
	Camera litrender.RTSCameraDump `json:"camera"`
	OK     bool                    `json:"ok"`
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
	Mode        string                  `json:"mode"`
	Before      *canvasSnapshot         `json:"before,omitempty"`
	After       canvasSnapshot          `json:"after"`
	HUD         hudRuntimeDump          `json:"hud,omitempty"`
	CommandCard *commandCardRuntimeDump `json:"commandCard,omitempty"`
	ResourceBar *resourceBarRuntimeDump `json:"resourceBar,omitempty"`
	OK          bool                    `json:"ok"`
	Errors      []string                `json:"errors,omitempty"`
}

type hudRuntimeDump struct {
	AtlasPath              string              `json:"atlasPath"`
	Locale                 string              `json:"locale"`
	WidgetPanels           int                 `json:"widgetPanels"`
	Labels                 int                 `json:"labels"`
	ExpectedGUIDrawCalls   int                 `json:"expectedGuiDrawCalls"`
	DrawCallBudget         int                 `json:"drawCallBudget"`
	ActualGUIDrawCalls     int                 `json:"actualGuiDrawCalls"`
	GUIStateChanges        int                 `json:"guiStateChanges"`
	WorstUpdateMicrosFrame float64             `json:"worstUpdateMicrosPerFrame"`
	UpdateScenarios        lithud.FSVScenarios `json:"updateScenarios"`
}

type commandCardRuntimeDump struct {
	TablePath string                    `json:"tablePath"`
	Scenario  string                    `json:"scenario"`
	Current   commandCardCaseDump       `json:"current"`
	Cases     []commandCardCaseDump     `json:"cases"`
	Clicks    []lithud.CommandCardClick `json:"clicks"`
	Emitted   []commandRecordDump       `json:"emitted"`
}

type commandCardCaseDump struct {
	Name           string                        `json:"name"`
	Selection      string                        `json:"selection"`
	ActiveSubgroup string                        `json:"activeSubgroup,omitempty"`
	Visible        bool                          `json:"visible"`
	Summary        string                        `json:"summary"`
	Update         lithud.CommandCardUpdate      `json:"update"`
	Slots          []lithud.CommandCardSlotState `json:"slots"`
}

type commandRecordDump struct {
	Version   uint8    `json:"version"`
	Player    uint8    `json:"player"`
	Seq       uint16   `json:"seq"`
	Opcode    uint8    `json:"opcode"`
	Flags     uint8    `json:"flags"`
	UnitCount uint8    `json:"unitCount"`
	Units     []uint32 `json:"units"`
	Target    uint32   `json:"target,omitempty"`
	PointX    int64    `json:"pointX"`
	PointY    int64    `json:"pointY"`
	Data      uint16   `json:"data,omitempty"`
}

type resourceBarRuntimeDump struct {
	Scenario string                    `json:"scenario"`
	Current  resourceBarCaseDump       `json:"current"`
	Cases    []resourceBarCaseDump     `json:"cases"`
	Feedback []lithud.ResourceFeedback `json:"feedback,omitempty"`
}

type resourceBarCaseDump struct {
	Name      string                    `json:"name"`
	Sim       resourceBarValues         `json:"sim"`
	Displayed string                    `json:"displayed"`
	Update    lithud.ResourceBarUpdate  `json:"update"`
	Feedback  []lithud.ResourceFeedback `json:"feedback,omitempty"`
}

type resourceBarValues struct {
	Gold     int `json:"gold"`
	Lumber   int `json:"lumber"`
	FoodUsed int `json:"foodUsed"`
	FoodCap  int `json:"foodCap"`
	Upkeep   int `json:"upkeep"`
}

func main() {
	res := resolutionFlag{W: defaultWidth, H: defaultHeight}
	resizeFrom := resolutionFlag{}
	sceneName := flag.String("scene", "counted", "scene to render: empty, single, counted, culled, shared, twomats, transparent, camera-rig")
	shotPath := flag.String("shot", "artifacts/stats-hud.png", "screenshot output path")
	dumpPath := flag.String("dump", "artifacts/stats.json", "stats JSON output path")
	autotest := flag.Bool("autotest", false, "exit non-zero if dumped counters do not match the hand count")
	hudMode := flag.Bool("hud", false, "render the HUD virtual-canvas FSV fixture")
	zoomMode := flag.String("zoom", "default", "RTS camera zoom request: default, min, max, below-min, above-max, or a numeric world-unit distance")
	localeTag := flag.String("locale", "en", "locale tag for HUD strings when -hud is set")
	cardScenario := flag.String("card-scenario", "", "command-card FSV scenario for -hud -scene basecamp: unit, building, subgroup, enemy, cooldown, empty")
	resbarScenario := flag.String("resbar-scenario", "", "resource-bar FSV scenario for -hud -scene basecamp: initial, after-spend, foodcap, insufficient, large")
	uiScale := flag.Float64("uiscale", 1, "HUD user UI scale multiplier; clamped to [0.75,1.5]")
	flag.Var(&res, "res", "window resolution WIDTHxHEIGHT")
	flag.Var(&resizeFrom, "resize-from", "optional pre-resize WIDTHxHEIGHT to include in HUD canvas dump")
	flag.Parse()
	if *hudMode && !res.set {
		res = resolutionFlag{W: 1366, H: 768, set: true}
	}

	a := app.App(res.W, res.H, "LitD render stats demo")
	scene := core.NewNode()
	cameraRig, err := buildCamera(res.W, res.H, *zoomMode)
	if err != nil {
		fmt.Fprintf(os.Stderr, "renderdemo: camera: %v\n", err)
		os.Exit(1)
	}
	cam := cameraRig.Camera

	var spec sceneSpec
	var canvasFSV canvasDump
	if *hudMode {
		table, err := litlocale.Load(os.DirFS("data"), *localeTag)
		if err != nil {
			fmt.Fprintf(os.Stderr, "renderdemo: locale: %v\n", err)
			os.Exit(1)
		}
		canvasFSV, err = buildCanvasHUD(scene, res, *uiScale, resizeFrom, *sceneName, *cardScenario, *resbarScenario, *localeTag, table, lithud.HUDStringsFromLocale(table))
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
		cameraRig.SetAspect(float32(w) / float32(h))
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
		var sceneDump renderDemoDump
		if !*hudMode {
			cameraDump := cameraRig.DumpWithLockProbe(91, 12, 45)
			pass := stats == spec.expected && cameraDump.OK
			sceneDump = renderDemoDump{FrameStats: stats, Scene: spec.name, Camera: cameraDump, OK: pass}
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
				if err := writeJSONFile(*dumpPath, sceneDump); err != nil {
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

		actualJSON, _ := json.Marshal(stats)
		expectedJSON, _ := json.Marshal(spec.expected)
		fmt.Printf("stats: scene=%s actual=%s expected=%s pass=%v shot=%s dump=%s\n",
			spec.name, actualJSON, expectedJSON, sceneDump.OK, *shotPath, *dumpPath)
		if *autotest && !sceneDump.OK {
			os.Exit(2)
		}
		os.Exit(0)
	})
}

func buildCamera(width, height int, zoomText string) (*litrender.RTSCamera, error) {
	cfg := litrender.DefaultRTSCameraConfig(float32(width) / float32(height))
	zoom, err := cameraZoomRequest(zoomText, cfg)
	if err != nil {
		return nil, err
	}
	rig := litrender.NewRTSCamera(cfg)
	rig.SetZoomRequested(zoom)
	return rig, nil
}

func cameraZoomRequest(zoomText string, cfg litrender.RTSCameraConfig) (float32, error) {
	switch strings.ToLower(strings.TrimSpace(zoomText)) {
	case "", "default", "zdefault":
		return cfg.Zoom, nil
	case "min", "zmin":
		return cfg.ZoomMin, nil
	case "max", "zmax":
		return cfg.ZoomMax, nil
	case "below-min":
		return cfg.ZoomMin * 0.5, nil
	case "above-max":
		return cfg.ZoomMax * 2, nil
	default:
		value, err := strconv.ParseFloat(zoomText, 32)
		if err != nil {
			return 0, fmt.Errorf("unknown zoom request %q", zoomText)
		}
		return float32(value), nil
	}
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
		addMesh(scene, geom, blue, 100000, 0, 0)
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
	case "camera-rig":
		addCameraRigScene(scene)
		return sceneSpec{name: name, expected: expectedStats(6, 0, 6, 0, 3, 0)}, nil
	default:
		return sceneSpec{}, fmt.Errorf("unknown scene %q", name)
	}
}

func addCameraRigScene(scene *core.Node) {
	groundMat := material.NewStandard(&math32.Color{R: 0.20, G: 0.44, B: 0.24})
	markerMat := material.NewStandard(&math32.Color{R: 0.82, G: 0.68, B: 0.30})
	ground := graphic.NewMesh(geometry.NewPlane(6400, 6400), groundMat)
	ground.SetRotationX(-math32.Pi / 2)
	scene.Add(ground)

	markerGeom := geometry.NewBox(90, 24, 90)
	addMesh(scene, markerGeom, markerMat, 0, 12, 0)
	addMesh(scene, markerGeom, markerMat, -320, 12, -320)
	addMesh(scene, markerGeom, markerMat, 320, 12, -320)
	addMesh(scene, markerGeom, markerMat, -320, 12, 320)
	addMesh(scene, markerGeom, markerMat, 320, 12, 320)
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

func buildCanvasHUD(scene *core.Node, res resolutionFlag, uiScale float64, resizeFrom resolutionFlag, sceneName, cardScenario, resbarScenario, localeTag string, localeTable *litlocale.Table, labels lithud.HUDStrings) (canvasDump, error) {
	canvas, err := lithud.NewCanvas(res.W, res.H, uiScale)
	if err != nil {
		return canvasDump{}, err
	}
	hud := lithud.NewDefaultHUDWithStrings(canvas, labels)
	after := canvasSnapshotFor(canvas, hud.Widgets())
	scenarios := hud.RunFSVScenarios()
	dump := canvasDump{
		Mode:  "hud-full",
		After: after,
		HUD: hudRuntimeDump{
			AtlasPath:              lithud.DefaultAtlasPath,
			Locale:                 localeTag,
			WidgetPanels:           hud.PanelDrawCalls(),
			Labels:                 hud.LabelDrawCalls(),
			ExpectedGUIDrawCalls:   hud.ExpectedGUIDrawCalls(),
			DrawCallBudget:         lithud.DefaultHUDDrawCallCap,
			WorstUpdateMicrosFrame: worstUpdateMicrosFrame(scenarios),
			UpdateScenarios:        scenarios,
		},
	}
	var card *lithud.CommandCard
	if sceneName == "basecamp" || cardScenario != "" {
		cardDump, displayCard, err := buildCommandCardFSV(localeTable, cardScenario)
		if err != nil {
			return canvasDump{}, err
		}
		dump.CommandCard = cardDump
		card = displayCard
	}
	if sceneName == "basecamp" || resbarScenario != "" {
		resourceDump, err := buildResourceBarFSV(&hud, resbarScenario)
		if err != nil {
			return canvasDump{}, err
		}
		dump.ResourceBar = resourceDump
	}
	if resizeFrom.set {
		beforeCanvas, err := lithud.NewCanvas(resizeFrom.W, resizeFrom.H, uiScale)
		if err != nil {
			return canvasDump{}, fmt.Errorf("resize-from: %w", err)
		}
		beforeHUD := lithud.NewDefaultHUDWithStrings(beforeCanvas, labels)
		before := canvasSnapshotFor(beforeCanvas, beforeHUD.Widgets())
		dump.Before = &before
	}
	dump.OK, dump.Errors = validateCanvasSnapshot(after)
	atlasTex, err := texture.NewTexture2DFromImage(lithud.DefaultAtlasPath)
	if err != nil {
		return canvasDump{}, fmt.Errorf("ui atlas: %w", err)
	}
	drawCanvasHUD(scene, after, &hud, card, atlasTex, dump.OK)
	return dump, nil
}

func buildResourceBarFSV(hud *lithud.DefaultHUD, scenario string) (*resourceBarRuntimeDump, error) {
	if scenario == "" {
		scenario = "initial"
	}
	names := []string{"initial", "after-spend", "foodcap", "insufficient", "large"}
	dump := &resourceBarRuntimeDump{Scenario: scenario}
	for _, name := range names {
		state, ok := renderDemoResourceScenarioState(name)
		if !ok {
			return nil, fmt.Errorf("resourcebar: unknown scenario %q", name)
		}
		dump.Cases = append(dump.Cases, snapshotResourceBarCase(hud.Labels, name, state))
	}
	state, ok := renderDemoResourceScenarioState(scenario)
	if !ok {
		return nil, fmt.Errorf("resourcebar: unknown scenario %q", scenario)
	}
	dump.Current = applyResourceBarCase(hud, scenario, state)
	dump.Feedback = hud.ResourceBar.FeedbackEvents()
	return dump, nil
}

func snapshotResourceBarCase(labels lithud.HUDStrings, name string, state lithud.HUDState) resourceBarCaseDump {
	var text lithud.TextBuffer
	bar := lithud.NewResourceBar(&text, lithud.ResourceBarStringsFromHUD(labels))
	var feedback []lithud.ResourceFeedback
	if name == "insufficient" {
		feedback = append(feedback, bar.InsufficientGold(12, state.Gold))
	}
	update := bar.Update(lithud.ResourceBarState{Gold: state.Gold, Lumber: state.Lumber, FoodUsed: state.FoodUsed, FoodCap: state.FoodCap, Upkeep: state.Upkeep, Tick: resourceBarTickFor(name)})
	return resourceBarCaseDump{Name: name, Sim: resourceValuesFor(state), Displayed: text.String(), Update: update, Feedback: feedback}
}

func applyResourceBarCase(hud *lithud.DefaultHUD, name string, state lithud.HUDState) resourceBarCaseDump {
	hud.Update(state)
	var feedback []lithud.ResourceFeedback
	if name == "insufficient" {
		feedback = append(feedback, hud.ResourceBar.InsufficientGold(12, state.Gold))
	}
	update := hud.ResourceBar.Update(lithud.ResourceBarState{Gold: state.Gold, Lumber: state.Lumber, FoodUsed: state.FoodUsed, FoodCap: state.FoodCap, Upkeep: state.Upkeep, Tick: resourceBarTickFor(name)})
	return resourceBarCaseDump{Name: name, Sim: resourceValuesFor(state), Displayed: hud.Resource.String(), Update: update, Feedback: feedback}
}

func resourceBarTickFor(name string) uint32 {
	if name == "insufficient" {
		return 12
	}
	if name == "large" {
		return 60
	}
	return 0
}

func renderDemoResourceScenarioState(name string) (lithud.HUDState, bool) {
	state := lithud.DefaultHUDState()
	switch name {
	case "initial":
		return state, true
	case "after-spend":
		state.Gold -= 135
		state.FoodUsed++
		return state, true
	case "foodcap":
		state.Gold = 999
		state.Lumber = 888
		state.FoodUsed = 100
		state.FoodCap = 100
		state.Upkeep = 2
		return state, true
	case "insufficient":
		return state, true
	case "large":
		state.Gold = 9999
		state.Lumber = 12000
		state.FoodUsed = 99
		state.FoodCap = 100
		state.Upkeep = 3
		return state, true
	default:
		return lithud.HUDState{}, false
	}
}

func resourceValuesFor(state lithud.HUDState) resourceBarValues {
	return resourceBarValues{
		Gold:     state.Gold,
		Lumber:   state.Lumber,
		FoodUsed: state.FoodUsed,
		FoodCap:  state.FoodCap,
		Upkeep:   state.Upkeep,
	}
}

func buildCommandCardFSV(localeTable *litlocale.Table, scenario string) (*commandCardRuntimeDump, *lithud.CommandCard, error) {
	if scenario == "" {
		scenario = "unit"
	}
	table, err := lithud.LoadCommandCardTable(os.DirFS("data"))
	if err != nil {
		return nil, nil, err
	}
	states := []struct {
		name  string
		state lithud.CommandCardState
	}{
		{name: "unit", state: renderDemoCardUnitState()},
		{name: "building", state: renderDemoCardBuildingState()},
		{name: "subgroup", state: renderDemoCardSubgroupState()},
		{name: "enemy", state: renderDemoCardEnemyState()},
		{name: "cooldown", state: renderDemoCardCooldownState()},
		{name: "empty", state: renderDemoCardEmptyState()},
	}
	dump := &commandCardRuntimeDump{TablePath: table.Path, Scenario: scenario}
	for _, entry := range states {
		card := lithud.NewCommandCard(table, localeTable)
		dump.Cases = append(dump.Cases, snapshotCommandCardCase(entry.name, &card, entry.state))
	}

	emitter := lithud.NewCommandCard(table, localeTable)
	emitter.Refresh(renderDemoCardUnitState())
	click := emitter.ClickSlot(0, false)
	dump.Clicks = append(dump.Clicks, click)
	if click.Accepted && click.PendingTarget {
		if rec, ok := emitter.ConfirmTarget(fixed.Vec2{X: fixed.FromInt(320), Y: fixed.FromInt(480)}, 0, false); ok {
			dump.Emitted = append(dump.Emitted, commandRecordDumpFor(rec))
		}
	}
	disabled := lithud.NewCommandCard(table, localeTable)
	disabled.Refresh(renderDemoCardCooldownState())
	dump.Clicks = append(dump.Clicks, disabled.ClickSlot(1, false))

	currentState, ok := renderDemoCardScenarioState(scenario)
	if !ok {
		return nil, nil, fmt.Errorf("command-card: unknown scenario %q", scenario)
	}
	display := lithud.NewCommandCard(table, localeTable)
	dump.Current = snapshotCommandCardCase(scenario, &display, currentState)
	return dump, &display, nil
}

func snapshotCommandCardCase(name string, card *lithud.CommandCard, state lithud.CommandCardState) commandCardCaseDump {
	update := card.Refresh(state)
	return commandCardCaseDump{
		Name:           name,
		Selection:      state.SelectionLabel,
		ActiveSubgroup: card.ActiveSubgroup,
		Visible:        card.Visible,
		Summary:        card.Summary.String(),
		Update:         update,
		Slots:          visibleCommandCardSlots(card),
	}
}

func visibleCommandCardSlots(card *lithud.CommandCard) []lithud.CommandCardSlotState {
	out := make([]lithud.CommandCardSlotState, 0, lithud.CommandCardSlots)
	for _, slot := range card.Slots {
		if slot.Visible {
			out = append(out, slot)
		}
	}
	return out
}

func commandRecordDumpFor(r sim.CommandRecord) commandRecordDump {
	out := commandRecordDump{
		Version:   r.Version,
		Player:    r.Player,
		Seq:       r.Seq,
		Opcode:    r.Opcode,
		Flags:     r.Flags,
		UnitCount: r.UnitCount,
		Target:    uint32(r.Target),
		PointX:    int64(r.Point.X),
		PointY:    int64(r.Point.Y),
		Data:      r.Data,
	}
	out.Units = make([]uint32, 0, r.UnitCount)
	for i := uint8(0); i < r.UnitCount; i++ {
		out.Units = append(out.Units, uint32(r.Units[i]))
	}
	return out
}

func renderDemoCardScenarioState(name string) (lithud.CommandCardState, bool) {
	switch name {
	case "unit":
		return renderDemoCardUnitState(), true
	case "building":
		return renderDemoCardBuildingState(), true
	case "subgroup":
		return renderDemoCardSubgroupState(), true
	case "enemy":
		return renderDemoCardEnemyState(), true
	case "cooldown":
		return renderDemoCardCooldownState(), true
	case "empty":
		return renderDemoCardEmptyState(), true
	default:
		return lithud.CommandCardState{}, false
	}
}

func renderDemoCardUnitState() lithud.CommandCardState {
	var state lithud.CommandCardState
	state.Player = 0
	state.OwnSelection = true
	state.SelectionLabel = "footman"
	state.Subgroups[0] = "footman"
	state.SubgroupCount = 1
	state.UnitCount = 2
	state.Units[0], state.Units[1] = 101, 102
	state.Gold, state.Lumber = 725, 240
	return state
}

func renderDemoCardBuildingState() lithud.CommandCardState {
	var state lithud.CommandCardState
	state.Player = 0
	state.OwnSelection = true
	state.SelectionLabel = "barracks"
	state.Subgroups[0] = "barracks"
	state.SubgroupCount = 1
	state.UnitCount = 1
	state.Units[0] = 201
	state.Gold, state.Lumber = 725, 240
	return state
}

func renderDemoCardSubgroupState() lithud.CommandCardState {
	state := renderDemoCardUnitState()
	state.SelectionLabel = "mixed"
	state.Subgroups[1] = "barracks"
	state.SubgroupCount = 2
	state.UnitCount = 3
	state.Units[2] = 201
	lithud.CycleCommandSubgroup(&state)
	return state
}

func renderDemoCardEnemyState() lithud.CommandCardState {
	state := renderDemoCardUnitState()
	state.OwnSelection = false
	state.SelectionLabel = "enemy-footman"
	return state
}

func renderDemoCardCooldownState() lithud.CommandCardState {
	state := renderDemoCardBuildingState()
	state.SelectionLabel = "barracks-low"
	state.Gold = 100
	state.Lumber = 0
	state.Cooldown[0] = 5
	return state
}

func renderDemoCardEmptyState() lithud.CommandCardState {
	state := renderDemoCardUnitState()
	state.SelectionLabel = "empty"
	state.UnitCount = 0
	return state
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

func drawCanvasHUD(scene *core.Node, snap canvasSnapshot, hud *lithud.DefaultHUD, card *lithud.CommandCard, atlasTex *texture.Texture2D, ok bool) {
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
		label := gui.NewLabel(hudLabel(region.Name, hud, card, ok))
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

func hudLabel(name string, hud *lithud.DefaultHUD, card *lithud.CommandCard, ok bool) string {
	switch name {
	case "resource-bar":
		return hud.Resource.String()
	case "portrait":
		return hud.Vitals.String()
	case "info-panel":
		return hud.Selection.String()
	case "command-card":
		if card != nil {
			return card.Summary.String()
		}
		return hud.Queue.String()
	case "control-groups":
		return hud.Groups.String()
	case "menu-cluster":
		if ok {
			return hud.Labels.MenuOKTrue
		}
		return hud.Labels.MenuOKFalse
	case "idle-worker":
		return hud.Labels.IdleWorker
	case "minimap":
		return hud.Labels.Minimap
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
