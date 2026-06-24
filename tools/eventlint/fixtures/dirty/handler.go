package dirty

// Registration inside a handler closure — mid-match hazard, must flag.
func Setup(g G) {
	g.OnEvent(1, func() {
		g.RegisterEvent("late") // BAD: closure-nested registration
	})
}

type G interface {
	RegisterEvent(string) uint16
	OnEvent(uint16, func())
}
