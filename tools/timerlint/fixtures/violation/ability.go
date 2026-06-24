//go:build ignore

// Fixture: an ability template that WRONGLY uses the Go-closure timer sugar.
// Both calls capture a closure that cannot be serialized, so the behaviour is
// dropped on load. timerlint must flag both.
package violation

import "time"

type game struct{}

func (g *game) After(d time.Duration, f func())  {}
func (g *game) Every(d time.Duration, f func(int)) {}

func build(g *game) {
	g.After(time.Second, func() {})   // flagged
	g.Every(time.Second, func(int) {}) // flagged
}
