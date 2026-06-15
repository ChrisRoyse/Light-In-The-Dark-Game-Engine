package litd

// #248 camera public-API FSV. The camera is render-only and sim-inert: SoTs
// are (a) the state hash, which must be identical before/after any camera
// script, and (b) the per-player applied-field cache + a recording sink, which
// make clamping and the local/non-local gate observable headlessly.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func cameraWorld(t *testing.T) (*sim.World, *Game) {
	t.Helper()
	w := sim.NewWorld(sim.Caps{Units: 8})
	return w, newGame(w)
}

// TestCameraClampVsCinematicFSV — issue edge (1): a gameplay-mode field write
// is clamped per R-RND-1; the same write inside a cinematic applies verbatim.
func TestCameraClampVsCinematicFSV(t *testing.T) {
	w, g := cameraWorld(t)
	g.SetLocalPlayer(g.Player(0))
	cam := g.Camera(g.Player(0))

	cam.SetZoom(99999) // gameplay: clamp to max (5000)
	applied := cam.Field(CameraTargetDistance)
	t.Logf("FSV gameplay SetZoom(99999) -> applied=%.0f (want 5000 clamp)", applied)
	if applied != 5000 {
		t.Fatalf("gameplay zoom not clamped: %.0f", applied)
	}
	cam.SetField(CameraAngleOfAttack, -30) // clamp to min (0)
	if cam.Field(CameraAngleOfAttack) != 0 {
		t.Fatalf("angle not clamped to 0: %.0f", cam.Field(CameraAngleOfAttack))
	}

	cam.BeginCinematic()
	cam.SetZoom(99999) // cinematic: verbatim
	t.Logf("FSV cinematic SetZoom(99999) -> applied=%.0f (want 99999 verbatim)", cam.Field(CameraTargetDistance))
	if cam.Field(CameraTargetDistance) != 99999 {
		t.Fatalf("cinematic zoom should be verbatim: %.0f", cam.Field(CameraTargetDistance))
	}
	cam.EndCinematic()
	cam.SetZoom(99999) // back to gameplay clamp
	if cam.Field(CameraTargetDistance) != 5000 {
		t.Fatalf("post-cinematic clamp not restored: %.0f", cam.Field(CameraTargetDistance))
	}
	_ = w
}

// TestCameraNonLocalNoOpFSV — issue edge (2): a camera verb for a non-local
// player records nothing and leaves that player's view state untouched.
func TestCameraNonLocalNoOpFSV(t *testing.T) {
	_, g := cameraWorld(t)
	g.SetLocalPlayer(g.Player(0))
	var rec []CameraEvent
	g.OnCamera(func(ev CameraEvent) { rec = append(rec, ev) })

	g.Camera(g.Player(1)).SetZoom(1000) // non-local: recorded no-op
	g.Camera(g.Player(1)).Pan(Vec2{X: 1, Y: 2})
	t.Logf("FSV non-local: events=%d p1.zoom=%.0f (want 0/0)", len(rec), g.Camera(g.Player(1)).Field(CameraTargetDistance))
	if len(rec) != 0 || g.Camera(g.Player(1)).Field(CameraTargetDistance) != 0 {
		t.Fatalf("non-local camera leaked: events=%d zoom=%.0f", len(rec), g.Camera(g.Player(1)).Field(CameraTargetDistance))
	}

	g.Camera(g.Player(0)).SetZoom(1000) // local: applies + emits
	t.Logf("FSV local: events=%d p0.zoom=%.0f (want 1/1000)", len(rec), g.Camera(g.Player(0)).Field(CameraTargetDistance))
	if len(rec) != 1 || g.Camera(g.Player(0)).Field(CameraTargetDistance) != 1000 {
		t.Fatal("local camera did not apply")
	}
}

// TestCameraSimInertFSV — issue edge (3): camera calls never change the state
// hash, with or without a local viewer / sink.
func TestCameraSimInertFSV(t *testing.T) {
	w, g := cameraWorld(t)
	if _, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, 0); !ok {
		t.Fatal("CreateUnit failed")
	}
	g.SetLocalPlayer(g.Player(0))
	g.OnCamera(func(ev CameraEvent) {})
	before := hashTop(w)
	cam := g.Camera(g.Player(0))
	cam.Pan(Vec2{X: 500, Y: 500}, PanOver(2), PanHeight(100))
	cam.SetZoom(1600)
	cam.Shake(5)
	cam.SetBounds(NewRect(Vec2{X: 0, Y: 0}, Vec2{X: 1000, Y: 1000}))
	cam.BeginCinematic()
	cam.SetField(CameraRotation, 45)
	cam.EndCinematic()
	after := hashTop(w)
	t.Logf("FSV camera sim-inert: hash before=%016x after=%016x", before, after)
	if before != after {
		t.Fatalf("camera mutated sim state: %016x -> %016x", before, after)
	}
}

// TestCameraFollowReleasedOnDeathFSV — issue edge (4): following a unit that
// then dies releases the lock cleanly (no panic).
func TestCameraFollowReleasedOnDeathFSV(t *testing.T) {
	w, g := cameraWorld(t)
	g.SetLocalPlayer(g.Player(0))
	id, ok := w.CreateUnit(fixed.Vec2{X: fixed.FromInt(64), Y: fixed.FromInt(64)}, 0)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	u := Unit{id: id, g: g}
	cam := g.Camera(g.Player(0))
	cam.Follow(u)
	t.Logf("FSV following before death: valid=%v", cam.Following().Valid())
	if !cam.Following().Valid() {
		t.Fatal("Follow did not lock on")
	}
	u.Remove() // unit ceases to exist
	released := cam.Following()
	t.Logf("FSV following after death: valid=%v (want false, no panic)", released.Valid())
	if released.Valid() {
		t.Fatal("camera should release a dead follow target")
	}

	// edge: zero-value camera verbs are safe no-ops.
	var zero Camera
	zero.SetZoom(100)
	zero.Follow(u)
	if zero.Following().Valid() || zero.Valid() {
		t.Fatal("zero-value Camera must be a safe no-op")
	}
}
