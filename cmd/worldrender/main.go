// Command worldrender is the render-capable FSV harness for an arbitrary world
// (#490). Where cmd/litd loads a world and runs it HEADLESS (state JSON only),
// worldrender loads the same world through litd/worldhost, stands up the g3n
// renderer (the locked RTS camera + day/night lighting + ground reused from the
// cmd/firstlight pattern), advances a scripted list of sim-tick beats, and at
// each beat writes a screenshot PLUS the "state:" JSON line — the same FSV
// contract cmd/firstlight's -autotest uses, generalized to any world directory.
//
// Units render as team-tinted boxes at their sim positions (auto-fit onto the
// ground plane); an optional -beacon-key places a light marker when a world's
// control-point storage flag is set, giving #482 its per-beat rendered evidence.
//
//	-world DIR        world directory to load (contains data/ and main.lua)
//	-archive FILE     verified .litdworld archive to load
//	-beats LIST       comma sim-tick beats to capture, e.g. "20,60,120"
//	-out DIR          screenshot output directory (default artifacts)
//	-seed N           deterministic PRNG seed (R-SIM-2)
//	-budget N         per-eval Lua instruction budget (R-SEC-1)
//	-tod H            fixed time-of-day hour for lighting [0,24) (default 11)
//	-beacon-key CAT:KEY  storage int read per beat; >0 lights a beacon marker
//	-dim-key CAT:KEY  storage int read per beat; >0 dims ambient+sun (Flicker, #500)
//	-dim-factor F     light multiplier applied when -dim-key is set (default 0.30)
//
// Like cmd/firstlight this drives the scripted advance + screenshot from inside
// app.Run's update callback (the g3n lifecycle is fiddly under WSLg; replicate
// firstlight rather than calling renderer.Render bare). Exit 0 = all beats
// captured; non-zero on load/render failure.
package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
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
	"github.com/g3n/engine/light"
	"github.com/g3n/engine/material"
	"github.com/g3n/engine/math32"
	"github.com/g3n/engine/renderer"
	"github.com/g3n/engine/window"
)

const (
	groundSize = 24.0
	fitHalf    = 8.0   // world half-extent the unit cloud is scaled to fit
	minSimHalf = 100.0 // floor on the sim half-span, so a lone unit isn't zoomed in
)

// teamColors maps an owner slot to a box tint; slot 0 / out-of-range = neutral.
var teamColors = []math32.Color{
	{R: 0.55, G: 0.55, B: 0.58}, // 0 neutral grey
	{R: 0.85, G: 0.18, B: 0.18}, // 1 red
	{R: 0.20, G: 0.35, B: 0.85}, // 2 blue
	{R: 0.20, G: 0.70, B: 0.25}, // 3 green
	{R: 0.85, G: 0.80, B: 0.20}, // 4 yellow
}

type beaconKey struct {
	cat, key string
	set      bool
}

type harness struct {
	app   *app.Application
	scene *core.Node
	cam   *camera.Camera
	day   *litrender.DayNight
	g     *api.Game

	unitsRoot  *core.Node // re-parented each beat
	beaconNode *core.Node

	world     string
	outDir    string
	tod       float64
	beats     []int
	beacon    beaconKey
	dim       beaconKey // flicker dim-phase storage flag (#500); CAT:KEY, >0 ⇒ dim
	dimFactor float64   // light multiplier applied on a dim beat
	cx, cy    float64   // sim-space center of the auto-fit transform
	scale     float64   // sim units -> world units
	archive   string

	beatIdx     int
	curTick     int
	shotPending bool
	done        bool
	beatRows    []unitState // rows captured for the current beat's state line
}

// unitState mirrors cmd/litd's per-unit FSV row so a reader can diff the two.
type unitState struct {
	X      float64 `json:"x"`
	Y      float64 `json:"y"`
	Facing float64 `json:"facing"`
	Life   float64 `json:"life"`
	Owner  int     `json:"owner"`
}

func fatalf(format string, a ...any) {
	fmt.Fprintf(os.Stderr, "worldrender: "+format+"\n", a...)
	os.Exit(1)
}

func main() {
	h := &harness{}
	flag.StringVar(&h.world, "world", "", "world directory to load (contains data/ and main.lua)")
	flag.StringVar(&h.archive, "archive", "", "verified .litdworld archive to load")
	beatsStr := flag.String("beats", "60", "comma sim-tick beats to capture, e.g. \"20,60,120\"")
	flag.StringVar(&h.outDir, "out", "artifacts", "screenshot output directory")
	seed := flag.Int64("seed", 1, "deterministic PRNG seed (R-SIM-2)")
	budget := flag.Int64("budget", 50_000_000, "per-eval Lua instruction budget (R-SEC-1)")
	flag.Float64Var(&h.tod, "tod", 11.0, "fixed time-of-day hour for lighting [0,24)")
	beaconKeyStr := flag.String("beacon-key", "", "storage int CAT:KEY read per beat; >0 lights a beacon marker")
	dimKeyStr := flag.String("dim-key", "", "storage int CAT:KEY read per beat; >0 dims ambient+sun (Flicker, #500)")
	flag.Float64Var(&h.dimFactor, "dim-factor", 0.30, "light multiplier applied on a dim beat [0,1]")
	flag.Parse()

	if err := h.validateSource(); err != nil {
		fatalf("%v", err)
	}
	if h.tod < 0 || h.tod >= 24 {
		fatalf("-tod must be in [0,24), got %v", h.tod)
	}
	h.beats = parseBeats(*beatsStr)
	if len(h.beats) == 0 {
		fatalf("no valid beats parsed from %q", *beatsStr)
	}
	if *beaconKeyStr != "" {
		c, k, ok := strings.Cut(*beaconKeyStr, ":")
		if !ok || c == "" || k == "" {
			fatalf("-beacon-key must be CAT:KEY, got %q", *beaconKeyStr)
		}
		h.beacon = beaconKey{cat: c, key: k, set: true}
	}
	if *dimKeyStr != "" {
		c, k, ok := strings.Cut(*dimKeyStr, ":")
		if !ok || c == "" || k == "" {
			fatalf("-dim-key must be CAT:KEY, got %q", *dimKeyStr)
		}
		h.dim = beaconKey{cat: c, key: k, set: true}
	}
	if h.dimFactor < 0 || h.dimFactor > 1 {
		fatalf("-dim-factor must be in [0,1], got %v", h.dimFactor)
	}

	host, err := h.loadHost(*seed, *budget)
	if err != nil {
		fatalf("load %s %q: %v", h.sourceKind(), h.sourcePath(), err)
	}
	defer host.Close()
	h.g = host.Game
	fmt.Printf("event: world loaded source=%s path=%s seed=%d beats=%v\n", h.sourceKind(), h.sourcePath(), *seed, h.beats)

	h.computeFit() // auto-fit transform from the initial unit cloud

	h.app = app.App(1280, 720, "Light in the Dark — worldrender (#490)")
	h.scene = core.NewNode()
	h.buildCamera()
	h.buildLights()
	h.buildGround()
	h.unitsRoot = core.NewNode()
	h.scene.Add(h.unitsRoot)
	h.beaconNode = core.NewNode()
	h.scene.Add(h.beaconNode)

	h.app.Run(h.update)
}

func parseBeats(s string) []int {
	var out []int
	for _, tok := range strings.Split(s, ",") {
		tok = strings.TrimSpace(tok)
		if tok == "" {
			continue
		}
		n, err := strconv.Atoi(tok)
		if err != nil || n < 0 {
			fatalf("bad beat %q (want non-negative int)", tok)
		}
		out = append(out, n)
	}
	sort.Ints(out) // beats are cumulative tick targets; advancing only goes forward
	return out
}

func (h *harness) validateSource() error {
	switch {
	case h.world == "" && h.archive == "":
		return fmt.Errorf("missing -world or -archive")
	case h.world != "" && h.archive != "":
		return fmt.Errorf("pass either -world or -archive, not both")
	default:
		return nil
	}
}

func (h *harness) loadHost(seed, budget int64) (*worldhost.Host, error) {
	if err := h.validateSource(); err != nil {
		return nil, err
	}
	if h.archive != "" {
		return worldhost.LoadArchive(h.archive, engineVersion(), seed, budget)
	}
	return worldhost.Load(h.world, seed, budget)
}

func (h *harness) sourceKind() string {
	if h.archive != "" {
		return "archive"
	}
	return "world"
}

func (h *harness) sourcePath() string {
	if h.archive != "" {
		return h.archive
	}
	return h.world
}

func (h *harness) sourceName() string {
	p := filepath.Base(strings.TrimRight(h.sourcePath(), string(os.PathSeparator)+"/"))
	if h.archive != "" {
		if ext := filepath.Ext(p); ext != "" {
			p = strings.TrimSuffix(p, ext)
		}
	}
	if p == "" || p == "." {
		return h.sourceKind()
	}
	return p
}

// computeFit derives the sim->world transform from the initial unit positions:
// center on their centroid, scale the larger half-span (floored at minSimHalf)
// down to fitHalf world units so motion is visible without filling the screen.
func (h *harness) computeFit() {
	us := h.g.UnitsInRange(api.Vec2{}, 1e9, nil)
	if len(us) == 0 {
		h.cx, h.cy, h.scale = 0, 0, fitHalf/minSimHalf
		fmt.Printf("event: fit no-units center=(0,0) scale=%.5f\n", h.scale)
		return
	}
	minX, maxX := us[0].Position().X, us[0].Position().X
	minY, maxY := us[0].Position().Y, us[0].Position().Y
	for _, u := range us {
		p := u.Position()
		minX, maxX = min(minX, p.X), max(maxX, p.X)
		minY, maxY = min(minY, p.Y), max(maxY, p.Y)
	}
	h.cx, h.cy = (minX+maxX)/2, (minY+maxY)/2
	half := max((maxX-minX)/2, (maxY-minY)/2)
	half = max(half, minSimHalf)
	h.scale = fitHalf / half
	fmt.Printf("event: fit center=(%.1f,%.1f) half=%.1f scale=%.5f units=%d\n", h.cx, h.cy, half, h.scale, len(us))
}

func (h *harness) simToWorld(p api.Vec2) (x, z float32) {
	return float32((p.X - h.cx) * h.scale), float32((p.Y - h.cy) * h.scale)
}

// buildCamera: the locked RTS camera (R-RND-1), same framing as cmd/firstlight.
func (h *harness) buildCamera() {
	h.cam = camera.New(1)
	h.cam.SetPosition(0, 12, 9)
	h.cam.LookAt(&math32.Vector3{X: 0, Y: 0, Z: -1.5}, &math32.Vector3{X: 0, Y: 1, Z: 0})
	h.scene.Add(h.cam)
	onResize := func(string, interface{}) {
		w, ht := h.app.GetSize()
		h.app.Gls().Viewport(0, 0, int32(w), int32(ht))
		h.cam.SetAspect(float32(w) / float32(ht))
	}
	h.app.Subscribe(window.OnWindowSize, onResize)
	onResize("", nil)
}

func (h *harness) buildLights() {
	ambient := light.NewAmbient(&math32.Color{}, 0)
	sun := light.NewDirectional(&math32.Color{}, 0)
	h.scene.Add(ambient)
	h.scene.Add(sun)
	h.day = litrender.NewDayNight(ambient, sun)
	h.day.Update(h.tod)
}

func (h *harness) buildGround() {
	geom := geometry.NewPlane(groundSize, groundSize)
	mat := material.NewStandard(&math32.Color{R: 0.22, G: 0.45, B: 0.20})
	ground := graphic.NewMesh(geom, mat)
	ground.SetRotationX(-math32.Pi / 2)
	h.scene.Add(ground)
}

// rebuildUnits drops the previous beat's boxes and mirrors the live sim units as
// team-tinted boxes at their mapped positions.
func (h *harness) rebuildUnits() []unitState {
	h.scene.Remove(h.unitsRoot)
	h.unitsRoot = core.NewNode()
	h.scene.Add(h.unitsRoot)

	var rows []unitState
	for _, u := range h.g.UnitsInRange(api.Vec2{}, 1e9, nil) {
		p := u.Position()
		slot := u.Owner().Slot()
		col := teamColors[0]
		if slot >= 0 && slot < len(teamColors) {
			col = teamColors[slot]
		}
		box := graphic.NewMesh(geometry.NewBox(0.6, 1.2, 0.6), material.NewStandard(&col))
		x, z := h.simToWorld(p)
		box.SetPosition(x, 0.6, z)
		box.SetRotationY(float32(u.Facing().Radians()))
		h.unitsRoot.Add(box)
		rows = append(rows, unitState{X: p.X, Y: p.Y, Facing: u.Facing().Degrees(), Life: u.Life(), Owner: slot})
	}
	h.updateBeacon()
	return rows
}

// updateBeacon lights a marker at the unit centroid when the world's control
// point flag (-beacon-key) is set (#169 VFX hook). World-agnostic: the world
// names the storage category/key; we only read it.
func (h *harness) updateBeacon() {
	h.scene.Remove(h.beaconNode)
	h.beaconNode = core.NewNode()
	h.scene.Add(h.beaconNode)
	if !h.beacon.set {
		return
	}
	v, ok := h.g.Storage().GetInt(h.beacon.cat, h.beacon.key)
	if !ok || v <= 0 {
		return
	}
	// A bright emissive pillar + point light at the scene origin (the beacon).
	mat := material.NewStandard(&math32.Color{R: 1.0, G: 0.85, B: 0.35})
	mat.SetEmissiveColor(&math32.Color{R: 1.0, G: 0.75, B: 0.25})
	pillar := graphic.NewMesh(geometry.NewCylinder(0.25, 3.0, 12, 1, true, true), mat)
	pillar.SetPosition(0, 1.5, 0)
	h.beaconNode.Add(pillar)
	// The beacon's own point light dims with the flicker phase too (#500): a dim
	// beat shows a fainter beacon glow, not just darker ground.
	pl := light.NewPoint(&math32.Color{R: 1.0, G: 0.8, B: 0.4}, 6.0*h.day.FlickerDim())
	pl.SetPosition(0, 2.5, 0)
	h.beaconNode.Add(pl)
	fmt.Printf("event: beacon lit key=%s:%s value=%d glow=%.2f\n", h.beacon.cat, h.beacon.key, v, 6.0*h.day.FlickerDim())
}

// updateFlickerDim reads the world's flicker-phase storage flag (-dim-key) for
// the current beat and dims the live ambient + sun when it is set (#500). The
// dim persists on the DayNight until the next beat flips it, so each beat's
// screenshot shows the correct bright/dim lighting for that tick. World-agnostic:
// the world names the storage category/key (the flicker-cycle world publishes
// flicker:phase); we only read it and rank the light down.
func (h *harness) updateFlickerDim() {
	if !h.dim.set {
		return
	}
	v, ok := h.g.Storage().GetInt(h.dim.cat, h.dim.key)
	if ok && v > 0 {
		h.day.SetFlickerDim(float32(h.dimFactor))
		fmt.Printf("event: flicker dim ON key=%s:%s value=%d factor=%.2f\n", h.dim.cat, h.dim.key, v, h.dimFactor)
	} else {
		h.day.SetFlickerDim(1)
	}
}

func (h *harness) update(rend *renderer.Renderer, _ time.Duration) {
	if h.done {
		os.Exit(0)
	}
	if !h.shotPending && h.beatIdx < len(h.beats) {
		target := h.beats[h.beatIdx]
		if target > h.curTick {
			h.g.Advance(target - h.curTick)
			h.curTick = target
		}
		h.updateFlickerDim() // set the dim before the beacon is (re)built so its VFX dims too
		h.beatRows = h.rebuildUnits()
		h.shotPending = true
	}

	h.day.Update(h.tod)
	h.app.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
	if err := rend.Render(h.scene, h.cam); err != nil {
		fatalf("render: %v", err)
	}

	if h.shotPending {
		h.shotPending = false
		name := h.sourceName()
		path := filepath.Join(h.outDir, fmt.Sprintf("%s-beat%d.png", name, h.curTick))
		if err := h.screenshot(path); err != nil {
			fatalf("screenshot: %v", err)
		}
		fmt.Printf("event: screenshot saved path=%s beat=%d\n", path, h.curTick)
		h.printState()
		h.beatIdx++
		if h.beatIdx >= len(h.beats) {
			h.done = true
		}
	}
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

// beatRows holds the rows captured for the current beat's state line.
func (h *harness) printState() {
	s := struct {
		TimeOfDay float64     `json:"tod"`
		Tick      int         `json:"tick"`
		Lighting  float64     `json:"lit_hour"`
		Dim       float64     `json:"flicker_dim"` // applied light multiplier (1 = bright)
		Units     []unitState `json:"units"`
	}{h.g.TimeOfDay(), h.curTick, h.tod, float64(h.day.FlickerDim()), h.beatRows}
	out, _ := json.Marshal(s)
	fmt.Printf("state: %s\n", out)
}

// screenshot reads the framebuffer and writes a PNG (R-FSV-1), same flip as
// cmd/firstlight.
func (h *harness) screenshot(path string) error {
	w, ht := h.app.GetFramebufferSize()
	data := h.app.Gls().ReadPixels(0, 0, w, ht, gls.RGBA, gls.UNSIGNED_BYTE)
	img := image.NewRGBA(image.Rect(0, 0, w, ht))
	rowLen := w * 4
	for y := 0; y < ht; y++ {
		src := data[(ht-1-y)*rowLen : (ht-y)*rowLen]
		copy(img.Pix[y*img.Stride:y*img.Stride+rowLen], src)
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
