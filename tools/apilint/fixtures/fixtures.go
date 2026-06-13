// Package fixtures is a deliberately non-conforming API surface used to prove
// apilint flags every rule (and, via the clean declarations, that it does not
// over-flag). It is never built into the engine — it is a lint fixture only.
//
// Each exported declaration is annotated with the rule it exercises: WANT lines
// must be flagged, OK lines must not.
package fixtures

import "github.com/g3n/engine/math32"

// Game is the in-fixture root type. apilint detects a noun handle structurally
// by its *Game field, so the fixtures must declare their own Game.
type Game struct{ tick int }

// Opt is an option type (func shape) for the variadic-exemption fixture.
type Opt func(*int)

// GizmoOption is an option type (name ends in "Option") so an option
// constructor that takes a handle is permitted.
type GizmoOption func(*Game)

// Gizmo is a CLEAN noun handle: *Game field, zero exported fields, has
// Valid() bool. Must produce no finding.
type Gizmo struct {
	id int
	g  *Game
}

func (x Gizmo) Valid() bool { return x.g != nil && x.id != 0 } // OK

// OkVerb is a clean method: 2 positional params + a variadic options slot.
func (x Gizmo) OkVerb(a, b int, opts ...Opt) {} // OK (variadic excluded -> 2)

// SixParamMethod has 6 positional params -> G2.3. (Also demonstrates edge 2:
// methods on noun handles are linted exactly like free verbs.)
func (x Gizmo) SixParamMethod(a, b, c, d, e, f int) {} // WANT G2.3

// FiveAndVariadic has 5 positional + a variadic slot: exactly at the ceiling.
func FiveAndVariadic(a, b, c, d, e int, opts ...Opt) {} // OK (variadic excluded -> 5)

// SixPositional has 6 positional params -> G2.3.
func SixPositional(a, b, c, d, e, f int) {} // WANT G2.3

// KillThing is a free package-level func taking a noun handle -> G2.4/R-API-1.
func KillThing(x Gizmo) {} // WANT G2.4

// ForThing takes a handle but returns an option type: allowed option ctor.
func ForThing(x Gizmo) GizmoOption { return func(*Game) {} } // OK

// Leaky has a *Game field (so it is a handle) but an exported field -> §2. It
// has Valid(), so only the exported-field rule fires (mirrors "exported field
// injected on Unit").
type Leaky struct {
	Exposed int // WANT §2 exported field
	g       *Game
}

func (l Leaky) Valid() bool { return l.g != nil } // OK

// NoValid is a handle ({id; *Game}, zero exported fields) with no Valid()
// method -> R-API-5.
type NoValid struct { // WANT R-API-5 missing Valid()
	id int
	g  *Game
}

// BadVerb is a method returning error on a gameplay verb -> R-API-5.
func (x Gizmo) BadVerb() error { return nil } // WANT R-API-5 error return

// Frob is a free func returning error, not in the setup allowlist -> R-API-5.
func Frob() error { return nil } // WANT R-API-5 error return

// NewGame is an allowlisted setup constructor: error return permitted.
func NewGame() (*Game, error) { return &Game{}, nil } // OK (setup allowlist)

// BadSig exposes a G3N type in an exported signature -> R-API-6.
func BadSig() math32.Vector3 { return math32.Vector3{} } // WANT R-API-6 signature

// Trigger is a forbidden exported identifier (the trigger zoo) -> R-API-4.
type Trigger struct{} // WANT forbidden Trigger

// Location is a forbidden exported identifier (heap location) -> R-API-2/4.
type Location struct{} // WANT forbidden Location

// Surface exposes a G3N type in an exported field -> R-API-6.
type Surface struct {
	Node math32.Vector3 // WANT R-API-6 exported field
}
