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
