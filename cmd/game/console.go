package main

// In-game backquote Lua REPL debug console (#399, R-OBS-5, split from #252). A
// rendered overlay that evaluates one line at a time against the LIVE sandboxed
// script VM via luabind.EvalLine (so it sees exactly the world's Lua surface — no
// extra powers), and installs the obs.* module (obs.dump / obs.counters /
// obs.loglevel) backed by the game's real observability instances. Toggle with the
// backquote key; type, Enter submits, Backspace edits, Esc closes. While open the
// console swallows all keyboard input so game hotkeys (F5/Tab/Space/Q…) do not fire
// under the typist. Evaluation is presentation-only: it reads/prints state and can
// nudge log verbosity, but EvalLine runs on the script VM and cannot reach sim core
// state outside the sandbox — the console never perturbs the determinism surface.

import (
	"fmt"
	"strings"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/obs"
	"github.com/g3n/engine/gui"
	"github.com/g3n/engine/math32"
)

// consoleMaxLines is how many scrollback lines the overlay shows (newest at the
// bottom, above the input row).
const consoleMaxLines = 14

// buildConsole creates the overlay label and wires the obs.* module onto the live
// host VM. Real counters + logger back it so obs.counters()/dump()/loglevel() report
// the running game, not a stub.
func (gm *game) buildConsole() {
	gm.obsLog = obs.New(256)
	gm.obsCounts = obs.NewDefaultCounters()
	gm.cUnits = gm.obsCounts.Register("units", "count/op", obs.CounterGauge)
	gm.cTicks = gm.obsCounts.Register("ticks", "count/op", obs.CounterGauge)
	gm.registerConsoleObs()

	gm.console = gui.NewLabel("")
	gm.console.SetPosition(12, 120)
	gm.console.SetColor(&math32.Color{R: 0.62, G: 1.0, B: 0.62})
	gm.console.SetVisible(false)
	gm.scene.Add(gm.console)
}

// registerConsoleObs installs the obs.* module on the current host VM. Called at
// build time and again after a quickload swaps the host (a fresh *lua.LState).
func (gm *game) registerConsoleObs() {
	if gm.host == nil {
		return
	}
	luabind.RegisterObs(gm.host.L, &luabind.LiveObs{Log: gm.obsLog, Counts: gm.obsCounts})
}

// refreshConsoleCounters mirrors a little live game state into the obs counters so
// obs.counters() in the console reports something real (unit count, tick).
func (gm *game) refreshConsoleCounters() {
	if gm.obsCounts == nil {
		return
	}
	gm.obsCounts.Set(gm.cUnits, int64(len(gm.g.AllUnits(nil))))
	gm.obsCounts.Set(gm.cTicks, int64(gm.curTick))
}

// toggleConsole opens/closes the overlay.
func (gm *game) toggleConsole() {
	gm.consoleActive = !gm.consoleActive
	gm.console.SetVisible(gm.consoleActive)
	gm.renderConsole()
}

// consoleType appends one printable ASCII rune (ignores the backquote toggle key and
// any control/non-ASCII rune).
func (gm *game) consoleType(r rune) {
	if r == '`' || r < 0x20 || r > 0x7e {
		return
	}
	gm.consoleInput += string(r)
	gm.renderConsole()
}

// consoleBackspace deletes the last input rune.
func (gm *game) consoleBackspace() {
	if n := len(gm.consoleInput); n > 0 {
		gm.consoleInput = gm.consoleInput[:n-1]
		gm.renderConsole()
	}
}

// consoleSubmit evaluates the current input line against the script VM and appends
// the echoed line + its output (or a loud, logged error) to the scrollback. A Lua
// error is surfaced in the overlay AND on stdout; it never crashes the game and the
// VM stays usable (EvalLine guarantees this).
func (gm *game) consoleSubmit() {
	line := strings.TrimSpace(gm.consoleInput)
	gm.consoleInput = ""
	if line == "" {
		gm.renderConsole()
		return
	}
	gm.refreshConsoleCounters()
	gm.consoleHist = append(gm.consoleHist, "> "+line)
	res := luabind.EvalLine(gm.host.L, line)
	if res.Err != "" {
		gm.consoleHist = append(gm.consoleHist, "!! "+res.Err)
		fmt.Printf("event: console error: %s\n", res.Err)
	} else if out := strings.TrimRight(res.Output, "\n"); out != "" {
		gm.consoleHist = append(gm.consoleHist, strings.Split(out, "\n")...)
	}
	if len(gm.consoleHist) > 200 {
		gm.consoleHist = gm.consoleHist[len(gm.consoleHist)-200:]
	}
	gm.renderConsole()
}

// renderConsole rebuilds the overlay text: a header, the tail of the scrollback, and
// the live input row with a cursor.
func (gm *game) renderConsole() {
	if gm.console == nil {
		return
	}
	lines := gm.consoleHist
	if len(lines) > consoleMaxLines {
		lines = lines[len(lines)-consoleMaxLines:]
	}
	var b strings.Builder
	b.WriteString("— Lua console (R-OBS-5) · obs.dump() obs.counters() obs.loglevel(n) · [Esc] close —\n")
	for _, ln := range lines {
		b.WriteString(ln)
		b.WriteByte('\n')
	}
	b.WriteString("> " + gm.consoleInput + "_")
	gm.console.SetText(b.String())
}

// runConsoleEvalFSV is the scripted FSV for -console-eval: open the console, type the
// given line, and submit it — driving the exact same consoleType/consoleSubmit/
// renderConsole path real keystrokes use. The caller renders the overlay over the
// scene, screenshots, and exits, so the PNG is faithful evidence of the live console.
func (gm *game) runConsoleEvalFSV() {
	gm.toggleConsole() // open
	for _, r := range gm.consoleEval {
		gm.consoleType(r)
	}
	gm.consoleSubmit()
	fmt.Printf("event: console-eval %q -> %d scrollback lines\n", gm.consoleEval, len(gm.consoleHist))
}
