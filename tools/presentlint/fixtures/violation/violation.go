//go:build ignore

// Fixture: a presentation consumer that WRONGLY wires into the sim-hashing
// subscription path. Each of these calls makes an audio-on game hash
// differently from an audio-off game (#449). presentlint must flag all of them.
package violation

type game struct{}

func (g *game) OnEvent(int, func())   {}
func (g *game) OnAbilityCast(func())  {}
func (g *game) OnAttack(func())       {}
func (g *game) OnBuffApplied(func())  {}
func (g *game) OnDamage(func())       {}
func (g *game) Subscribe(int, int)    {}
func (g *game) NewTrigger()           {}

func wire(g *game) {
	g.OnEvent(1, func() {})    // violation: hashing subscription
	g.OnAbilityCast(func() {}) // violation: sugar over OnEvent
	g.OnAttack(func() {})      // violation: sugar over OnEvent
	g.OnBuffApplied(func() {}) // violation: sugar over OnEvent
	g.OnDamage(func() {})      // violation: sim damage modifier
	g.Subscribe(1, 2)          // violation: sim subscribe
	g.NewTrigger()             // violation: hashed trigger slab
}
