package luabind_test

// #261 bring-up FSV for the vendored gopher-lua fork. SoT = the values the
// embedded VM actually computes (read back from eval output) and the
// iteration order it produces — not the fact that it compiled.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

// TestLuaSmoke — the vendored fork runs a chunk and returns the right value.
func TestLuaSmoke(t *testing.T) {
	got, err := luabind.Eval("return 1+1")
	if err != nil {
		t.Fatalf("eval: %v", err)
	}
	t.Logf("FSV smoke: Eval(\"return 1+1\") = %q (want \"2\")", got)
	if got != "2" {
		t.Fatalf("1+1 = %q, want \"2\"", got)
	}
}

// TestLuaPairsDeterministicOrder — pairs() over a 100-key table yields the
// SAME iteration order on two independent runs. This is the determinism
// property the fork was selected for (D-25: pairs never ranges a Go map). The
// SoT is the concatenated key=value walk; the two runs must be byte-identical.
func TestLuaPairsDeterministicOrder(t *testing.T) {
	const src = `
		local t = {}
		for i = 1, 100 do t["k" .. i] = i end
		local s = ""
		for k, v in pairs(t) do s = s .. k .. "=" .. v .. ";" end
		return s
	`
	r1, err := luabind.Eval(src)
	if err != nil {
		t.Fatalf("run1: %v", err)
	}
	r2, err := luabind.Eval(src)
	if err != nil {
		t.Fatalf("run2: %v", err)
	}
	t.Logf("FSV pairs(): len=%d run1==run2=%v; head=%.40q", len(r1), r1 == r2, r1)
	if r1 != r2 {
		t.Fatalf("pairs() iteration order nondeterministic:\n run1=%q\n run2=%q", r1, r2)
	}
	// Sanity: all 100 entries are present (no truncation, no map drop).
	if got := countSemicolons(r1); got != 100 {
		t.Fatalf("pairs() walked %d entries, want 100", got)
	}
}

// TestLuaTostringPureGo — tostring(0.1) is the pure-Go strconv rendering, not a
// libc/printf result, so it is identical across platforms (D-25 rationale).
func TestLuaTostringPureGo(t *testing.T) {
	got, err := luabind.Eval("return tostring(0.1)")
	if err != nil {
		t.Fatalf("eval: %v", err)
	}
	t.Logf("FSV tostring(0.1) = %q (want \"0.1\")", got)
	if got != "0.1" {
		t.Fatalf("tostring(0.1) = %q, want \"0.1\" (pure-Go strconv)", got)
	}
}

func countSemicolons(s string) int {
	n := 0
	for _, c := range s {
		if c == ';' {
			n++
		}
	}
	return n
}
