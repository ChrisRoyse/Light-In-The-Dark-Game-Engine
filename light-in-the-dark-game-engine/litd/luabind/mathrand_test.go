package luabind_test

// #263 (half 1) math.random → deterministic PRNG FSV. SoT = the draw sequences
// the VM produces, read back, and compared run-to-run and against the shared
// stream's raw output — never Go's global rand.

import (
	"fmt"
	"math"
	"strings"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

// newLCG is a deterministic [0,1) generator standing in for the sim PRNG: same
// seed → same sequence, no global state, no clock.
func newLCG(seed uint64) func() float64 {
	s := seed
	return func() float64 {
		s = s*6364136223846793005 + 1442695040888963407
		return float64(s>>11) / float64(uint64(1)<<53)
	}
}

// TestMathRandomDeterministicSameSeed — edge (2a): two interpreters with
// identically-seeded sources produce identical first-10 math.random draws.
func TestMathRandomDeterministicSameSeed(t *testing.T) {
	draws := func() string {
		i := luabind.New()
		defer i.Close()
		i.SetRandomSource(newLCG(0xC0FFEE))
		s, err := i.Eval(`local t={}; for k=1,10 do t[k]=string.format("%.6f", math.random()) end; return table.concat(t, ",")`)
		if err != nil {
			t.Fatalf("eval: %v", err)
		}
		return s
	}
	a, b := draws(), draws()
	t.Logf("FSV same-seed draws:\n  run1=%s\n  run2=%s", a, b)
	if a != b {
		t.Fatalf("math.random not deterministic for the same seed:\n run1=%s\n run2=%s", a, b)
	}
	if strings.Count(a, ",") != 9 {
		t.Fatalf("expected 10 draws, got %q", a)
	}
}

// TestMathRandomSharedStreamInterleave — edge (2b): Lua math.random and a
// Go-side draw pull from ONE stream in deterministic call order. Binding the
// same closure, the interleaved sequence Go,Lua,Go,Lua equals the generator's
// raw outputs [0],[1],[2],[3] in order.
func TestMathRandomSharedStreamInterleave(t *testing.T) {
	// Raw reference: the first 4 outputs of the stream.
	ref := newLCG(42)
	want := []float64{ref(), ref(), ref(), ref()}

	gen := newLCG(42)
	i := luabind.New()
	defer i.Close()
	i.SetRandomSource(gen)

	go0 := gen()                          // Go draw  → want[0]
	lua1, err := i.Eval("return math.random()") // Lua draw → want[1]
	if err != nil {
		t.Fatalf("eval1: %v", err)
	}
	go2 := gen()                          // Go draw  → want[2]
	lua3, err := i.Eval("return math.random()") // Lua draw → want[3]
	if err != nil {
		t.Fatalf("eval3: %v", err)
	}
	t.Logf("FSV interleave: go0=%.9f lua1=%s go2=%.9f lua3=%s | want=%v", go0, lua1, go2, lua3, want)
	if go0 != want[0] || go2 != want[2] {
		t.Fatalf("Go-side draws diverged from the shared stream: go0=%v go2=%v want %v,%v", go0, go2, want[0], want[2])
	}
	if !floatStrEq(lua1, want[1]) || !floatStrEq(lua3, want[3]) {
		t.Fatalf("Lua draws diverged from the shared stream: lua1=%s lua3=%s want %.9f,%.9f", lua1, lua3, want[1], want[3])
	}
}

// TestMathRandomRangeSemantics — random(n) ∈ [1,n], random(m,n) ∈ [m,n], all
// from the bound source; with no source bound math.random fails closed.
func TestMathRandomRangeSemantics(t *testing.T) {
	i := luabind.New()
	defer i.Close()
	i.SetRandomSource(newLCG(7))
	for k := 0; k < 50; k++ {
		v, err := i.Eval("return math.random(1, 6)")
		if err != nil {
			t.Fatalf("eval: %v", err)
		}
		if v < "1" || v > "6" || len(v) != 1 {
			t.Fatalf("math.random(1,6) = %q, want a digit 1..6", v)
		}
	}

	// No source bound → loud error (fail-closed, no nondeterministic fallback).
	none := luabind.New()
	defer none.Close()
	_, err := none.Eval("return math.random()")
	t.Logf("FSV unbound source: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "no deterministic source") {
		t.Fatalf("math.random with no source must fail loudly, got: %v", err)
	}
}

// TestMathRandomseedDisabled — edge: math.randomseed is a loud sandbox error
// (the seed is sim state, R-SIM-2), never a silent no-op.
func TestMathRandomseedDisabled(t *testing.T) {
	i := luabind.New()
	defer i.Close()
	i.SetRandomSource(newLCG(1))
	_, err := i.Eval("math.randomseed(123); return 1")
	t.Logf("FSV randomseed: err=%v", err)
	if err == nil || !strings.Contains(err.Error(), "randomseed is disabled") {
		t.Fatalf("math.randomseed must raise a loud sandbox error, got: %v", err)
	}
}

// floatStrEq compares a Lua tostring(number) against a Go float by re-rendering
// the Go value the way gopher-lua's LNumber.String does.
func floatStrEq(luaStr string, goVal float64) bool {
	return luaStr == luaNumberString(goVal)
}

// luaNumberString mirrors gopher-lua's LNumber.String(): integers print as
// int64, everything else as fmt.Sprint(float64) (pure-Go strconv shortest).
func luaNumberString(v float64) string {
	if !math.IsInf(v, 0) && !math.IsNaN(v) && v == math.Trunc(v) {
		return fmt.Sprint(int64(v))
	}
	return fmt.Sprint(v)
}
