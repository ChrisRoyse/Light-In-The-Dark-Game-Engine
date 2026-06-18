// Package dirty is a godoclint fixture exercising both gates: it has an
// exported field with no doc comment (a G-1 violation) and a doc comment that
// leaks an internal package reference (a G-5 violation).
package dirty

// Gadget is documented, but its field below is not.
type Gadget struct {
	Size int
}

// Leaky is documented, but this doc references litd/sim internals, which the
// public surface must never mention.
func Leaky() {}
