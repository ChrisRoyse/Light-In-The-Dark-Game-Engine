package litd_test

// Runnable godoc examples for the core noun types (#259 G-4). `go test` executes
// each and checks its Output against the live sim — so these double as headless
// usage documentation that cannot rot silently. They are deliberately minimal:
// the canonical first contact with Game, Unit, Player, Vec2, and the event
// contract.

import (
	"bytes"
	"fmt"
	"time"

	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// exampleGame builds a headless game with one unit type registered ("hfoo").
func exampleGame() *litd.Game {
	g, err := litd.NewGame(litd.GameOptions{MaxUnits: 16, Seed: 1})
	if err != nil {
		panic(err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		panic(err)
	}
	return g
}

// A fresh game has valid player slots and no error.
func ExampleNewGame() {
	g, err := litd.NewGame(litd.GameOptions{MaxUnits: 8, Seed: 42})
	fmt.Println(err == nil, g.Player(0).Valid())
	// Output: true true
}

// CreateUnit spawns a unit for a player at a world position; the handle reads
// back its live sim state.
func ExampleGame_CreateUnit() {
	g := exampleGame()
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), litd.Vec2{X: 64, Y: 96}, litd.Deg(0))
	p := u.Position()
	fmt.Printf("at (%.0f, %.0f) life %.0f valid=%v\n", p.X, p.Y, u.Life(), u.Valid())
	// Output: at (64, 96) life 100 valid=true
}

// SetLife writes a unit's current life; the change is immediately observable.
func ExampleUnit_SetLife() {
	g := exampleGame()
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), litd.Vec2{}, litd.Deg(0))
	u.SetLife(33)
	fmt.Printf("%.0f\n", u.Life())
	// Output: 33
}

// Vec2 is a plain value type with allocation-free math.
func ExampleVec2() {
	a := litd.Vec2{X: 1, Y: 2}
	b := litd.Vec2{X: 3, Y: 4}
	fmt.Println(a.Add(b), a.Sub(b))
	// Output: {4 6} {-2 -2}
}

// OnEvent is the modder event contract: subscribe to a sim event kind, then the
// handler fires when the sim emits it during a step. Here a unit death is
// counted after the kill is processed by Advance.
func ExampleGame_OnEvent() {
	g := exampleGame()
	deaths := 0
	g.OnEvent(litd.EventUnitDeath, func(litd.Event) { deaths++ })
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), litd.Vec2{}, litd.Deg(0))
	u.Kill()
	g.Advance(1)
	fmt.Println("deaths:", deaths)
	// Output: deaths: 1
}

// SetName writes a player's display name (the writable companion to Name).
func ExamplePlayer_SetName() {
	g := exampleGame()
	p := g.Player(0)
	p.SetName("Commander")
	fmt.Println(p.Name())
	// Output: Commander
}

// After schedules a one-shot callback on the deterministic scheduler; it fires
// when game time reaches the delay, advanced by stepping the sim (20 ticks/s).
func ExampleGame_After() {
	g := exampleGame()
	fired := false
	g.After(1*time.Second, func() { fired = true })
	fmt.Println("before:", fired)
	g.Advance(20)
	fmt.Println("after:", fired)
	// Output:
	// before: false
	// after: true
}

// Storage is the campaign key-value store; Save/Load round-trips it through any
// byte stream, so cross-map state survives between worlds.
func ExampleStorage() {
	g := exampleGame()
	g.Storage().SetInt("campaign", "gold_banked", 500)

	var buf bytes.Buffer
	if err := g.Storage().Save(&buf); err != nil {
		panic(err)
	}

	// A fresh game (e.g. the next map) loads the persisted store.
	next := exampleGame()
	if err := next.Storage().Load(&buf); err != nil {
		panic(err)
	}
	v, ok := next.Storage().GetInt("campaign", "gold_banked")
	fmt.Println(v, ok)
	// Output: 500 true
}

// A Region is a script-defined area; AddRect fills it with the grid cells a
// rectangle overlaps, and Contains answers point membership.
func ExampleRegion() {
	g := exampleGame()
	rg := g.NewRegion()
	rg.AddRect(litd.NewRect(litd.Vec2{X: 0, Y: 0}, litd.Vec2{X: 300, Y: 300}))
	fmt.Println(rg.Contains(litd.Vec2{X: 150, Y: 150}), rg.Contains(litd.Vec2{X: 500, Y: 500}))
	// Output: true false
}

// UnitsInRange returns the units within a radius of a point; a nil filter
// accepts all. The query reads live sim positions.
func ExampleGame_UnitsInRange() {
	g := exampleGame()
	g.CreateUnit(g.Player(0), g.UnitType("hfoo"), litd.Vec2{X: 100, Y: 100}, litd.Deg(0))
	near := g.UnitsInRange(litd.Vec2{X: 100, Y: 100}, 50, nil)
	far := g.UnitsInRange(litd.Vec2{X: 1000, Y: 1000}, 50, nil)
	fmt.Println(len(near), len(far))
	// Output: 1 0
}

// A UnitFilter narrows a query: here, only units owned by player 5 (there are
// none), so the result is empty.
func ExampleGame_UnitsInRange_filter() {
	g := exampleGame()
	g.CreateUnit(g.Player(0), g.UnitType("hfoo"), litd.Vec2{X: 100, Y: 100}, litd.Deg(0))
	owned := g.UnitsInRange(litd.Vec2{X: 100, Y: 100}, 50, func(v litd.UnitView) bool {
		return v.OwnerPlayer() == 5
	})
	fmt.Println(len(owned))
	// Output: 0
}

// Rect is an axis-aligned world rectangle with value-type geometry helpers.
func ExampleRect() {
	r := litd.NewRect(litd.Vec2{X: 0, Y: 0}, litd.Vec2{X: 200, Y: 100})
	c := r.Center()
	fmt.Printf("center (%.0f,%.0f) w=%.0f h=%.0f contains(50,50)=%v\n",
		c.X, c.Y, r.Width(), r.Height(), r.Contains(litd.Vec2{X: 50, Y: 50}))
	// Output: center (100,50) w=200 h=100 contains(50,50)=true
}

// Angle is a value type in radians with degree accessors; Normalized folds into
// a single turn.
func ExampleAngle() {
	fmt.Printf("%.0f %.0f\n", litd.Deg(90).Degrees(), litd.Deg(450).Normalized().Degrees())
	// Output: 90 90
}

// Force is a stateful player group; AddPlayer builds the membership set.
func ExampleGame_CreateForce() {
	g := exampleGame()
	f := g.CreateForce()
	f.AddPlayer(g.Player(0))
	f.AddPlayer(g.Player(1))
	fmt.Println(f.Count())
	// Output: 2
}

// An ability is registered once, equipped on a unit, then leveled — the handle
// reads back its live level.
func ExampleUnit_AddAbility() {
	g := exampleGame()
	ref := g.RegisterAbility(litd.AbilityDef{ID: "AHbz", Name: "Blizzard", Cooldown: 6})
	u := g.CreateUnit(g.Player(0), g.UnitType("hfoo"), litd.Vec2{}, litd.Deg(0))
	a := u.AddAbility(ref)
	a.SetLevel(3)
	fmt.Println(a.Valid(), a.Level())
	// Output: true 3
}

// The camera is sim-inert render state: a verb for the local player emits a
// CameraEvent to the installed sink without touching the simulation.
func ExampleGame_Camera() {
	g := exampleGame()
	var got litd.CameraEvent
	g.OnCamera(func(e litd.CameraEvent) { got = e })
	g.SetLocalPlayer(g.Player(0))
	g.Camera(g.Player(0)).Pan(litd.Vec2{X: 512, Y: 256})
	fmt.Printf("%v (%.0f,%.0f)\n", got.Kind == litd.CameraPan, got.Pos.X, got.Pos.Y)
	// Output: true (512,256)
}
