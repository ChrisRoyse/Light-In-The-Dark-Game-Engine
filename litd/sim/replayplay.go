package sim

// Replay playback engine (#83). The replay viewer's defining property (input.md
// §8, D-2026-06-11-16) is "playback = re-simulation": the viewer never stores
// game state beyond the live sim — it re-applies the recorded command stream
// onto a fresh identical world, tick by tick, and the resulting state IS the
// playback. ReplayPlayer is that headless engine; the client shell wraps it with
// the free camera, per-player fog perspective, and input (pure client state,
// input.md §6 — not part of this deterministic core).
//
// Speed control is TICK BATCHING, never interpolation (the spec is explicit):
// at N×, more recorded ticks are simulated per viewer frame; at 0.5×, one tick
// every two frames. Pause simply stops feeding ticks. Because playback is pure
// re-simulation, the final state hash is identical regardless of the speed or
// pause schedule the viewer used to get there — that invariant is the SoT the
// tests assert.

// PlaybackSpeed is a viewer playback rate. Values are tick-batching ratios over
// a fixed denominator of 2, so 0.5× advances one tick every two frames and the
// integer speeds advance that many ticks per frame.
type PlaybackSpeed uint8

const (
	SpeedHalf PlaybackSpeed = iota // 0.5×
	Speed1x
	Speed2x
	Speed4x
	Speed8x
)

const playbackDenom = 2

// speedNumer[s] / playbackDenom = the ticks-per-frame rate for speed s.
var speedNumer = [...]int{
	SpeedHalf: 1,
	Speed1x:   2,
	Speed2x:   4,
	Speed4x:   8,
	Speed8x:   16,
}

// Valid reports whether s is a known speed.
func (s PlaybackSpeed) Valid() bool { return int(s) < len(speedNumer) }

// ReplayPlayer re-simulates a decoded replay onto a world. The world must be a
// FRESH world set up identically to the recorded match (same seed, same initial
// roster) — the player only feeds it the recorded inputs; it does not restore
// state. resolve maps a command's roster/entity index to a live EntityID (see
// melee.EntityResolver for the AI-match convention, or a spawn-roster array for
// a player-input replay).
type ReplayPlayer struct {
	rep     *Replay
	w       *World
	resolve func(idx uint32) (EntityID, bool)

	tick   uint32 // ticks simulated so far (0..rep.Ticks)
	cmd    int    // index of the next command to apply
	paused bool
	speed  PlaybackSpeed
	acc    int // sub-frame tick accumulator for fractional speeds
}

// NewReplayPlayer binds a player to rep and w. Playback starts at 1× and
// unpaused, at tick 0.
func NewReplayPlayer(rep *Replay, w *World, resolve func(idx uint32) (EntityID, bool)) *ReplayPlayer {
	return &ReplayPlayer{rep: rep, w: w, resolve: resolve, speed: Speed1x}
}

// Pause stops feeding ticks; Frame becomes a no-op until Resume.
func (p *ReplayPlayer) Pause() { p.paused = true }

// Resume restarts playback.
func (p *ReplayPlayer) Resume() { p.paused = false }

// Paused reports the current pause state.
func (p *ReplayPlayer) Paused() bool { return p.paused }

// SetSpeed changes the playback rate. An invalid speed is ignored (fail-closed:
// the viewer keeps its current rate rather than running at an undefined one).
func (p *ReplayPlayer) SetSpeed(s PlaybackSpeed) {
	if s.Valid() {
		p.speed = s
	}
}

// Speed is the current playback rate.
func (p *ReplayPlayer) Speed() PlaybackSpeed { return p.speed }

// Tick is the number of ticks simulated so far.
func (p *ReplayPlayer) Tick() uint32 { return p.tick }

// Done reports whether playback has reached the recorded tick count.
func (p *ReplayPlayer) Done() bool { return p.tick >= p.rep.Ticks }

// Frame advances playback by one viewer frame and returns the number of recorded
// ticks simulated this frame (0 when paused or finished). Each simulated tick
// applies the commands recorded at that tick (in record order) and then steps
// the world once — the same apply-then-step ordering the headless record/verify
// driver uses, so playback reproduces the recording exactly.
func (p *ReplayPlayer) Frame() int {
	if p.paused || p.Done() {
		return 0
	}
	p.acc += speedNumer[p.speed]
	n := p.acc / playbackDenom
	p.acc %= playbackDenom
	advanced := 0
	for i := 0; i < n && !p.Done(); i++ {
		p.advanceOneTick()
		advanced++
	}
	return advanced
}

// RunToEnd plays the remaining ticks at the current speed (respecting pause),
// returning the total ticks simulated. A convenience for headless verification.
// A frame that advances 0 ticks is normal at fractional speed (0.5× advances one
// tick every two frames), so the loop stops on Done or pause, never on a 0-tick
// frame — numer >= 1 guarantees a tick lands within playbackDenom frames.
func (p *ReplayPlayer) RunToEnd() int {
	total := 0
	for !p.Done() && !p.paused {
		total += p.Frame()
	}
	return total
}

func (p *ReplayPlayer) advanceOneTick() {
	next := p.tick + 1
	for p.cmd < len(p.rep.Commands) && p.rep.Commands[p.cmd].Tick == next {
		p.rep.Commands[p.cmd].Apply(p.w, p.resolve)
		p.cmd++
	}
	p.w.Step()
	p.tick = next
}
