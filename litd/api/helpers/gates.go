package helpers

import (
	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// WidgetOptions configures a Gate or Elevator widget. Type is the
// destructable content id; Pos/Facing place it; Life seeds current and max
// life; Footprint is the square pathing-footprint side in cells (only
// meaningful for the pathing-blocking Gate).
type WidgetOptions struct {
	Type      uint16
	Pos       litd.Vec2
	Facing    litd.Angle
	Life      int
	Footprint int
}

// Gate creates a closed gate: a destructable that BLOCKS pathing over its
// footprint until it is destroyed (killing it frees the footprint the same
// tick — the sim is the source of truth). It is the D4 keep for the WC3
// gate widgets, whose distinguishing logic over a plain destructable is the
// pathing-blocking default; opening/closing a gate is then Destructable
// life/kill/restore on the returned handle. Built purely on
// Game.CreateDestructable.
func Gate(g *litd.Game, o WidgetOptions) litd.Destructable {
	return g.CreateDestructable(litd.DestructableOptions{
		Type:          o.Type,
		Pos:           o.Pos,
		Facing:        o.Facing,
		Life:          o.Life,
		BlocksPathing: true,
		Footprint:     o.Footprint,
	})
}

// Elevator creates an elevator platform: a destructable that does NOT block
// pathing (units walk across it), distinguishing it from a Gate. It is the
// D4 keep for the WC3 elevator/pathing-platform widgets; the logic-bearing
// difference is the passable default. Built purely on
// Game.CreateDestructable.
func Elevator(g *litd.Game, o WidgetOptions) litd.Destructable {
	return g.CreateDestructable(litd.DestructableOptions{
		Type:          o.Type,
		Pos:           o.Pos,
		Facing:        o.Facing,
		Life:          o.Life,
		BlocksPathing: false,
	})
}
