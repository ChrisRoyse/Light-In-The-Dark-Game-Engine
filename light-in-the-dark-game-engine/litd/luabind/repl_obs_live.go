package luabind

import (
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/obs"
)

// LiveObs adapts the running observability subsystem (*obs.Logger, *obs.Counters)
// to the ReplObs read surface that the obs.* console module (RegisterObs) consumes.
// This is the production backing the console host installs; repl_test.go exercises
// the same interface through a fake. Drawing the backquote overlay is blocked on
// the render/UI layer (#399), but this bridge is headless and is verified against
// real obs instances in repl_obs_live_test.go — so the eventual wiring is a
// known-good connection, not a first-contact integration.
//
// loglevel semantics: obs.Logger keeps an independent threshold Level per channel;
// the console exposes a single global knob. SetLogLevel(n) therefore sets EVERY
// channel to Level(n) — a global verbosity override — and rejects an out-of-range n
// (fail-closed: no mutation, returns false). LogLevel() reports that level via a
// reference channel (after any SetLogLevel all channels are uniform). This touches
// logging verbosity only — never sim state (the #449/#471 presentation rule).
type LiveObs struct {
	Log    *obs.Logger
	Counts *obs.Counters
}

// LiveObs implements ReplObs.
var _ ReplObs = (*LiveObs)(nil)

// replMaxLevel is the most-verbose valid Level (Error=0 .. Trace=4) SetLogLevel
// accepts. Anything outside [0, replMaxLevel] is rejected.
const replMaxLevel = int(obs.Trace)

// replRefChannel is the channel LogLevel reports. Channel 0 is representative
// because SetLogLevel always writes every channel uniformly.
const replRefChannel = obs.Channel(0)

// DumpLog returns the formatted log ring (obs.dump()). Empty when no logger is
// attached (a headless game with observability off).
func (o *LiveObs) DumpLog() string {
	if o.Log == nil {
		return ""
	}
	var b strings.Builder
	// Dump only errors on the io.Writer; strings.Builder never fails to write.
	_ = o.Log.Dump(&b)
	return b.String()
}

// Counters returns one row per registered counter with its current value
// (obs.counters()). Nil when no counters subsystem is attached.
func (o *LiveObs) Counters() []CounterLine {
	if o.Counts == nil {
		return nil
	}
	n := o.Counts.CounterCount()
	rows := make([]CounterLine, 0, n)
	for i := 0; i < n; i++ {
		id := obs.CounterID(i)
		def := o.Counts.Def(id)
		rows = append(rows, CounterLine{Name: def.Name, Unit: def.Unit, Value: o.Counts.Value(id)})
	}
	return rows
}

// LogLevel reports the current global verbosity (obs.loglevel()). 0 when no logger
// is attached.
func (o *LiveObs) LogLevel() int {
	if o.Log == nil {
		return 0
	}
	return int(o.Log.ChannelLevel(replRefChannel))
}

// SetLogLevel sets every channel to level (obs.loglevel(n)). Fail-closed: an absent
// logger or an out-of-range level changes nothing and returns false.
func (o *LiveObs) SetLogLevel(level int) bool {
	if o.Log == nil || level < 0 || level > replMaxLevel {
		return false
	}
	for ch := 0; ch < int(obs.NumChannels); ch++ {
		o.Log.SetChannelLevel(obs.Channel(ch), obs.Level(level))
	}
	return true
}
