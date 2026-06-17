package litd

import "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"

// NewGameForTest exposes the unexported newGame seam to the external
// litd_test package (and, via it, to the helpers dogfood tests), so a test
// can obtain a *Game over a hand-built headless sim world without a public
// constructor existing yet. Compiled only into the test binary. The
// production public setup path (NewGame/map load) lands with its own issue
// (#201 game flow); this is strictly a test seam, not shipped API.
func NewGameForTest(w *sim.World) *Game { return newGame(w) }
