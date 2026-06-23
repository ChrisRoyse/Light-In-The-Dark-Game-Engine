// Command game is the interactive game-shell skeleton (#531): it composes the
// loadable world (litd/worldhost → api.Game), the g3n render pipeline (locked RTS
// camera + day/night + ground, reused from cmd/worldrender), a real-time sim tick
// loop, keyboard input routed to unit selection/orders, save/load, and a pause
// overlay — the first place the engine's verticals run as one playable session
// rather than a scripted harness.
//
//	-world DIR     world directory (contains data/ and main.lua)
//	-archive FILE  verified .litdworld archive
//	-tod H         fixed time-of-day hour for lighting [0,24) (default 11)
//	-save FILE     quicksave file path (F5 save / F9 load); default <out>/quicksave.litdsave
//	-out DIR       screenshot/save output directory (default artifacts)
//	-autotest      scripted FSV: render, order a unit, verify it moved, save→diverge→
//	               load→verify state restored, verify pause freezes the sim, then exit
//
// Interactive keys: Esc pause/menu · F5 quicksave · F9 quickload · Tab cycle
// selection · Space order selected unit forward · Q quit (in menu). Like
// cmd/firstlight/worldrender the frame work runs inside app.Run's update callback
// (the g3n lifecycle is fiddly under WSLg). Exit 0 = clean / autotest passed.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"time"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/buildinfo"
	litrender "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/render"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
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
	"github.com/g3n/engine/window"
)

const (
	groundSize             = 24.0
	fitHalf                = 8.0
	minSimHalf             = 100.0
	ticksPerSecond         = 20
	orderReach             = 80.0               // sim units a Space-order pushes the selected unit forward (kept modest so the demo unit stays in the auto-fit frame; a following camera is a follow-up)
	saveFingerprint uint64 = 0x4C49544435415645 // "LITD5AVE" — same-world quicksave tag
)

var teamColors = []math32.Color{
	{R: 0.55, G: 0.55, B: 0.58},
	{R: 0.85, G: 0.18, B: 0.18},
	{R: 0.20, G: 0.35, B: 0.85},
	{R: 0.20, G: 0.70, B: 0.25},
	{R: 0.85, G: 0.80, B: 0.20},
}

type game struct {
	app   *app.Application
	scene *core.Node
	cam   *camera.Camera
	day   *litrender.DayNight
	host  *worldhost.Host
	g     *api.Game

	unitsRoot *core.Node
	menuRoot  *core.Node
	hud       *gui.Label

	world, archive, savePath, outDir string
	tod                              float64
	cx, cy, scale                    float64

	paused   bool
	selected int
	curTick  int
	acc      time.Duration

	shotPending bool
	shotName    string

	// autotest state
	autotest  bool
	phase     int
	beforePos api.Vec2
	target    api.Vec2
	startDist float64
	savedHash uint64
	divergeOK bool
	moveOK    bool
	loadOK    bool
	pauseOK   bool
}

func fatalf(format string, a ...any) {
	fmt.Fprintf(os.Stderr, "game: "+format+"\n", a...)
	os.Exit(1)
}

func main() {
	gm := &game{selected: 0}
	flag.StringVar(&gm.world, "world", "", "world directory (contains data/ and main.lua)")
	flag.StringVar(&gm.archive, "archive", "", "verified .litdworld archive")
	seed := flag.Int64("seed", 1, "deterministic PRNG seed (R-SIM-2)")
	budget := flag.Int64("budget", 50_000_000, "per-eval Lua instruction budget (R-SEC-1)")
	flag.Float64Var(&gm.tod, "tod", 11.0, "fixed time-of-day hour for lighting [0,24)")
	flag.StringVar(&gm.savePath, "save", "", "quicksave file path (default <out>/quicksave.litdsave)")
	flag.StringVar(&gm.outDir, "out", "artifacts", "screenshot/save output directory")
	flag.BoolVar(&gm.autotest, "autotest", false, "scripted FSV run, then exit")
	showVersion := flag.Bool("version", false, "print version and exit")
	flag.Parse()

	if *showVersion {
		fmt.Printf("game %s\n", engineVersion())
		return
	}
	if gm.tod < 0 || gm.tod >= 24 {
		fatalf("-tod must be in [0,24), got %v", gm.tod)
	}
	if (gm.world == "") == (gm.archive == "") {
		fatalf("pass exactly one of -world or -archive")
	}
	if gm.savePath == "" {
		gm.savePath = filepath.Join(gm.outDir, "quicksave.litdsave")
	}

	host, err := gm.loadHost(*seed, *budget)
	if err != nil {
		fatalf("load world: %v", err)
	}
	defer host.Close()
	gm.host = host
	gm.g = host.Game
	fmt.Printf("event: world loaded units=%d seed=%d\n", len(gm.g.AllUnits(nil)), *seed)

	gm.computeFit()
	gm.app = app.App(1280, 720, "Light in the Dark — game (#531)")
	gm.scene = core.NewNode()
	gm.buildCamera()
	gm.buildLights()
	gm.buildGround()
	gm.unitsRoot = core.NewNode()
	gm.scene.Add(gm.unitsRoot)
	gm.buildMenu()
	gm.buildHUD()
	gm.bindInput()
	gm.rebuildUnits()

	gm.app.Run(gm.update)
}

func (gm *game) loadHost(seed, budget int64) (*worldhost.Host, error) {
	if gm.archive != "" {
		return worldhost.LoadArchive(gm.archive, engineVersion(), seed, budget)
	}
	return worldhost.Load(gm.world, seed, budget)
}

func engineVersion() string {
	v := buildinfo.Get().Version
	if len(v) > 0 && v[0] == 'v' {
		v = v[1:]
	}
	if v == "" || v == "dev" {
		return "0.1.0"
	}
	return v
}

// --- scene plumbing (reused from cmd/worldrender) ---

func (gm *game) buildCamera() {
	gm.cam = camera.New(1)
	gm.cam.SetPosition(0, 12, 9)
	gm.cam.LookAt(&math32.Vector3{X: 0, Y: 0, Z: -1.5}, &math32.Vector3{X: 0, Y: 1, Z: 0})
	gm.scene.Add(gm.cam)
	onResize := func(string, interface{}) {
		w, ht := gm.app.GetSize()
		gm.app.Gls().Viewport(0, 0, int32(w), int32(ht))
		gm.cam.SetAspect(float32(w) / float32(ht))
	}
	gm.app.Subscribe(window.OnWindowSize, onResize)
	onResize("", nil)
}

func (gm *game) buildLights() {
	ambient := light.NewAmbient(&math32.Color{}, 0)
	sun := light.NewDirectional(&math32.Color{}, 0)
	gm.scene.Add(ambient)
	gm.scene.Add(sun)
	gm.day = litrender.NewDayNight(ambient, sun)
	gm.day.Update(gm.tod)
}

func (gm *game) buildGround() {
	geom := geometry.NewPlane(groundSize, groundSize)
	mat := material.NewStandard(&math32.Color{R: 0.22, G: 0.45, B: 0.20})
	ground := graphic.NewMesh(geom, mat)
	ground.SetRotationX(-math32.Pi / 2)
	gm.scene.Add(ground)
}

func (gm *game) computeFit() {
	us := gm.g.UnitsInRange(api.Vec2{}, 1e9, nil)
	if len(us) == 0 {
		gm.cx, gm.cy, gm.scale = 0, 0, fitHalf/minSimHalf
		return
	}
	minX, maxX := us[0].Position().X, us[0].Position().X
	minY, maxY := us[0].Position().Y, us[0].Position().Y
	for _, u := range us {
		p := u.Position()
		minX, maxX = min(minX, p.X), max(maxX, p.X)
		minY, maxY = min(minY, p.Y), max(maxY, p.Y)
	}
	gm.cx, gm.cy = (minX+maxX)/2, (minY+maxY)/2
	half := max(max((maxX-minX)/2, (maxY-minY)/2), minSimHalf)
	gm.scale = fitHalf / half
}

func (gm *game) simToWorld(p api.Vec2) (x, z float32) {
	return float32((p.X - gm.cx) * gm.scale), float32((p.Y - gm.cy) * gm.scale)
}

// worldToSim is the inverse of simToWorld: a ground-plane (x,z) back to sim units.
func (gm *game) worldToSim(x, z float32) api.Vec2 {
	return api.Vec2{X: float64(x)/gm.scale + gm.cx, Y: float64(z)/gm.scale + gm.cy}
}

// screenToGround casts the camera ray through window pixel (sx,sy) of a w×h
// viewport and intersects the y=0 ground plane, returning the sim-space point.
// It fails closed (ok=false) when the ray is parallel to the ground or the hit is
// behind the near point — callers must not act on a bogus point. Pure camera math:
// headless-verifiable via a project→unproject round-trip, no GL context needed.
func (gm *game) screenToGround(sx, sy float64, w, h int) (api.Vec2, bool) {
	if w <= 0 || h <= 0 {
		return api.Vec2{}, false
	}
	ndcX := float32(2*sx/float64(w) - 1)
	ndcY := float32(1 - 2*sy/float64(h)) // window Y grows downward; NDC Y grows up
	gm.cam.UpdateMatrixWorld()
	near := gm.cam.Unproject(&math32.Vector3{X: ndcX, Y: ndcY, Z: -1})
	far := gm.cam.Unproject(&math32.Vector3{X: ndcX, Y: ndcY, Z: 1})
	dy := far.Y - near.Y
	if math32.Abs(dy) < 1e-6 {
		return api.Vec2{}, false // ray parallel to the ground
	}
	t := -near.Y / dy
	if t < 0 {
		return api.Vec2{}, false // ground intersection is behind the near point
	}
	hitX := near.X + t*(far.X-near.X)
	hitZ := near.Z + t*(far.Z-near.Z)
	return gm.worldToSim(hitX, hitZ), true
}

// selectNearest selects the unit closest to a picked sim point (left-click).
func (gm *game) selectNearest(pt api.Vec2) {
	us := gm.g.AllUnits(nil)
	best, bestD := -1, 0.0
	for i, u := range us {
		p := u.Position()
		d := (p.X-pt.X)*(p.X-pt.X) + (p.Y-pt.Y)*(p.Y-pt.Y)
		if best < 0 || d < bestD {
			best, bestD = i, d
		}
	}
	if best >= 0 {
		gm.selected = best
		fmt.Printf("event: select-pick unit index=%d at (%.0f,%.0f)\n", best, pt.X, pt.Y)
	}
}

// orderSelectedTo orders the selected unit to move to a picked point (right-click).
func (gm *game) orderSelectedTo(pt api.Vec2) {
	us := gm.g.AllUnits(nil)
	if gm.selected < 0 || gm.selected >= len(us) {
		return
	}
	ok := us[gm.selected].Order(api.OrderMove, api.TargetPoint(pt))
	fmt.Printf("event: order-pick unit=%d to (%.0f,%.0f) accepted=%v\n", gm.selected, pt.X, pt.Y, ok)
}

// rebuildUnits mirrors the live sim units as team-tinted boxes; the selected unit
// gets a bright emissive cap so the selection is visible on screen.
func (gm *game) rebuildUnits() {
	gm.scene.Remove(gm.unitsRoot)
	gm.unitsRoot = core.NewNode()
	gm.scene.Add(gm.unitsRoot)
	us := gm.g.AllUnits(nil)
	for i, u := range us {
		p := u.Position()
		slot := u.Owner().Slot()
		col := teamColors[0]
		if slot >= 0 && slot < len(teamColors) {
			col = teamColors[slot]
		}
		box := graphic.NewMesh(geometry.NewBox(0.6, 1.2, 0.6), material.NewStandard(&col))
		x, z := gm.simToWorld(p)
		box.SetPosition(x, 0.6, z)
		box.SetRotationY(float32(u.Facing().Radians()))
		gm.unitsRoot.Add(box)
		if i == gm.selected {
			capMat := material.NewStandard(&math32.Color{R: 1, G: 0.95, B: 0.4})
			capMat.SetEmissiveColor(&math32.Color{R: 0.9, G: 0.8, B: 0.2})
			selCap := graphic.NewMesh(geometry.NewSphere(0.28, 12, 8), capMat)
			selCap.SetPosition(x, 1.5, z)
			gm.unitsRoot.Add(selCap)
		}
	}
}

// buildMenu makes the pause overlay: a dark translucent quad in front of the
// camera, hidden until paused. Menu actions are keyboard-driven (skeleton; rich
// g3n-gui text + mouse routing is #311 / a follow-up slice).
func (gm *game) buildMenu() {
	gm.menuRoot = core.NewNode()
	mat := material.NewStandard(&math32.Color{R: 0.02, G: 0.02, B: 0.05})
	mat.SetTransparent(true)
	mat.SetOpacity(0.6)
	quad := graphic.NewMesh(geometry.NewPlane(7, 4), mat)
	quad.SetPosition(0, 1.5, 6) // between the camera (z=9) and the field
	gm.menuRoot.Add(quad)
	bar := material.NewStandard(&math32.Color{R: 0.9, G: 0.75, B: 0.2})
	bar.SetEmissiveColor(&math32.Color{R: 0.8, G: 0.65, B: 0.15})
	strip := graphic.NewMesh(geometry.NewPlane(7, 0.4), bar)
	strip.SetPosition(0, 2.6, 6.01)
	gm.menuRoot.Add(strip)
	gm.menuRoot.SetVisible(false)
	gm.scene.Add(gm.menuRoot)
}

// buildHUD adds the on-screen status/command bar: a gui label (built-in font, no
// asset) pinned top-left, refreshed each frame from live game state. This is the
// HUD's data-binding core; the resource bar / command-card widgets (#192) layer on
// once a world ships an economy.
func (gm *game) buildHUD() {
	gm.hud = gui.NewLabel("")
	gm.hud.SetPosition(12, 10)
	gm.hud.SetColor(&math32.Color{R: 0.95, G: 0.95, B: 0.85})
	gm.scene.Add(gm.hud)
}

// hudText is the live HUD string — a deterministic function of game state, so it
// is FSV-able without reading glyphs off a PNG. Shows the clock/tick, run state,
// the selected unit's id/hp/position, the unit count, and the control legend.
func (gm *game) hudText() string {
	us := gm.g.AllUnits(nil)
	sel := "none"
	if gm.selected >= 0 && gm.selected < len(us) {
		u := us[gm.selected]
		p := u.Position()
		sel = fmt.Sprintf("#%d hp=%.0f (%.0f,%.0f)", u.ID(), u.Life(), p.X, p.Y)
	}
	state := "playing"
	if gm.paused {
		state = "PAUSED"
	}
	return fmt.Sprintf("LitD  tick=%d  %s  tod=%.2f  units=%d  selected=%s\n[LMB select · RMB move · Tab cycle · Space fwd · F5/F9 save/load · Esc menu]",
		gm.curTick, state, gm.g.TimeOfDay(), len(us), sel)
}

func (gm *game) setPaused(p bool) {
	gm.paused = p
	gm.menuRoot.SetVisible(p)
	if p {
		fmt.Println("event: PAUSED — [F5] save  [F9] load  [Q] quit  [Esc] resume")
	} else {
		fmt.Println("event: resumed")
	}
}

// --- input ---

func (gm *game) bindInput() {
	gm.app.Subscribe(window.OnKeyDown, func(_ string, ev interface{}) {
		ke := ev.(*window.KeyEvent)
		switch ke.Key {
		case window.KeyEscape:
			gm.setPaused(!gm.paused)
		case window.KeyF5:
			gm.quicksave()
		case window.KeyF9:
			gm.quickload()
		case window.KeyTab:
			gm.cycleSelection()
		case window.KeySpace:
			gm.orderSelectedForward()
		case window.KeyQ:
			if gm.paused {
				fmt.Println("event: quit")
				os.Exit(0)
			}
		}
	})
	// Mouse: left-click selects the unit nearest the picked ground point,
	// right-click orders the selected unit there. Picking is suspended while the
	// pause menu is open.
	gm.app.Subscribe(window.OnMouseDown, func(_ string, ev interface{}) {
		if gm.paused {
			return
		}
		me := ev.(*window.MouseEvent)
		w, h := gm.app.GetSize()
		pt, ok := gm.screenToGround(float64(me.Xpos), float64(me.Ypos), w, h)
		if !ok {
			return
		}
		switch me.Button {
		case window.MouseButtonLeft:
			gm.selectNearest(pt)
		case window.MouseButtonRight:
			gm.orderSelectedTo(pt)
		}
	})
}

func (gm *game) cycleSelection() {
	n := len(gm.g.AllUnits(nil))
	if n == 0 {
		return
	}
	gm.selected = (gm.selected + 1) % n
	fmt.Printf("event: selected unit index=%d\n", gm.selected)
}

// orderSelectedForward issues a Move order to the selected unit, orderReach sim
// units along +X (the keyboard stand-in for a right-click move; mouse-picked
// destinations are a follow-up slice). Returns the target for FSV.
func (gm *game) orderSelectedForward() (api.Vec2, bool) {
	us := gm.g.AllUnits(nil)
	if gm.selected < 0 || gm.selected >= len(us) {
		return api.Vec2{}, false
	}
	u := us[gm.selected]
	p := u.Position()
	target := api.Vec2{X: p.X + orderReach, Y: p.Y}
	ok := u.Order(api.OrderMove, api.TargetPoint(target))
	fmt.Printf("event: order move unit=%d from=(%.0f,%.0f) to=(%.0f,%.0f) accepted=%v\n", gm.selected, p.X, p.Y, target.X, target.Y, ok)
	return target, ok
}

// --- save / load (sim-state quicksave; full Lua-coroutine save is #204) ---

func (gm *game) quicksave() {
	if err := os.MkdirAll(filepath.Dir(gm.savePath), 0o755); err != nil {
		fmt.Printf("event: quicksave FAILED mkdir: %v\n", err)
		return
	}
	f, err := os.Create(gm.savePath)
	if err != nil {
		fmt.Printf("event: quicksave FAILED create: %v\n", err)
		return
	}
	defer f.Close()
	if err := gm.g.SaveState(f, saveFingerprint); err != nil {
		fmt.Printf("event: quicksave FAILED: %v\n", err)
		return
	}
	fmt.Printf("event: quicksave path=%s tick=%d hash=%#x\n", gm.savePath, gm.curTick, gm.g.StateHash())
}

func (gm *game) quickload() bool {
	f, err := os.Open(gm.savePath)
	if err != nil {
		fmt.Printf("event: quickload FAILED open: %v\n", err)
		return false
	}
	defer f.Close()
	if err := gm.g.LoadState(f, saveFingerprint); err != nil {
		fmt.Printf("event: quickload FAILED: %v\n", err)
		return false
	}
	fmt.Printf("event: quickload path=%s hash=%#x\n", gm.savePath, gm.g.StateHash())
	return true
}

// --- frame loop ---

func (gm *game) advanceSim(ticks int) {
	if gm.paused {
		return
	}
	for i := 0; i < ticks; i++ {
		gm.g.Advance(1)
		gm.curTick++
	}
}

func (gm *game) update(rend *renderer.Renderer, dt time.Duration) {
	if gm.autotest {
		gm.autotestStep()
	} else {
		// Real-time loop: accumulate wall time, step the sim at a fixed rate
		// unless paused.
		gm.acc += dt
		tickDur := time.Second / ticksPerSecond
		for gm.acc >= tickDur {
			gm.advanceSim(1)
			gm.acc -= tickDur
		}
	}

	gm.rebuildUnits()
	gm.hud.SetText(gm.hudText())
	gm.day.Update(gm.tod)
	gm.app.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
	if err := rend.Render(gm.scene, gm.cam); err != nil {
		fatalf("render: %v", err)
	}
	if gm.shotPending {
		gm.shotPending = false
		path := filepath.Join(gm.outDir, gm.shotName)
		if err := gm.screenshot(path); err != nil {
			fatalf("screenshot: %v", err)
		}
		fmt.Printf("event: screenshot saved path=%s\n", path)
	}
}

func (gm *game) screenshot(path string) error {
	w, ht := gm.app.GetFramebufferSize()
	data := gm.app.Gls().ReadPixels(0, 0, w, ht, gls.RGBA, gls.UNSIGNED_BYTE)
	img := image.NewRGBA(image.Rect(0, 0, w, ht))
	rowLen := w * 4
	for y := 0; y < ht; y++ {
		copy(img.Pix[y*img.Stride:y*img.Stride+rowLen], data[(ht-1-y)*rowLen:(ht-y)*rowLen])
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

func (gm *game) printState(tag string) {
	us := gm.g.AllUnits(nil)
	rows := make([]map[string]any, 0, len(us))
	for _, u := range us {
		p := u.Position()
		rows = append(rows, map[string]any{"id": u.ID(), "x": p.X, "y": p.Y, "life": u.Life()})
	}
	s := map[string]any{
		"tag": tag, "tick": gm.curTick, "paused": gm.paused,
		"tod": gm.g.TimeOfDay(), "hash": fmt.Sprintf("%#x", gm.g.StateHash()),
		"selected": gm.selected, "units": rows,
	}
	out, _ := json.Marshal(s)
	fmt.Printf("state: %s\n", out)
}

func dist(a, b api.Vec2) float64 {
	dx, dy := a.X-b.X, a.Y-b.Y
	return dx*dx + dy*dy // squared distance; monotonic, enough for "got closer"
}

// autotestStep drives one scripted FSV beat per frame and exits when done. SoT is
// the live sim read through the public api (unit positions, StateHash) plus the
// screenshots.
func (gm *game) autotestStep() {
	switch gm.phase {
	case 0:
		gm.printState("initial")
		gm.shotPending, gm.shotName = true, "game-initial.png"
	case 1:
		us := gm.g.AllUnits(nil)
		if len(us) == 0 {
			fatalf("autotest: world has no units to order")
		}
		gm.selected = 0
		gm.beforePos = us[0].Position()
		gm.target, _ = gm.orderSelectedForward()
		gm.startDist = dist(gm.beforePos, gm.target)
	case 2:
		gm.advanceSim(80)
		after := gm.g.AllUnits(nil)[0].Position()
		nowDist := dist(after, gm.target)
		gm.moveOK = nowDist < gm.startDist && after != gm.beforePos
		fmt.Printf("FSV move: before=(%.0f,%.0f) after=(%.0f,%.0f) target=(%.0f,%.0f) startDist2=%.0f nowDist2=%.0f moved=%v\n",
			gm.beforePos.X, gm.beforePos.Y, after.X, after.Y, gm.target.X, gm.target.Y, gm.startDist, nowDist, gm.moveOK)
		gm.printState("after-order")
		gm.shotPending, gm.shotName = true, "game-moved.png"
	case 3:
		gm.savedHash = gm.g.StateHash()
		gm.quicksave()
		fmt.Printf("FSV save: tick=%d hash=%#x\n", gm.curTick, gm.savedHash)
	case 4:
		gm.advanceSim(40) // diverge from the saved state
		divHash := gm.g.StateHash()
		gm.divergeOK = divHash != gm.savedHash
		fmt.Printf("FSV diverge: advanced 40 ticks, hash=%#x changed=%v\n", divHash, gm.divergeOK)
	case 5:
		ok := gm.quickload()
		restored := gm.g.StateHash()
		gm.loadOK = ok && restored == gm.savedHash
		fmt.Printf("FSV load: restored hash=%#x == saved %#x : %v\n", restored, gm.savedHash, gm.loadOK)
		gm.printState("after-load")
		gm.shotPending, gm.shotName = true, "game-loaded.png"
	case 6:
		gm.setPaused(true)
		tick0 := gm.curTick
		gm.advanceSim(25) // must be a no-op while paused
		gm.pauseOK = gm.curTick == tick0
		fmt.Printf("FSV pause: tick before=%d after advanceSim(25)=%d frozen=%v\n", tick0, gm.curTick, gm.pauseOK)
		gm.shotPending, gm.shotName = true, "game-paused.png"
	case 7:
		gm.setPaused(false)
		pass := gm.moveOK && gm.divergeOK && gm.loadOK && gm.pauseOK
		res := map[string]any{
			"pass": pass, "moveOK": gm.moveOK, "divergeOK": gm.divergeOK,
			"loadOK": gm.loadOK, "pauseOK": gm.pauseOK,
		}
		out, _ := json.Marshal(res)
		fmt.Printf("autotest: %s\n", out)
		if !pass {
			os.Exit(3)
		}
		os.Exit(0)
	}
	gm.phase++
}
