package clean

// Top-level setup registration — allowed.
func Setup(g G) {
	waveKind = g.RegisterEvent("wave")
	bossKind = g.RegisterEvent("boss")
}

var waveKind, bossKind uint16

type G interface {
	RegisterEvent(string) uint16
	OnEvent(uint16, func())
}
