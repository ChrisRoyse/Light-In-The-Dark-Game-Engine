package sim

import (
	"math/bits"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

const (
	// DefaultDayLengthTicks is the WC3 baseline: one game day every
	// 480 real seconds at the fixed 20 Hz sim tick.
	DefaultDayLengthTicks uint32 = 9600

	// TimeDawn and TimeDusk are canonical day/night boundaries in
	// fixed-point game hours.
	TimeDawn fixed.F64 = 6 * fixed.One
	TimeDusk fixed.F64 = 18 * fixed.One

	clockDay    fixed.F64 = 24 * fixed.One
	clockDayRaw uint64    = uint64(clockDay)
)

// TimeOfDay returns the deterministic game clock hour in [0, 24).
func (w *World) TimeOfDay() fixed.F64 { return w.tod }

// IsNight reports whether the deterministic game clock is outside the
// canonical [dawn,dusk) daylight band.
func (w *World) IsNight() bool { return w.tod < TimeDawn || w.tod >= TimeDusk }

// SetTimeOfDay wraps h into the deterministic game clock range [0, 24).
func (w *World) SetTimeOfDay(h fixed.F64) {
	w.tod = wrapTimeOfDay(h)
	w.todCarry = 0
}

// TimeOfDayScale returns the fixed-point day/night progression scale.
func (w *World) TimeOfDayScale() fixed.F64 { return w.todScale }

// SetTimeOfDayScale changes day/night progression without changing
// the fixed 20 Hz simulation tick. Negative scales are rejected.
func (w *World) SetTimeOfDayScale(s fixed.F64) bool {
	if s < 0 {
		return false
	}
	w.todScale = s
	return true
}

// SuspendTimeOfDay freezes or resumes day/night progression.
func (w *World) SuspendTimeOfDay(suspended bool) { w.todFrozen = suspended }

// TimeOfDaySuspended reports whether day/night progression is frozen.
func (w *World) TimeOfDaySuspended() bool { return w.todFrozen }

// ElapsedTicks returns the integer game time in 50 ms ticks.
func (w *World) ElapsedTicks() uint32 { return w.Tick() }

func (w *World) advanceTimeOfDay() {
	if w.todFrozen || w.todScale == 0 {
		return
	}
	inc, carry := clockAdvance(w.todScale, w.dayLengthTicks, w.todCarry)
	w.todCarry = carry
	raw := uint64(w.tod) + inc
	if raw >= clockDayRaw {
		raw -= clockDayRaw
	}
	w.tod = fixed.F64(raw)
}

func clockAdvance(scale fixed.F64, dayLength uint32, carry uint64) (uint64, uint64) {
	if scale <= 0 || dayLength == 0 {
		return 0, carry
	}
	hi, lo := bits.Mul64(uint64(scale), 24)
	var carryOut uint64
	lo, carryOut = bits.Add64(lo, carry, 0)
	hi += carryOut
	div := uint64(dayLength)
	if hi >= div {
		return 0, carry
	}
	q, rem := bits.Div64(hi, lo, div)
	return q % clockDayRaw, rem
}

func wrapTimeOfDay(h fixed.F64) fixed.F64 {
	if h >= 0 && h < clockDay {
		return h
	}
	raw := int64(h) % int64(clockDay)
	if raw < 0 {
		raw += int64(clockDay)
	}
	return fixed.F64(raw)
}
