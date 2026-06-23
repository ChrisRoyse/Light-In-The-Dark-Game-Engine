// Package match holds the match-flow controller (#201): the state machine that
// turns a sim world into a playable match — setup → countdown → play → terminal
// (victory/defeat) → exit — tracking per-match stats and driving the terminal
// screen. It consumes only the public litd/api surface (plus locale keys for
// chrome), so it sits cleanly outside litd/sim (D-5: no flow state in the sim
// tick, lockstep-safe) and adds no apilint/jassgen surface of its own.
//
// The controller observes, it never mutates the sim: it subscribes to the
// trained/death events to count stats, polls the local player's match result,
// and on a terminal latch shows the end-match UIScreen (locale-key chrome) while
// publishing the dynamic Stats for the render layer to format. Reset() tears the
// subscriptions down and zeroes state so a second match in the same process
// starts clean (no leaked stats, no orphan subscriptions).
package match

import (
	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
)

// Phase is the match-flow state. The zero value is PhaseSetup, so a fresh Flow
// begins at setup.
type Phase uint8

const (
	// PhaseSetup is faction/opponent selection before the match starts.
	PhaseSetup Phase = iota
	// PhaseCountdown is the pre-play countdown (units spawned, clock not yet
	// running); the driver sits here for its countdown duration.
	PhaseCountdown
	// PhasePlay is the live match; stats accrue and the result is polled.
	PhasePlay
	// PhaseTerminal is the end screen after a result latches.
	PhaseTerminal
	// PhaseExit is post-terminal — the match is torn down, returning to menu.
	PhaseExit
)

// String renders the phase name for logs/dumps.
func (p Phase) String() string {
	switch p {
	case PhaseSetup:
		return "setup"
	case PhaseCountdown:
		return "countdown"
	case PhasePlay:
		return "play"
	case PhaseTerminal:
		return "terminal"
	case PhaseExit:
		return "exit"
	default:
		return "unknown"
	}
}

// Faction is a playable side. Vigil is the zero value.
type Faction uint8

const (
	// FactionVigil is the Vigil faction (the default local pick).
	FactionVigil Faction = iota
	// FactionUnbound is the Unbound faction.
	FactionUnbound
)

// Setup is the match configuration chosen during PhaseSetup.
type Setup struct {
	Faction  Faction // the local player's faction
	Opponent Faction // the AI opponent's faction
}

// Stats are the per-match numbers shown on the terminal screen. Duration is in
// sim ticks (PhasePlay start to result latch).
type Stats struct {
	DurationTicks int
	UnitsTrained  int
	UnitsLost     int
}

// TerminalScreenID is the stable UIScreen id of the end-match screen (Show
// replaces / Hide targets it).
const TerminalScreenID = "terminal"

// Flow is the match-flow state machine. Create with NewFlow, then drive it:
// Begin → StartPlay → Poll (each tick) → ExitToMenu, with Reset for a clean
// second match. Not safe for concurrent use; it runs on the single game loop.
type Flow struct {
	g     *api.Game
	local api.Player

	phase     Phase
	setup     Setup
	result    api.MatchResult
	stats     Stats
	startTick uint32

	trainedSub api.Subscription
	deathSub   api.Subscription
	subscribed bool
}

// NewFlow builds a flow for game g whose local player is local (the player whose
// result and stats the terminal screen reports). It starts at PhaseSetup.
func NewFlow(g *api.Game, local api.Player) *Flow {
	return &Flow{g: g, local: local, phase: PhaseSetup, result: api.ResultPlaying}
}

// Phase returns the current flow phase.
func (f *Flow) Phase() Phase { return f.phase }

// Result returns the latched match result (ResultPlaying until terminal).
func (f *Flow) Result() api.MatchResult { return f.result }

// Stats returns the current per-match stats. During PhasePlay the counters are
// live; DurationTicks is final only once PhaseTerminal is reached.
func (f *Flow) Stats() Stats { return f.stats }

// Setup returns the chosen match configuration.
func (f *Flow) Setup() Setup { return f.setup }

// Begin records the setup and advances PhaseSetup → PhaseCountdown. It is a
// no-op (returns false) outside PhaseSetup — fail-closed against double-start.
func (f *Flow) Begin(s Setup) bool {
	if f.phase != PhaseSetup {
		return false
	}
	f.setup = s
	f.phase = PhaseCountdown
	return true
}

// StartPlay advances PhaseCountdown → PhasePlay: it subscribes the stat counters
// and stamps the play-start tick. No-op (false) outside PhaseCountdown.
func (f *Flow) StartPlay() bool {
	if f.phase != PhaseCountdown {
		return false
	}
	f.subscribeStats()
	f.startTick = f.g.Tick()
	f.phase = PhasePlay
	return true
}

// Poll advances the live match. During PhasePlay it reads the local player's
// result; once a terminal result latches it freezes the duration, transitions to
// PhaseTerminal, and shows the end-match screen. It is a no-op in any other
// phase, so the driver can call it unconditionally every tick. Returns true on
// the tick the terminal transition happens.
func (f *Flow) Poll() bool {
	if f.phase != PhasePlay {
		return false
	}
	r := f.local.Result()
	if r == api.ResultPlaying {
		return false
	}
	f.result = r
	f.stats.DurationTicks = int(f.g.Tick() - f.startTick)
	f.phase = PhaseTerminal
	f.showTerminal()
	return true
}

// ExitToMenu advances PhaseTerminal → PhaseExit and tears the match down (cancel
// subscriptions, hide the terminal screen). No-op (false) outside PhaseTerminal.
func (f *Flow) ExitToMenu() bool {
	if f.phase != PhaseTerminal {
		return false
	}
	f.cancelStats()
	f.g.UI().Hide(TerminalScreenID)
	f.phase = PhaseExit
	return true
}

// Reset returns the flow to PhaseSetup for a clean second match in the same
// process: it cancels any live subscriptions and zeroes all per-match state, so
// no stats, result, or start tick leak from the previous match. Idempotent.
func (f *Flow) Reset() {
	f.cancelStats()
	f.phase = PhaseSetup
	f.setup = Setup{}
	f.result = api.ResultPlaying
	f.stats = Stats{}
	f.startTick = 0
}

// subscribeStats wires the trained/death counters, filtered to the local player.
// Idempotent: a second call without an intervening cancel is ignored so the
// counters can't double-count.
func (f *Flow) subscribeStats() {
	if f.subscribed {
		return
	}
	slot := f.local.Slot()
	f.trainedSub = f.g.OnEvent(api.EventUnitTrained, func(e api.Event) {
		if e.Unit().Owner().Slot() == slot {
			f.stats.UnitsTrained++
		}
	})
	f.deathSub = f.g.OnEvent(api.EventUnitDeath, func(e api.Event) {
		if e.Unit().Owner().Slot() == slot {
			f.stats.UnitsLost++
		}
	})
	f.subscribed = true
}

// cancelStats tears down the stat subscriptions. Safe to call when not
// subscribed (no orphan subscriptions survive a Reset/exit).
func (f *Flow) cancelStats() {
	if !f.subscribed {
		return
	}
	f.trainedSub.Cancel()
	f.deathSub.Cancel()
	f.subscribed = false
}

// showTerminal pushes the end-match UIScreen: locale-key chrome only (the
// victory/defeat headline and the exit button). The dynamic stat rows are NOT in
// the UIScreen (D-17: a UIScreen carries keys, not values) — the render layer
// reads Stats() and formats them via hud.NewTerminalScreenLayout.
func (f *Flow) showTerminal() {
	title := locale.TerminalDefeat
	if f.result == api.ResultWon {
		title = locale.TerminalVictory
	}
	f.g.UI().Show(api.UIScreen{
		ID:       TerminalScreenID,
		TitleKey: string(title),
		Buttons: []api.UIButton{
			{ID: "exit", LabelKey: string(locale.TerminalExit), Command: "flow.exit"},
		},
	})
}
