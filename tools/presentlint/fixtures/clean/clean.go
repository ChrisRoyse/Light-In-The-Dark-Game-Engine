//go:build ignore

// Fixture: a well-behaved presentation consumer. It reacts to gameplay by
// draining the render-event snapshot (the non-hashing channel) and installs
// only the presentation sinks (OnAudio/OnCamera), which set a Game field and
// never touch the sim. presentlint must report ZERO findings here.
package clean

type snap struct{ Events []int }

type game struct{}

func (g *game) OnAudio(func()) {}
func (g *game) OnCamera(func()) {}

func consume(g *game, s snap) {
	g.OnAudio(func() {})
	g.OnCamera(func() {})
	for range s.Events { // drain the render-event mirror
		_ = 0
	}
}
