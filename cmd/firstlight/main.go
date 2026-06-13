// Command firstlight is the M0.5 "First Light" demo (PRD §7):
// a window, a ground plane, one unit, player control.
//
//   - Left-click the unit to select it (yellow ring), left-click ground to deselect.
//   - Right-click the ground to order the unit to move there.
//   - F12 saves a screenshot to firstlight.png.
//
// The unit is the census-verified KayKit Knight (assets/kaykit-adventurers/
// Knight.glb, OK-ANIMATED) playing Idle/Running_A clips; the grounds are
// dressed with static Quaternius Ultimate Fantasy RTS scenery (OK-STATIC).
// Asset binaries are gitignored — when a GLB is missing the demo falls back
// to the original blue box so -autotest still works on a fresh checkout.
//
// Verification flags (FSV protocol, PRD §5.5 / R-FSV-1..3):
//
//	-autotest        scripted run: orders the unit to a known target, waits for
//	                 arrival, prints final state as JSON, captures a screenshot,
//	                 and exits 0 (non-zero on timeout).
//	-shot PATH       screenshot output path (default artifacts/firstlight.png)
//	-tod H           fixed time-of-day hour for render verification; valid [0,24)
//
// This code is M0.5 throwaway-tolerant: movement runs in the render loop with
// float math. The deterministic 20 Hz sim replaces it in M3.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/png"
	"math"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"time"

	litobs "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/obs"
	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	lithud "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render/hud"
	"github.com/g3n/engine/animation"
	"github.com/g3n/engine/app"
	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/experimental/collision"
	"github.com/g3n/engine/geometry"
	"github.com/g3n/engine/gls"
	"github.com/g3n/engine/graphic"
	"github.com/g3n/engine/gui"
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/loader/gltf"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/renderer"
	"github.com/g3n/engine/texture"
	"github.com/g3n/engine/window"
)

const (
	groundSize  = 24.0
	unitSpeed   = 4.0 // world units per second
	arriveEps   = 0.05
	autoTargetX = 5.0
	autoTargetZ = -4.0
	autoTimeout = 10 * time.Second

	todTicksPerSecond = 20.0
	todDayTicks       = 9600.0
	todHoursPerSecond = 24.0 * todTicksPerSecond / todDayTicks

	assetsDir    = "assets"
	knightGLB    = "kaykit-adventurers/Knight.glb"
	knightHeight = 1.6 // world units, bbox-normalized
)

// sceneryItem places one static GLB: bbox-normalized to extent world units,
// bottom on the ground, bbox center at (x, z), rotated rotY about +Y.
type sceneryItem struct {
	glb    string
	extent float32
	x, z   float32
	rotY   float32
}

// scenery dresses the ground plane (±12 world units). All models are
// OK-STATIC in docs/assets/census.tsv.
var scenery = []sceneryItem{
	{glb: "quaternius-ultimate-fantasy-rts/Castle_Fortress.glb", extent: 7.0, x: 0, z: -9},
	{glb: "quaternius-ultimate-fantasy-rts/Village_Market.glb", extent: 4.0, x: -7.5, z: -5, rotY: 0.5},
	{glb: "quaternius-ultimate-fantasy-rts/Hut.glb", extent: 2.4, x: 7, z: -6, rotY: -0.4},
	{glb: "quaternius-ultimate-fantasy-rts/House.glb", extent: 2.8, x: 9, z: -3.5, rotY: -0.9},
	{glb: "quaternius-ultimate-fantasy-rts/Gold_Rocks.glb", extent: 2.2, x: -8.5, z: 4},
	{glb: "quaternius-ultimate-fantasy-rts/Pine_Trees.glb", extent: 3.6, x: -10, z: -8},
	{glb: "quaternius-ultimate-fantasy-rts/Pine_Trees.glb", extent: 3.2, x: 9.5, z: -8.5, rotY: 2.1},
	{glb: "quaternius-ultimate-fantasy-rts/Trees.glb", extent: 3.0, x: -10, z: 8.5, rotY: 1.2},
	{glb: "quaternius-ultimate-fantasy-rts/Trees.glb", extent: 2.8, x: 10, z: 7.5, rotY: -1.7},
	{glb: "quaternius-ultimate-fantasy-rts/Stone_Tower.glb", extent: 3.0, x: 5.5, z: 5.5, rotY: 2.6},
}

type demo struct {
	app    *app.Application
	scene  *core.Node
	cam    *camera.Camera
	ground *graphic.Mesh
	unit   *core.Node // wrapper: position = unit ground position
	body   core.INode // visual child (GLB model or fallback box), pick target
	ring   *graphic.Mesh

	idleAnim *animation.Animation
	runAnim  *animation.Animation
	active   *animation.Animation

	selected  bool
	hasTarget bool
	target    math32.Vector3

	shotPath    string
	autotest    bool
	autoOrdered bool
	autoStart   time.Time
	shotPending bool

	fixedTOD  todFlag
	todHours  float64
	todSource func() float64
	dayNight  *litrender.DayNight

	counters *litobs.Counters
	std      litobs.StandardCounters

	frame        uint32
	simTick      uint32
	simTickAccum time.Duration
	lastMallocs  uint64

	perfStartVisible bool
	perfDumpPath     string
	perfToggleSpam   int
	perfTogglesDone  int
	stressUnits      int

	perfOverlay  *lithud.PerfOverlay
	perfTexture  *texture.Texture2D
	perfImage    *gui.Image
	lastPerf     lithud.PerfInput
	lastStats    litrender.FrameStats
	phaseScratch [7]int64
}

// state is the FSV state dump printed by -autotest (R-FSV-2).
type state struct {
	UnitX, UnitZ     float64
	TargetX, TargetZ float64
	HasTarget        bool
	Selected         bool
	ArrivedAtTarget  bool
	TimeOfDay        float64
}

type perfDump struct {
	Enabled          bool                   `json:"enabled"`
	StressUnits      int                    `json:"stressUnits"`
	ToggleSpamTarget int                    `json:"toggleSpamTarget"`
	TogglesDone      int                    `json:"togglesDone"`
	FrameStats       litrender.FrameStats   `json:"frameStats"`
	Counters         perfCounterDump        `json:"counters"`
	Overlay          lithud.PerfOverlayDump `json:"overlay"`
	CrossCheck       perfCrossCheck         `json:"crossCheck"`
}

type perfCounterDump struct {
	Tick            uint32 `json:"tick"`
	Frame           uint32 `json:"frame"`
	RenderDrawCalls int64  `json:"renderDrawCalls"`
	RenderFPS       int64  `json:"renderFPS"`
	RenderBatches   int64  `json:"renderBatches"`
	RenderInstances int64  `json:"renderInstances"`
	RenderAllocs    int64  `json:"renderAllocsFrame"`
	SimAllocs       int64  `json:"simAllocsTick"`
	HeapBytes       int64  `json:"heapBytes"`
	Units           int64  `json:"units"`
	Missiles        int64  `json:"missiles"`
	Buffs           int64  `json:"buffs"`
}

type perfCrossCheck struct {
	OverlayDrawCalls      int   `json:"overlayDrawCalls"`
	OverlayDrawCallBudget int   `json:"overlayDrawCallBudget"`
	GUIDrawCalls          int   `json:"guiDrawCalls"`
	CounterDrawCalls      int64 `json:"counterDrawCalls"`
	StatsDrawCalls        int   `json:"statsDrawCalls"`
	DisplayedDrawCalls    int64 `json:"displayedDrawCalls"`
	DrawCallsMatch        bool  `json:"drawCallsMatch"`
	OverlayWithinBudget   bool  `json:"overlayWithinBudget"`
}

type todFlag struct {
	set   bool
	value float64
}

func (t *todFlag) String() string {
	if !t.set {
		return ""
	}
	return strconv.FormatFloat(t.value, 'f', -1, 64)
}

func (t *todFlag) Set(s string) error {
	v, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return fmt.Errorf("parse tod hour: %w", err)
	}
	if math.IsNaN(v) || math.IsInf(v, 0) || v < 0 || v >= 24 {
		return fmt.Errorf("tod must be in [0,24), got %s", s)
	}
	t.set = true
	t.value = v
	return nil
}

func main() {
	d := &demo{}
	flag.StringVar(&d.shotPath, "shot", "artifacts/firstlight.png", "screenshot output path")
	flag.BoolVar(&d.autotest, "autotest", false, "scripted FSV run: order unit, verify arrival, screenshot, exit")
	flag.Var(&d.fixedTOD, "tod", "fixed time-of-day hour for render verification; valid [0,24)")
	flag.BoolVar(&d.perfStartVisible, "perf-overlay", false, "start with the F11 perf overlay visible")
	flag.StringVar(&d.perfDumpPath, "perf-dump", "", "write F11 perf overlay/counter JSON at autotest completion")
	flag.IntVar(&d.perfToggleSpam, "perf-toggle-spam", 0, "autotest-only: toggle the F11 overlay this many times at 10 Hz")
	flag.IntVar(&d.stressUnits, "stress-units", 0, "spawn N synthetic visible units for perf overlay stress counters")
	flag.Parse()
	if d.perfToggleSpam < 0 {
		fatalf("perf-toggle-spam must be >=0, got %d", d.perfToggleSpam)
	}
	if d.stressUnits < 0 {
		fatalf("stress-units must be >=0, got %d", d.stressUnits)
	}
	d.configureTODSource(time.Now())
	d.counters = litobs.NewDefaultCounters()
	d.std = litobs.RegisterStandardCounters(d.counters)

	d.app = app.App(1280, 720, "Light in the Dark — First Light (M0.5)")
	d.scene = core.NewNode()

	d.buildCamera()
	d.buildLights()
	d.buildGround()
	d.buildScenery()
	d.buildUnit()
	d.buildStressUnits()
	if err := d.buildPerfOverlay(); err != nil {
		fatalf("perf overlay: %v", err)
	}
	d.installInput()
	d.resetAllocBaseline()

	d.autoStart = time.Now()
	d.app.Run(d.update)
}

func fatalf(format string, a ...any) {
	fmt.Fprintf(os.Stderr, "firstlight: "+format+"\n", a...)
	os.Exit(1)
}

func (d *demo) configureTODSource(now time.Time) {
	if d.fixedTOD.set {
		fixed := d.fixedTOD.value
		d.todSource = func() float64 { return fixed }
		fmt.Printf("event: tod source fixed hour=%.3f\n", fixed)
		return
	}
	d.todHours = cyclingTODSeed(now)
	d.todSource = func() float64 { return d.todHours }
	fmt.Printf("event: tod source cycling hour=%.3f rate=%.6f_hours_per_sec\n", d.todHours, todHoursPerSecond)
}

func cyclingTODSeed(now time.Time) float64 {
	return math.Mod(float64(now.UnixNano())/float64(time.Second)*todHoursPerSecond, 24)
}

func (d *demo) advanceTOD(dt time.Duration) {
	if d.fixedTOD.set {
		return
	}
	d.todHours += dt.Seconds() * todHoursPerSecond
	if d.todHours >= 24 {
		d.todHours = math.Mod(d.todHours, 24)
	}
}

// buildCamera creates the locked RTS camera (PRD R-RND-1): fixed yaw,
// ~34 degree pitch from vertical, no orbit control.
func (d *demo) buildCamera() {
	d.cam = camera.New(1)
	d.cam.SetPosition(0, 12, 9)
	d.cam.LookAt(&math32.Vector3{X: 0, Y: 0, Z: -1.5}, &math32.Vector3{X: 0, Y: 1, Z: 0})
	d.scene.Add(d.cam)

	onResize := func(evname string, ev interface{}) {
		width, height := d.app.GetSize()
		d.app.Gls().Viewport(0, 0, int32(width), int32(height))
		d.cam.SetAspect(float32(width) / float32(height))
	}
	d.app.Subscribe(window.OnWindowSize, onResize)
	onResize("", nil)
}

func (d *demo) buildLights() {
	ambient := light.NewAmbient(&math32.Color{}, 0)
	sun := light.NewDirectional(&math32.Color{}, 0)
	d.scene.Add(ambient)
	d.scene.Add(sun)
	d.dayNight = litrender.NewDayNight(ambient, sun)
	d.dayNight.Update(d.todSource())
}

func (d *demo) buildGround() {
	geom := geometry.NewPlane(groundSize, groundSize)
	mat := material.NewStandard(&math32.Color{R: 0.22, G: 0.45, B: 0.20})
	d.ground = graphic.NewMesh(geom, mat)
	d.ground.SetRotationX(-math32.Pi / 2)
	d.scene.Add(d.ground)
}

// loadGLB parses a GLB and returns its default scene node, or an error.
func loadGLB(rel string) (core.INode, error) {
	doc, err := gltf.ParseBin(filepath.Join(assetsDir, rel))
	if err != nil {
		return nil, err
	}
	sceneIdx := 0
	if doc.Scene != nil {
		sceneIdx = *doc.Scene
	}
	return doc.LoadScene(sceneIdx)
}

// normalize wraps model so its bbox is scaled to extent world units (largest
// axis), bottom at y=0, bbox center on the wrapper origin in XZ.
func normalize(inode core.INode, extent float32) *core.Node {
	model := inode.GetNode()
	model.UpdateMatrixWorld() // bbox needs world matrices, stale until first render
	bb := model.BoundingBox()
	var center, size math32.Vector3
	bb.Center(&center)
	bb.Size(&size)
	max := size.X
	if size.Y > max {
		max = size.Y
	}
	if size.Z > max {
		max = size.Z
	}
	scale := float32(1)
	if max > 0.001 {
		scale = extent / max
	}
	model.SetScale(scale, scale, scale)
	model.SetPosition(-center.X*scale, -bb.Min.Y*scale, -center.Z*scale)
	wrapper := core.NewNode()
	wrapper.Add(model)
	return wrapper
}

func (d *demo) buildScenery() {
	for _, it := range scenery {
		model, err := loadGLB(it.glb)
		if err != nil {
			fmt.Printf("event: scenery skipped %s (%v)\n", it.glb, err)
			continue
		}
		w := normalize(model, it.extent)
		w.SetPosition(it.x, 0, it.z)
		w.SetRotationY(it.rotY)
		d.scene.Add(w)
		fmt.Printf("event: scenery placed %s at (%.1f, %.1f)\n", it.glb, it.x, it.z)
	}
}

func (d *demo) buildUnit() {
	d.unit = core.NewNode()
	d.unit.SetPosition(0, 0, 0)
	d.scene.Add(d.unit)

	if doc, err := gltf.ParseBin(filepath.Join(assetsDir, knightGLB)); err == nil {
		if err := d.buildKnight(doc); err != nil {
			fmt.Printf("event: knight load failed (%v), using fallback box\n", err)
			d.buildFallbackBox()
		}
	} else {
		fmt.Printf("event: knight GLB missing (%v), using fallback box\n", err)
		d.buildFallbackBox()
	}

	ringGeom := geometry.NewTorus(0.7, 0.05, 8, 24, 2*math32.Pi)
	ringMat := material.NewStandard(&math32.Color{R: 1.0, G: 0.9, B: 0.1})
	d.ring = graphic.NewMesh(ringGeom, ringMat)
	d.ring.SetRotationX(-math32.Pi / 2)
	d.ring.SetPosition(0, 0.05, 0)
	d.ring.SetVisible(false)
	d.unit.Add(d.ring)
}

// buildKnight loads the skinned Knight model and its Idle/Running_A clips.
// Animations must be loaded after LoadScene (channels bind to loaded nodes).
func (d *demo) buildKnight(doc *gltf.GLTF) error {
	sceneIdx := 0
	if doc.Scene != nil {
		sceneIdx = *doc.Scene
	}
	model, err := doc.LoadScene(sceneIdx)
	if err != nil {
		return err
	}
	body := normalize(model, knightHeight)
	d.body = body
	d.unit.Add(body)

	idle, err := doc.LoadAnimationByName("Idle")
	if err != nil {
		return fmt.Errorf("load Idle clip: %w", err)
	}
	run, err := doc.LoadAnimationByName("Running_A")
	if err != nil {
		return fmt.Errorf("load Running_A clip: %w", err)
	}
	idle.SetLoop(true)
	run.SetLoop(true)
	d.idleAnim = idle
	d.runAnim = run
	d.active = idle
	fmt.Printf("event: knight loaded model=%s clips=[Idle Running_A]\n", knightGLB)
	return nil
}

func (d *demo) buildFallbackBox() {
	geom := geometry.NewBox(0.6, 1.2, 0.6)
	mat := material.NewStandard(&math32.Color{R: 0.15, G: 0.30, B: 0.85})
	box := graphic.NewMesh(geom, mat)
	box.SetPosition(0, 0.6, 0)
	d.body = box
	d.unit.Add(box)
}

func (d *demo) buildStressUnits() {
	if d.stressUnits == 0 {
		return
	}
	geom := geometry.NewGeometry()
	positions := math32.NewArrayF32(0, d.stressUnits*4*3)
	normals := math32.NewArrayF32(0, d.stressUnits*4*3)
	uvs := math32.NewArrayF32(0, d.stressUnits*4*2)
	indices := math32.NewArrayU32(0, d.stressUnits*6)

	mat := material.NewStandard(&math32.Color{R: 0.30, G: 0.62, B: 0.76})
	cols := int(math.Ceil(math.Sqrt(float64(d.stressUnits))))
	spacing := float32(0.42)
	size := float32(0.08)
	y := float32(0.035)
	for i := 0; i < d.stressUnits; i++ {
		col := i % cols
		row := i / cols
		x := (float32(col) - float32(cols)/2) * spacing
		z := 2.0 + (float32(row)-float32(cols)/2)*spacing
		base := uint32(i * 4)

		positions.Append(
			x-size, y, z-size,
			x+size, y, z-size,
			x+size, y, z+size,
			x-size, y, z+size,
		)
		for j := 0; j < 4; j++ {
			normals.Append(0, 1, 0)
		}
		uvs.Append(0, 0, 1, 0, 1, 1, 0, 1)
		indices.Append(base, base+2, base+1, base, base+3, base+2)
	}
	geom.SetIndices(indices)
	geom.AddVBO(gls.NewVBO(positions).AddAttrib(gls.VertexPosition))
	geom.AddVBO(gls.NewVBO(normals).AddAttrib(gls.VertexNormal))
	geom.AddVBO(gls.NewVBO(uvs).AddAttrib(gls.VertexTexcoord))
	geom.AddGroup(0, indices.Size(), 0)

	mesh := graphic.NewMesh(geom, mat)
	d.scene.Add(mesh)
	fmt.Printf("event: perf stress units spawned count=%d renderables=1 markers=%d\n", d.stressUnits, d.stressUnits)
}

func (d *demo) buildPerfOverlay() error {
	overlay, err := lithud.NewPerfOverlay(lithud.DefaultPerfOverlayWidth, lithud.DefaultPerfOverlayHeight)
	if err != nil {
		return err
	}
	overlay.SetVisible(d.perfStartVisible)
	tex := texture.NewTexture2DFromRGBA(overlay.Image())
	tex.SetMagFilter(gls.NEAREST)
	tex.SetMinFilter(gls.NEAREST)
	img := gui.NewImageFromTex(tex)
	img.SetPosition(12, 12)
	img.SetVisible(overlay.Visible())
	d.scene.Add(img)

	d.perfOverlay = overlay
	d.perfTexture = tex
	d.perfImage = img
	return nil
}

func (d *demo) resetAllocBaseline() {
	var mem runtime.MemStats
	runtime.ReadMemStats(&mem)
	d.lastMallocs = mem.Mallocs
}

// setMoving switches the active clip between Idle and Running_A.
func (d *demo) setMoving(moving bool) {
	if d.idleAnim == nil || d.runAnim == nil {
		return
	}
	next := d.idleAnim
	if moving {
		next = d.runAnim
	}
	if next != d.active {
		next.Reset()
		d.active = next
	}
}

func (d *demo) installInput() {
	d.app.Subscribe(window.OnMouseDown, func(evname string, ev interface{}) {
		mev := ev.(*window.MouseEvent)
		switch mev.Button {
		case window.MouseButtonLeft:
			d.onLeftClick(mev.Xpos, mev.Ypos)
		case window.MouseButtonRight:
			d.onRightClick(mev.Xpos, mev.Ypos)
		}
	})
	d.app.Subscribe(window.OnKeyDown, func(evname string, ev interface{}) {
		kev := ev.(*window.KeyEvent)
		switch kev.Key {
		case window.KeyF11:
			d.togglePerfOverlay()
		case window.KeyF12:
			d.shotPending = true
		}
	})
}

func (d *demo) togglePerfOverlay() {
	if d.perfOverlay == nil || d.perfImage == nil {
		return
	}
	d.perfOverlay.Toggle()
	d.perfImage.SetVisible(d.perfOverlay.Visible())
	fmt.Printf("event: perf overlay visible=%v toggles=%d\n", d.perfOverlay.Visible(), d.perfOverlay.ToggleCount())
}

// raycast casts a picking ray through window coordinates (x, y).
func (d *demo) raycast(x, y float32, target core.INode) []collision.Intersect {
	width, height := d.app.GetSize()
	ndcX := 2*x/float32(width) - 1
	ndcY := -2*y/float32(height) + 1
	rc := collision.NewRaycaster(&math32.Vector3{}, &math32.Vector3{})
	if err := rc.SetFromCamera(d.cam, ndcX, ndcY); err != nil {
		return nil
	}
	return rc.IntersectObject(target, true)
}

func (d *demo) onLeftClick(x, y float32) {
	if len(d.raycast(x, y, d.body)) > 0 {
		d.setSelected(true)
		return
	}
	d.setSelected(false)
}

func (d *demo) onRightClick(x, y float32) {
	hits := d.raycast(x, y, d.ground)
	if len(hits) == 0 {
		return
	}
	d.orderMove(hits[0].Point.X, hits[0].Point.Z)
}

func (d *demo) setSelected(sel bool) {
	d.selected = sel
	d.ring.SetVisible(sel)
}

func (d *demo) orderMove(x, z float32) {
	d.target = math32.Vector3{X: x, Y: 0, Z: z}
	d.hasTarget = true
	d.setMoving(true)
	fmt.Printf("event: move order issued target=(%.2f, %.2f)\n", x, z) // R-FSV-3
}

func (d *demo) update(rend *renderer.Renderer, deltaTime time.Duration) {
	d.frame++
	frameStart := time.Now()
	phaseStart := frameStart
	d.advanceTOD(deltaTime)
	d.dayNight.Update(d.todSource())
	d.phaseScratch[1] = time.Since(phaseStart).Nanoseconds()

	phaseStart = time.Now()
	d.stepMovement(float32(deltaTime.Seconds()))
	d.phaseScratch[3] = time.Since(phaseStart).Nanoseconds()

	phaseStart = time.Now()
	if d.active != nil {
		d.active.Update(float32(deltaTime.Seconds()))
	}
	d.phaseScratch[4] = time.Since(phaseStart).Nanoseconds()

	phaseStart = time.Now()
	d.runAutotest()
	d.phaseScratch[6] = time.Since(phaseStart).Nanoseconds()
	d.updatePerfOverlayTexture()

	d.app.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
	if err := rend.Render(d.scene, d.cam); err != nil {
		fmt.Fprintf(os.Stderr, "render error: %v\n", err)
		os.Exit(1)
	}
	stats := litrender.ReadFrameStats(rend)
	d.lastStats = stats
	d.recordPerfCounters(deltaTime, stats, d.phaseScratch, time.Since(frameStart))

	if d.shotPending {
		d.shotPending = false
		if err := d.screenshot(d.shotPath); err != nil {
			fmt.Fprintf(os.Stderr, "screenshot error: %v\n", err)
			os.Exit(1)
		}
		fmt.Printf("event: screenshot saved path=%s tod=%.3f\n", d.shotPath, d.todSource())
		if d.autotest {
			d.finishAutotest()
		}
	}
	for i := range d.phaseScratch {
		d.phaseScratch[i] = 0
	}
}

func (d *demo) updatePerfOverlayTexture() {
	if d.perfOverlay == nil || d.perfTexture == nil || d.perfImage == nil {
		return
	}
	d.perfOverlay.Update(d.lastPerf)
	visible := d.perfOverlay.Visible()
	d.perfImage.SetVisible(visible)
	if visible {
		d.perfTexture.SetFromRGBA(d.perfOverlay.Image())
	}
}

func (d *demo) recordPerfCounters(deltaTime time.Duration, stats litrender.FrameStats, phases [7]int64, frameCPU time.Duration) {
	d.simTickAccum += deltaTime
	for d.simTickAccum >= 50*time.Millisecond {
		d.simTick++
		d.simTickAccum -= 50 * time.Millisecond
	}

	var mem runtime.MemStats
	runtime.ReadMemStats(&mem)
	var frameAllocs int64
	if mem.Mallocs >= d.lastMallocs {
		frameAllocs = int64(mem.Mallocs - d.lastMallocs)
	}
	d.lastMallocs = mem.Mallocs

	tickNS := int64(0)
	for i := 0; i < len(phases); i++ {
		tickNS += phases[i]
	}
	if tickNS == 0 {
		tickNS = frameCPU.Nanoseconds()
	}
	fps := int64(0)
	if deltaTime > 0 {
		fps = int64(time.Second / deltaTime)
	}
	units := int64(1 + d.stressUnits)

	d.counters.Set(d.std.SimTickNS, tickNS)
	d.counters.Set(d.std.SimPhaseInputNS, phases[0])
	d.counters.Set(d.std.SimPhaseScriptsNS, phases[1])
	d.counters.Set(d.std.SimPhaseOrdersNS, phases[2])
	d.counters.Set(d.std.SimPhaseMovementNS, phases[3])
	d.counters.Set(d.std.SimPhaseCombatNS, phases[4])
	d.counters.Set(d.std.SimPhaseEventsNS, phases[5])
	d.counters.Set(d.std.SimPhaseCleanupNS, phases[6])
	d.counters.Set(d.std.RenderFrameNS, deltaTime.Nanoseconds())
	d.counters.Set(d.std.RenderFPS, fps)
	d.counters.Set(d.std.RenderDrawCalls, int64(stats.DrawCalls))
	d.counters.Set(d.std.RenderBatches, int64(stats.StateChanges))
	d.counters.Set(d.std.RenderInstances, int64(stats.VisibleGraphics))
	d.counters.Set(d.std.RenderAllocsFrame, frameAllocs)
	d.counters.Set(d.std.SimAllocsTick, 0)
	d.counters.Set(d.std.HeapBytes, int64(mem.HeapAlloc))
	d.counters.Set(d.std.SimPathExpansionsTick, 0)
	d.counters.Set(d.std.SimPathQueueDepth, 0)
	d.counters.Set(d.std.SimEntitiesUnitsActive, units)
	d.counters.Set(d.std.SimEntitiesMissiles, 0)
	d.counters.Set(d.std.SimEntitiesBuffs, 0)
	d.counters.Set(d.std.AudioVoicesActive, 0)
	d.counters.Sample(d.simTick, d.frame)

	d.lastPerf = lithud.PerfInput{
		Tick:        d.simTick,
		Frame:       d.frame,
		TickNS:      d.counters.Value(d.std.SimTickNS),
		PhaseNS:     phases,
		FrameNS:     d.counters.Value(d.std.RenderFrameNS),
		FPS:         d.counters.Value(d.std.RenderFPS),
		DrawCalls:   d.counters.Value(d.std.RenderDrawCalls),
		Batches:     d.counters.Value(d.std.RenderBatches),
		Instances:   d.counters.Value(d.std.RenderInstances),
		AllocsFrame: d.counters.Value(d.std.RenderAllocsFrame),
		AllocsTick:  d.counters.Value(d.std.SimAllocsTick),
		HeapBytes:   d.counters.Value(d.std.HeapBytes),
		Units:       d.counters.Value(d.std.SimEntitiesUnitsActive),
		Missiles:    d.counters.Value(d.std.SimEntitiesMissiles),
		Buffs:       d.counters.Value(d.std.SimEntitiesBuffs),
	}
}

func (d *demo) stepMovement(dt float32) {
	if !d.hasTarget {
		return
	}
	pos := d.unit.Position()
	delta := math32.Vector3{X: d.target.X - pos.X, Y: 0, Z: d.target.Z - pos.Z}
	dist := delta.Length()
	if dist <= arriveEps {
		d.unit.SetPosition(d.target.X, 0, d.target.Z)
		d.hasTarget = false
		d.setMoving(false)
		fmt.Printf("event: arrived pos=(%.2f, %.2f)\n", d.target.X, d.target.Z)
		return
	}
	step := unitSpeed * dt
	if step > dist {
		step = dist
	}
	delta.Normalize().MultiplyScalar(step)
	d.unit.SetPosition(pos.X+delta.X, 0, pos.Z+delta.Z)
	d.unit.SetRotationY(math32.Atan2(delta.X, delta.Z))
}

// runAutotest drives the scripted FSV scenario: select the unit and order it
// to a known target through the same code paths the player input uses, then
// verify arrival against the source of truth (the unit's actual position).
func (d *demo) runAutotest() {
	if !d.autotest {
		return
	}
	elapsed := time.Since(d.autoStart)
	d.runPerfToggleSpam(elapsed)
	if !d.autoOrdered && elapsed > 500*time.Millisecond {
		d.autoOrdered = true
		d.setSelected(true)
		d.orderMove(autoTargetX, autoTargetZ)
	}
	if d.autoOrdered && !d.hasTarget && !d.shotPending {
		d.shotPending = true // capture, then finishAutotest runs after render
	}
	if elapsed > autoTimeout {
		fmt.Fprintln(os.Stderr, "autotest: TIMEOUT waiting for arrival")
		os.Exit(2)
	}
}

func (d *demo) runPerfToggleSpam(elapsed time.Duration) {
	if d.perfToggleSpam == 0 || d.perfOverlay == nil {
		return
	}
	want := int(elapsed / (100 * time.Millisecond))
	if want > d.perfToggleSpam {
		want = d.perfToggleSpam
	}
	for d.perfTogglesDone < want {
		d.togglePerfOverlay()
		d.perfTogglesDone++
	}
}

func (d *demo) finishAutotest() {
	pos := d.unit.Position()
	s := state{
		UnitX: float64(pos.X), UnitZ: float64(pos.Z),
		TargetX: autoTargetX, TargetZ: autoTargetZ,
		HasTarget: d.hasTarget, Selected: d.selected,
		ArrivedAtTarget: math32.Abs(pos.X-autoTargetX) < 0.1 && math32.Abs(pos.Z-autoTargetZ) < 0.1,
		TimeOfDay:       d.todSource(),
	}
	out, _ := json.Marshal(s)
	fmt.Printf("state: %s\n", out)
	if d.perfDumpPath != "" {
		if err := d.writePerfDump(d.perfDumpPath); err != nil {
			fmt.Fprintf(os.Stderr, "perf dump error: %v\n", err)
			os.Exit(4)
		}
		fmt.Printf("event: perf dump saved path=%s\n", d.perfDumpPath)
	}
	if !s.ArrivedAtTarget {
		os.Exit(3)
	}
	os.Exit(0)
}

func (d *demo) writePerfDump(path string) error {
	if d.perfOverlay != nil {
		d.perfOverlay.Update(d.lastPerf)
	}
	dump := d.perfDump()
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
	return enc.Encode(dump)
}

func (d *demo) perfDump() perfDump {
	snap := d.perfOverlay.Snapshot()
	displayedDrawCalls := perfOverlayRowCurrent(snap, "DRAW")
	counterDrawCalls := d.counters.Value(d.std.RenderDrawCalls)
	return perfDump{
		Enabled:          snap.Visible,
		StressUnits:      d.stressUnits,
		ToggleSpamTarget: d.perfToggleSpam,
		TogglesDone:      d.perfTogglesDone,
		FrameStats:       d.lastStats,
		Counters: perfCounterDump{
			Tick:            d.lastPerf.Tick,
			Frame:           d.lastPerf.Frame,
			RenderDrawCalls: d.counters.Value(d.std.RenderDrawCalls),
			RenderFPS:       d.counters.Value(d.std.RenderFPS),
			RenderBatches:   d.counters.Value(d.std.RenderBatches),
			RenderInstances: d.counters.Value(d.std.RenderInstances),
			RenderAllocs:    d.counters.Value(d.std.RenderAllocsFrame),
			SimAllocs:       d.counters.Value(d.std.SimAllocsTick),
			HeapBytes:       d.counters.Value(d.std.HeapBytes),
			Units:           d.counters.Value(d.std.SimEntitiesUnitsActive),
			Missiles:        d.counters.Value(d.std.SimEntitiesMissiles),
			Buffs:           d.counters.Value(d.std.SimEntitiesBuffs),
		},
		Overlay: snap,
		CrossCheck: perfCrossCheck{
			OverlayDrawCalls:      snap.DrawCalls,
			OverlayDrawCallBudget: snap.DrawCallBudget,
			GUIDrawCalls:          d.lastStats.GUIDrawCalls,
			CounterDrawCalls:      counterDrawCalls,
			StatsDrawCalls:        d.lastStats.DrawCalls,
			DisplayedDrawCalls:    displayedDrawCalls,
			DrawCallsMatch:        displayedDrawCalls == counterDrawCalls && int64(d.lastStats.DrawCalls) == counterDrawCalls,
			OverlayWithinBudget:   snap.DrawCalls <= snap.DrawCallBudget && d.lastStats.GUIDrawCalls <= snap.DrawCallBudget,
		},
	}
}

func perfOverlayRowCurrent(snap lithud.PerfOverlayDump, name string) int64 {
	for _, row := range snap.Rows {
		if row.Name == name {
			return row.Current
		}
	}
	return 0
}

// screenshot reads the framebuffer and writes it as PNG (R-FSV-1).
func (d *demo) screenshot(path string) error {
	width, height := d.app.GetFramebufferSize()
	data := d.app.Gls().ReadPixels(0, 0, width, height, gls.RGBA, gls.UNSIGNED_BYTE)
	img := image.NewRGBA(image.Rect(0, 0, width, height))
	rowLen := width * 4
	for y := 0; y < height; y++ {
		src := data[(height-1-y)*rowLen : (height-y)*rowLen]
		dst := img.Pix[y*img.Stride : y*img.Stride+rowLen]
		copy(dst, src)
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
