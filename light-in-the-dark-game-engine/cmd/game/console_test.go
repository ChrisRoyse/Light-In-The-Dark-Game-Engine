package main

import "testing"

// The console's input state machine (#399) is the new logic here; the eval itself
// (luabind.EvalLine) is covered by litd/luabind/repl_test.go. renderConsole no-ops
// when gm.console is nil (no GL label in a unit test), so the buffer logic is
// headless-testable. SoT = the consoleInput buffer after a sequence of edits.
func TestConsoleInputEditingFSV(t *testing.T) {
	gm := &game{}

	for _, r := range "obs.counters()" {
		gm.consoleType(r)
	}
	if gm.consoleInput != "obs.counters()" {
		t.Fatalf("after typing: consoleInput=%q, want %q", gm.consoleInput, "obs.counters()")
	}

	// The backquote (toggle key) and control runes must never enter the buffer.
	gm.consoleType('`')
	gm.consoleType('\n')
	gm.consoleType(rune(0x1b))
	if gm.consoleInput != "obs.counters()" {
		t.Fatalf("toggle/control runes leaked into buffer: %q", gm.consoleInput)
	}

	gm.consoleBackspace()
	gm.consoleBackspace()
	if gm.consoleInput != "obs.counters" {
		t.Fatalf("after 2 backspaces: consoleInput=%q, want %q", gm.consoleInput, "obs.counters")
	}

	// Backspacing past empty is a no-op, never a panic/underflow.
	gm.consoleInput = ""
	gm.consoleBackspace()
	if gm.consoleInput != "" {
		t.Fatalf("backspace on empty buffer changed it to %q", gm.consoleInput)
	}
	t.Logf("FSV #399 console input state machine: type/backspace/guard-runes ok")
}
