package helpers

import (
	"time"

	litd "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

// Transmit shows a timed transmission message to the given recipients: the
// speaker's name (when non-empty) is prefixed to the line, and the message
// stays on screen for d (the engine default when d <= 0). It is the D4 keep
// for the TransmissionFromUnitWithName / cinematic-transmission BJ family —
// the logic-bearing part of which (who hears what line, for how long) is
// the sim-inert text presentation reimplemented here on Game.Print + the
// For() duration option.
//
// The portrait/sound side of a WC3 transmission is render-only audio
// (#244, the AudioEvent surface) and is deliberately NOT bundled here:
// Transmit is the deterministic, headless-verifiable text path; a caller
// that also wants the portrait sound layers it via the audio verbs. A
// nil/empty recipient list is a no-op (no "to all" footgun — recipients are
// always explicit), mirroring Game.Print.
func Transmit(g *litd.Game, to []litd.Player, speaker, msg string, d time.Duration) {
	if g == nil || len(to) == 0 {
		return
	}
	line := msg
	if speaker != "" {
		line = speaker + ": " + msg
	}
	if d > 0 {
		g.Print(to, line, litd.For(d.Seconds()))
		return
	}
	g.Print(to, line)
}
