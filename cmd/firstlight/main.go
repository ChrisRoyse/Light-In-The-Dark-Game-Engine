// Command firstlight is the M0.5 "First Light" demo (PRD §7):
// a window, a ground plane, one unit, player control.
//
//   - Left-click the unit to select it (yellow ring), left-click ground to deselect.
//   - Right-click the ground to order the unit to move there.
//   - F12 saves a screenshot to firstlight.png.
//
// Verification flags (FSV protocol, PRD §5.5 / R-FSV-1..3):
//
//	-autotest        scripted run: orders the unit to a known target, waits for
//	                 arrival, prints final state as JSON, captures a screenshot,
//	                 and exits 0 (non-zero on timeout).
//	-shot PATH       screenshot output path (default artifacts/firstlight.png)
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
	"os"
	"path/filepath"
	"time"

	"github.com/g3n/engine/app"
	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/core"
	"github.com/g3n/engine/experimental/collision"
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
	groundSize  = 24.0
	unitSpeed   = 4.0 // world units per second
	arriveEps   = 0.05
	autoTargetX = 5.0
	autoTargetZ = -4.0
	autoTimeout = 10 * time.Second
)

type demo struct {
	app    *app.Application
	scene  *core.Node
	cam    *camera.Camera
	ground *graphic.Mesh
	unit   *graphic.Mesh
	ring   *graphic.Mesh

	selected  bool
	hasTarget bool
	target    math32.Vector3

	shotPath    string
	autotest    bool
	autoOrdered bool
	autoStart   time.Time
	shotPending bool
}

// state is the FSV state dump printed by -autotest (R-FSV-2).
type state struct {
	UnitX, UnitZ     float64
	TargetX, TargetZ float64
	HasTarget        bool
	Selected         bool
	ArrivedAtTarget  bool
}

func main() {
	d := &demo{}
	flag.StringVar(&d.shotPath, "shot", "artifacts/firstlight.png", "screenshot output path")
	flag.BoolVar(&d.autotest, "autotest", false, "scripted FSV run: order unit, verify arrival, screenshot, exit")
	flag.Parse()

	d.app = app.App(1280, 720, "Light in the Dark — First Light (M0.5)")
	d.scene = core.NewNode()

	d.buildCamera()
	d.buildLights()
	d.buildGround()
	d.buildUnit()
	d.installInput()

	d.autoStart = time.Now()
	d.app.Run(d.update)
}

// buildCamera creates the locked RTS camera (PRD R-RND-1): fixed yaw,
// ~34 degree pitch from vertical, no orbit control.
func (d *demo) buildCamera() {
	d.cam = camera.New(1)
	d.cam.SetPosition(0, 14, 9.5)
	d.cam.LookAt(&math32.Vector3{X: 0, Y: 0, Z: 0}, &math32.Vector3{X: 0, Y: 1, Z: 0})
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
	d.scene.Add(light.NewAmbient(&math32.Color{R: 1, G: 1, B: 1}, 0.5))
	sun := light.NewDirectional(&math32.Color{R: 1, G: 1, B: 1}, 0.9)
	sun.SetPosition(10, 20, 10)
	d.scene.Add(sun)
}

func (d *demo) buildGround() {
	geom := geometry.NewPlane(groundSize, groundSize)
	mat := material.NewStandard(&math32.Color{R: 0.22, G: 0.45, B: 0.20})
	d.ground = graphic.NewMesh(geom, mat)
	d.ground.SetRotationX(-math32.Pi / 2)
	d.scene.Add(d.ground)
}

func (d *demo) buildUnit() {
	geom := geometry.NewBox(0.6, 1.2, 0.6)
	mat := material.NewStandard(&math32.Color{R: 0.15, G: 0.30, B: 0.85})
	d.unit = graphic.NewMesh(geom, mat)
	d.unit.SetPosition(0, 0.6, 0)
	d.scene.Add(d.unit)

	ringGeom := geometry.NewTorus(0.7, 0.05, 8, 24, 2*math32.Pi)
	ringMat := material.NewStandard(&math32.Color{R: 1.0, G: 0.9, B: 0.1})
	d.ring = graphic.NewMesh(ringGeom, ringMat)
	d.ring.SetRotationX(-math32.Pi / 2)
	d.ring.SetPosition(0, -0.55, 0) // relative to unit center, just above ground
	d.ring.SetVisible(false)
	d.unit.Add(d.ring)
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
		if kev.Key == window.KeyF12 {
			d.shotPending = true
		}
	})
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
	if len(d.raycast(x, y, d.unit)) > 0 {
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
	d.target = math32.Vector3{X: x, Y: 0.6, Z: z}
	d.hasTarget = true
	fmt.Printf("event: move order issued target=(%.2f, %.2f)\n", x, z) // R-FSV-3
}

func (d *demo) update(rend *renderer.Renderer, deltaTime time.Duration) {
	d.stepMovement(float32(deltaTime.Seconds()))
	d.runAutotest()

	d.app.Gls().Clear(gls.DEPTH_BUFFER_BIT | gls.STENCIL_BUFFER_BIT | gls.COLOR_BUFFER_BIT)
	if err := rend.Render(d.scene, d.cam); err != nil {
		fmt.Fprintf(os.Stderr, "render error: %v\n", err)
		os.Exit(1)
	}

	if d.shotPending {
		d.shotPending = false
		if err := d.screenshot(d.shotPath); err != nil {
			fmt.Fprintf(os.Stderr, "screenshot error: %v\n", err)
			os.Exit(1)
		}
		fmt.Printf("event: screenshot saved path=%s\n", d.shotPath)
		if d.autotest {
			d.finishAutotest()
		}
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
		d.unit.SetPosition(d.target.X, 0.6, d.target.Z)
		d.hasTarget = false
		fmt.Printf("event: arrived pos=(%.2f, %.2f)\n", d.target.X, d.target.Z)
		return
	}
	step := unitSpeed * dt
	if step > dist {
		step = dist
	}
	delta.Normalize().MultiplyScalar(step)
	d.unit.SetPosition(pos.X+delta.X, 0.6, pos.Z+delta.Z)
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

func (d *demo) finishAutotest() {
	pos := d.unit.Position()
	s := state{
		UnitX: float64(pos.X), UnitZ: float64(pos.Z),
		TargetX: autoTargetX, TargetZ: autoTargetZ,
		HasTarget: d.hasTarget, Selected: d.selected,
		ArrivedAtTarget: math32.Abs(pos.X-autoTargetX) < 0.1 && math32.Abs(pos.Z-autoTargetZ) < 0.1,
	}
	out, _ := json.Marshal(s)
	fmt.Printf("state: %s\n", out)
	if !s.ArrivedAtTarget {
		os.Exit(3)
	}
	os.Exit(0)
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
