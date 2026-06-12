package fixed

//go:generate go run ./gen

// Angle is a binary angular measurement: 1/65536 of a full turn.
// Wrap-around is free (uint16 overflow), comparison is exact, and π
// never appears in the API (determinism.md §2.4).
type Angle uint16

const (
	quarterTurn Angle = 0x4000
	halfTurn    Angle = 0x8000
)

// Sin returns sin(a) as 32.32 fixed point, via the committed
// quarter-wave table (sinQuarter, generated source — no runtime math.*).
func (a Angle) Sin() F64 {
	quadrant := a >> 14   // 0..3
	idx := int(a & 0x3FFF) // position within quarter wave
	switch quadrant {
	case 0:
		return sinQuarter[idx]
	case 1:
		return sinQuarter[quarterSteps-idx]
	case 2:
		return -sinQuarter[idx]
	default:
		return -sinQuarter[quarterSteps-idx]
	}
}

// Cos returns cos(a) as 32.32 fixed point: cos(a) = sin(a + quarter turn).
func (a Angle) Cos() F64 { return (a + quarterTurn).Sin() }
