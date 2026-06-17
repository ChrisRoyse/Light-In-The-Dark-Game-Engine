package litd

// Advance drives the deterministic sim forward by ticks whole 50 ms ticks,
// running every tick's seven phases in order — including phase 2, where the
// cooperative scheduler resumes any threads/timers whose wake tick has arrived
// (thread.go, timer.go). It is the host-loop primitive: the world loader (#268)
// and the Lua execution track (#269) call it to make game time pass, and tests
// use it to fast-forward to a scheduled wake. ticks <= 0 and a nil/uninitialized
// game are no-ops.
//
// Advance is the authoritative clock for everything riding the scheduler, so a
// PolledWait registered by a thread resumes exactly when Advance reaches its
// wake tick — deterministically, independent of wall-clock time.
func (g *Game) Advance(ticks int) {
	if g == nil || g.w == nil {
		return
	}
	for i := 0; i < ticks; i++ {
		g.w.Step()
	}
}
