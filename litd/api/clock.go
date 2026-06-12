package litd

// TimeDawn and TimeDusk are the canonical day/night boundaries in
// public game-clock hours.
const (
	TimeDawn = 6.0
	TimeDusk = 18.0
)

// TimeOfDay returns the current game clock hour in [0, 24), or 0 for
// a nil Game. JASS: GetTimeOfDay / GetFloatGameState time-of-day.
func (g *Game) TimeOfDay() float64 {
	if g == nil || g.w == nil {
		return 0
	}
	return toFloat(g.w.TimeOfDay())
}

// SetTimeOfDay sets the game clock hour, following the sim wrap rule
// for values outside [0, 24). No-op for a nil Game. JASS: SetTimeOfDay.
func (g *Game) SetTimeOfDay(h float64) {
	if g == nil || g.w == nil {
		return
	}
	g.w.SetTimeOfDay(fromFloat(h))
}

// TimeOfDayScale returns the day/night progression scale, or 0 for a
// nil Game. The scale changes game-clock progression only; it never
// changes the fixed 20 Hz sim tick rate.
func (g *Game) TimeOfDayScale() float64 {
	if g == nil || g.w == nil {
		return 0
	}
	return toFloat(g.w.TimeOfDayScale())
}

// SetTimeOfDayScale sets the day/night progression scale. Negative
// scales are rejected by the sim and leave the current scale unchanged.
// No-op for a nil Game. JASS: SetTimeOfDayScale.
func (g *Game) SetTimeOfDayScale(s float64) {
	if g == nil || g.w == nil {
		return
	}
	g.w.SetTimeOfDayScale(fromFloat(s))
}

// SuspendTimeOfDay freezes or resumes day/night progression without
// changing the sim tick rate. No-op for a nil Game. JASS:
// SuspendTimeOfDay.
func (g *Game) SuspendTimeOfDay(suspended bool) {
	if g == nil || g.w == nil {
		return
	}
	g.w.SuspendTimeOfDay(suspended)
}

// TimeOfDaySuspended reports whether day/night progression is frozen.
// Nil-safe and false for a nil Game. JASS: IsTimeOfDaySuspended.
func (g *Game) TimeOfDaySuspended() bool {
	if g == nil || g.w == nil {
		return false
	}
	return g.w.TimeOfDaySuspended()
}

// ElapsedTime returns elapsed game time in seconds. Each sim tick is
// exactly 0.05 seconds; game-clock scale does not affect this value.
// Nil-safe and zero for a nil Game. JASS: GetElapsedGameTime.
func (g *Game) ElapsedTime() float64 {
	if g == nil || g.w == nil {
		return 0
	}
	return float64(g.w.ElapsedTicks()) / 20
}
