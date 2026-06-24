package luabind_test

import (
	"bytes"
	"strings"
	"testing"

	lua "github.com/yuin/gopher-lua"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
)

// Regression for #633: tostring of a table/function must not embed a heap
// address. That string can reach hashed sim state (SetGlobalKV → sim KV store,
// R-SIM-6); a per-run address would desync lockstep. SoT = the actual string.
func TestTostringNoAddressFSV(t *testing.T) {
	// Fork root: LValue.String() is address-free.
	L := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer L.Close()
	for _, c := range []struct {
		name string
		s    string
	}{
		{"table", L.NewTable().String()},
		{"function", L.NewFunction(func(*lua.LState) int { return 0 }).String()},
	} {
		t.Logf("BEFORE→AFTER %s.String() = %q", c.name, c.s)
		if strings.Contains(c.s, "0x") || strings.Contains(c.s, ": ") {
			t.Fatalf("%s.String()=%q still leaks an address (#633)", c.name, c.s)
		}
	}

	// Sandbox reach (the desync vector): tostring({}) is address-free and equal
	// across two fresh sandboxes — i.e. deterministic, the whole point.
	a, err := newTestSandbox().Eval(`return tostring({})`)
	if err != nil {
		t.Fatalf("eval a: %v", err)
	}
	b, err := newTestSandbox().Eval(`return tostring({})`)
	if err != nil {
		t.Fatalf("eval b: %v", err)
	}
	t.Logf("sandbox tostring({}) run-A=%q run-B=%q", a, b)
	if strings.Contains(a, "0x") {
		t.Fatalf("sandbox tostring({})=%q leaks an address (#633)", a)
	}
	if a != b {
		t.Fatalf("sandbox tostring({}) nondeterministic: %q != %q (#633)", a, b)
	}
}

// Regression for #634: SerializeValue must be byte-reproducible even for tables
// that spilled past small-mode (>8 string keys) into the fork's backing Go maps.
// Before the fix encodeTable used ForEach (Go-map order) → a different blob every
// run. SoT = the actual bytes across N runs, plus a round-trip content check.
func TestSerializeManyKeysDeterministicFSV(t *testing.T) {
	keys := []string{
		"alpha", "bravo", "charlie", "delta", "echo", "foxtrot",
		"golf", "hotel", "india", "juliet", "kilo", "lima", "mike", "november",
	} // 14 > smallTableMax(8) → spills to strdict
	build := func() *lua.LTable {
		L := lua.NewState(lua.Options{SkipOpenLibs: true})
		defer L.Close()
		tb := L.NewTable()
		for _, k := range keys {
			tb.RawSetString(k, lua.LString("v_"+k))
			// nested table per key — also exercises deterministic intern-id order
			n := L.NewTable()
			n.RawSetString("of", lua.LString(k))
			tb.RawSetString(k+"_n", n)
		}
		return tb
	}

	first, err := luabind.SerializeValue(build())
	if err != nil {
		t.Fatalf("serialize: %v", err)
	}
	t.Logf("BEFORE: blob is %d bytes", len(first))
	for i := 0; i < 64; i++ {
		b, err := luabind.SerializeValue(build())
		if err != nil {
			t.Fatalf("serialize run %d: %v", i, err)
		}
		if !bytes.Equal(b, first) {
			t.Fatalf("AFTER: run %d blob differs from run 0 — nondeterministic save (#634)", i)
		}
	}
	t.Logf("AFTER: 64 serializations of a 28-key (>8) graph all byte-identical")

	// Round-trip preserves every key (deterministic order must not drop entries).
	dst := lua.NewState(lua.Options{SkipOpenLibs: true})
	defer dst.Close()
	got, err := luabind.DeserializeValue(dst, first)
	if err != nil {
		t.Fatalf("deserialize: %v", err)
	}
	d := got.(*lua.LTable)
	for _, k := range keys {
		if v := d.RawGetString(k); v != lua.LString("v_"+k) {
			t.Fatalf("round-trip lost key %q: got %v", k, v)
		}
		if n := d.RawGetString(k + "_n"); n == lua.LNil {
			t.Fatalf("round-trip lost nested key %q_n", k)
		}
	}
	t.Logf("round-trip preserved all %d string keys + nested tables", len(keys)*2)
}
