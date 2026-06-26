// Package clean is a godoclint fixture: every exported symbol carries a doc
// comment and none leak internal references. audit() must report zero findings.
package clean

// Widget is a documented exported type.
type Widget struct {
	// Size is a documented exported field.
	Size int
}

// Frob is a documented exported function.
func Frob() {}

// Answer is a documented exported constant.
const Answer = 42
