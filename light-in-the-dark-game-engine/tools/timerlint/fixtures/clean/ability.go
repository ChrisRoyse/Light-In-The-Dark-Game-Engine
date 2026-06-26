//go:build ignore

// Fixture: a well-behaved ability template. It schedules time only via the
// serializable continuation API, so it survives save/load. timerlint must
// find nothing here.
package clean

type game struct{}

type cont uint16
type payload struct{ A, B, C, D int64 }

func (g *game) AfterCont(d int, c cont, p payload)        {}
func (g *game) LoopCont(d int, c cont, p payload)         {}
func (g *game) CountCont(d, n int, c cont, p payload)     {}
func (g *game) RegisterCont(c cont, fn func(*game, payload)) {}

const ctPulse cont = 1

func build(g *game) {
	g.RegisterCont(ctPulse, func(*game, payload) {})
	g.AfterCont(3, ctPulse, payload{})
	g.LoopCont(2, ctPulse, payload{})
	g.CountCont(1, 5, ctPulse, payload{})
}
