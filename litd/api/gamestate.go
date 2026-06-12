package litd

// Game-level lifecycle and static match configuration
// (jass-mapping/game-state-and-melee.md). Pause and speed are
// driver-owned controls: they affect real-time tick feeding, never sim
// tick content or hash state.

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

type driverHook interface {
	SetPaused(bool)
	SetSpeed(float64) bool
}

// GameSpeed is the public real-time-to-tick scale accepted by SetSpeed.
type GameSpeed float64

const (
	GameSpeedSlow   GameSpeed = 0.8
	GameSpeedNormal GameSpeed = 1.0
	GameSpeedFast   GameSpeed = 1.25
)

// MapFlag is a static match/map flag. Values mirror common.j mapflag
// bit values while using Go names.
type MapFlag uint32

const (
	MapFlagFogHideTerrain            MapFlag = 1
	MapFlagFogMapExplored            MapFlag = 2
	MapFlagFogAlwaysVisible          MapFlag = 4
	MapFlagUseHandicaps              MapFlag = 8
	MapFlagObservers                 MapFlag = 16
	MapFlagObserversOnDeath          MapFlag = 32
	MapFlagFixedColors               MapFlag = 128
	MapFlagLockResourceTrading       MapFlag = 256
	MapFlagResourceTradingAlliesOnly MapFlag = 512
	MapFlagLockAllianceChanges       MapFlag = 1024
	MapFlagAllianceChangesHidden     MapFlag = 2048
	MapFlagCheats                    MapFlag = 4096
	MapFlagCheatsHidden              MapFlag = 8192
	MapFlagLockSpeed                 MapFlag = 8192 * 2
	MapFlagLockRandomSeed            MapFlag = 8192 * 4
	MapFlagSharedAdvancedControl     MapFlag = 8192 * 8
	MapFlagRandomHero                MapFlag = 8192 * 16
	MapFlagRandomRaces               MapFlag = 8192 * 32
	MapFlagReloaded                  MapFlag = 8192 * 64
)

type matchConfig struct {
	flags          MapFlag
	startLocations [sim.MaxPlayers]Vec2
	teams          int
}

// Pause stops the driver from feeding ticks to the sim. Render can keep
// drawing with the driver's frozen interpolation alpha. No-op when no
// driver hook is attached; debug mode reports that setup error.
func (g *Game) Pause() {
	if g == nil || g.driver == nil {
		g.reportInvalid("Game.Pause")
		return
	}
	g.driver.SetPaused(true)
}

// Resume lets the driver feed ticks again. It does not catch up wall
// time spent paused.
func (g *Game) Resume() {
	if g == nil || g.driver == nil {
		g.reportInvalid("Game.Resume")
		return
	}
	g.driver.SetPaused(false)
}

// SetSpeed sets the driver's real-time-to-tick scale. The fixed sim
// tick remains 20 Hz and tick content is unchanged; bad speeds are
// refused by the driver and reported in debug mode. JASS: SetGameSpeed.
func (g *Game) SetSpeed(s GameSpeed) {
	if g == nil || g.driver == nil {
		g.reportInvalid("Game.SetSpeed")
		return
	}
	if !g.driver.SetSpeed(float64(s)) {
		g.reportInvalid("Game.SetSpeed")
	}
}

// MapFlag reports whether f is set in the static match config. Nil
// games and the zero flag fail closed to false. JASS: IsMapFlagSet.
func (g *Game) MapFlag(f MapFlag) bool {
	if g == nil || f == 0 {
		return false
	}
	return g.match.flags&f == f
}

// StartLocation returns the configured start location for index i, or
// the zero vector when i is outside the fixed player-slot range. JASS:
// GetStartLocationX/Y.
func (g *Game) StartLocation(i int) Vec2 {
	if g == nil || i < 0 || i >= sim.MaxPlayers {
		if g != nil {
			g.reportInvalid("Game.StartLocation")
		}
		return Vec2{}
	}
	return g.match.startLocations[i]
}

// Teams returns the configured team count, or 0 before map metadata is
// loaded. JASS: SetTeams readback for the public melee helper surface.
func (g *Game) Teams() int {
	if g == nil {
		return 0
	}
	return g.match.teams
}

// IsReplay reports whether the current match is replay playback. The
// replay/net layer is not wired into Game yet, so the API fails closed
// to false for now.
func (g *Game) IsReplay() bool { return false }
