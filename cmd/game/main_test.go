package main

// #531 mouse-pick FSV. SoT = the sim point screenToGround recovers from a pixel,
// cross-checked against the point we forward-projected to that pixel. The mouse
// EVENT can't be driven headless, but the pick MATH (camera ray -> ground plane ->
// sim units) is the error-prone part and is fully verifiable via a project ->
// screen -> unproject round-trip with no GL context. X+X=Y: project sim point P to
// a pixel, pick that pixel, get P back.

import (
	"math"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/g3n/engine/camera"
	"github.com/g3n/engine/math32"
)

func pickTestGame() *game {
	gm := &game{cx: 320, cy: 256, scale: 0.08}
	gm.cam = camera.New(1280.0 / 720.0)
	gm.cam.SetPosition(0, 12, 9)
	gm.cam.LookAt(&math32.Vector3{X: 0, Y: 0, Z: -1.5}, &math32.Vector3{X: 0, Y: 1, Z: 0})
	gm.cam.UpdateMatrixWorld()
	return gm
}

// forwardProject maps a sim point to the window pixel the renderer would draw it
// at — the inverse direction of screenToGround, used to generate known pixels.
func (gm *game) forwardProject(p api.Vec2, w, h int) (sx, sy float64) {
	wx, wz := gm.simToWorld(p)
	v := &math32.Vector3{X: wx, Y: 0, Z: wz}
	gm.cam.UpdateMatrixWorld()
	gm.cam.Project(v) // -> NDC in v
	sx = (float64(v.X) + 1) / 2 * float64(w)
	sy = (1 - float64(v.Y)) / 2 * float64(h)
	return sx, sy
}

func TestWorldToSimInverseFSV(t *testing.T) {
	gm := &game{cx: 320, cy: 256, scale: 0.08}
	// simToWorld returns float32, so the round-trip is inverse to float32 precision
	// (~1e-4 absolute at these magnitudes), not bit-exact — a sub-milliunit pick is
	// far finer than any gameplay needs.
	const tol = 1e-3
	for _, p := range []api.Vec2{{X: 320, Y: 256}, {X: 400, Y: 100}, {X: -50, Y: 600}} {
		x, z := gm.simToWorld(p)
		back := gm.worldToSim(x, z)
		t.Logf("FSV simToWorld inverse: %+v -> world(%.3f,%.3f) -> %+v", p, x, z, back)
		if math.Abs(back.X-p.X) > tol || math.Abs(back.Y-p.Y) > tol {
			t.Fatalf("worldToSim(simToWorld(%+v)) = %+v, not an inverse within %.0e", p, back, tol)
		}
	}
}

func TestScreenToGroundRoundTripFSV(t *testing.T) {
	gm := pickTestGame()
	const W, H = 1280, 720
	// Points spread around the screen centre (sim (320,256) <-> world origin),
	// each well inside the down-looking camera frame.
	points := []api.Vec2{
		{X: 320, Y: 256}, // centre
		{X: 360, Y: 256}, // right
		{X: 320, Y: 312}, // toward camera
		{X: 280, Y: 200}, // up-left
		{X: 405, Y: 330}, // far corner-ish
	}
	const tol = 1.0 // sim units; round-trip should be sub-unit
	for _, p := range points {
		sx, sy := gm.forwardProject(p, W, H)
		got, ok := gm.screenToGround(sx, sy, W, H)
		t.Logf("FSV pick: sim%+v -> pixel(%.1f,%.1f) -> sim%+v ok=%v", p, sx, sy, got, ok)
		if !ok {
			t.Fatalf("screenToGround returned ok=false for an on-screen point %+v", p)
		}
		if math.Abs(got.X-p.X) > tol || math.Abs(got.Y-p.Y) > tol {
			t.Fatalf("pick round-trip %+v -> %+v exceeds tol %.2f", p, got, tol)
		}
	}
}

func TestScreenToGroundEdgesFSV(t *testing.T) {
	gm := pickTestGame()
	// Empty viewport fails closed.
	if _, ok := gm.screenToGround(640, 360, 0, 0); ok {
		t.Fatal("screenToGround with a 0x0 viewport returned ok=true, want fail-closed")
	}
	// A camera looking horizontally makes the centre ray parallel to the ground —
	// no intersection, must fail closed (not return a garbage/NaN point).
	flat := &game{cx: 0, cy: 0, scale: 1}
	flat.cam = camera.New(1280.0 / 720.0)
	flat.cam.SetPosition(0, 5, 9)
	flat.cam.LookAt(&math32.Vector3{X: 0, Y: 5, Z: 0}, &math32.Vector3{X: 0, Y: 1, Z: 0}) // horizontal gaze
	flat.cam.UpdateMatrixWorld()
	pt, ok := flat.screenToGround(640, 360, 1280, 720)
	t.Logf("FSV parallel-ray: ok=%v pt=%+v", ok, pt)
	if ok {
		t.Fatalf("parallel ray returned ok=true (%+v), want fail-closed", pt)
	}
}
