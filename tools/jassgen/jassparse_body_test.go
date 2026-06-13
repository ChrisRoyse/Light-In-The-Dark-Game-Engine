package main

import (
	"os"
	"testing"
)

const bodyFixture = `globals
    integer bj_x = 5
    constant real bj_PI = 3.14 // comment line, not counted
endglobals

function IsUnitPausedBJ takes unit whichUnit returns boolean
    return IsUnitPaused(whichUnit)
endfunction

function SetUnitLifeBJ takes unit whichUnit, real newValue returns nothing
    call SetUnitState(whichUnit, UNIT_STATE_LIFE, RMaxBJ(0,newValue))
endfunction

function RMaxBJ takes real a, real b returns real
    if (a < b) then
        return b
    else
        return a
    endif
endfunction

function PolledWait takes real duration returns nothing
    local timer t
    local real timeRemaining
    if (duration > 0) then
        set t = CreateTimer()
        loop
            set timeRemaining = TimerGetRemaining(t)
            exitwhen timeRemaining <= 0
        endloop
        call DestroyTimer(t)
    endif
endfunction
`

func TestParseBodyFixture(t *testing.T) {
	funcs := ParseFuncs(bodyFixture)
	if len(funcs) != 4 {
		t.Fatalf("got %d funcs, want 4: %+v", len(funcs), funcs)
	}
	byName := map[string]Func{}
	for _, f := range funcs {
		byName[f.Name] = f
	}

	// Globals: 2 declaration lines (bj_x, bj_PI); the // comment line excluded.
	if g := CountGlobals(bodyFixture); g != 2 {
		t.Errorf("globals decls = %d, want 2", g)
	}

	// Edge 1: passthrough return.
	ip := byName["IsUnitPausedBJ"]
	if ip.Shape() != "passthrough-return" {
		t.Errorf("IsUnitPausedBJ shape = %q, want passthrough-return", ip.Shape())
	}
	if len(ip.Body) != 1 {
		t.Fatalf("IsUnitPausedBJ body len = %d", len(ip.Body))
	}
	ret, ok := ip.Body[0].(ReturnStmt)
	if !ok {
		t.Fatalf("IsUnitPausedBJ stmt not return: %T", ip.Body[0])
	}
	ce, ok := ret.Value.(CallExpr)
	if !ok || ce.Func != "IsUnitPaused" || len(ce.Args) != 1 {
		t.Fatalf("IsUnitPausedBJ return expr malformed: %+v", ret.Value)
	}
	if id, ok := ce.Args[0].(Ident); !ok || id.Name != "whichUnit" {
		t.Errorf("IsUnitPausedBJ arg not passthrough ident: %+v", ce.Args[0])
	}

	// Edge 2: nested call arg tree, single-call-modified (D2-shaped).
	sl := byName["SetUnitLifeBJ"]
	if sl.Shape() != "single-call-modified" {
		t.Errorf("SetUnitLifeBJ shape = %q, want single-call-modified", sl.Shape())
	}
	cs, ok := sl.Body[0].(CallStmt)
	if !ok {
		t.Fatalf("SetUnitLifeBJ stmt not call: %T", sl.Body[0])
	}
	if cs.Call.Func != "SetUnitState" || len(cs.Call.Args) != 3 {
		t.Fatalf("SetUnitState call malformed: %+v", cs.Call)
	}
	nested, ok := cs.Call.Args[2].(CallExpr)
	if !ok || nested.Func != "RMaxBJ" || len(nested.Args) != 2 {
		t.Errorf("nested RMaxBJ arg not parsed as call tree: %+v", cs.Call.Args[2])
	}

	// Edge 3: control flow (D4-shaped).
	pw := byName["PolledWait"]
	if pw.Shape() != "control-flow" {
		t.Errorf("PolledWait shape = %q, want control-flow", pw.Shape())
	}
	if !hasControlFlow(pw.Body) {
		t.Error("PolledWait body lacks control flow")
	}
	// RMaxBJ has if/else => control-flow.
	if byName["RMaxBJ"].Shape() != "control-flow" {
		t.Errorf("RMaxBJ shape = %q, want control-flow", byName["RMaxBJ"].Shape())
	}

	// Source order preserved.
	want := []string{"IsUnitPausedBJ", "SetUnitLifeBJ", "RMaxBJ", "PolledWait"}
	for i, n := range want {
		if funcs[i].Name != n {
			t.Errorf("func[%d] = %q, want %q", i, funcs[i].Name, n)
		}
	}
}

// TestParseRealBlizzardJ asserts exact function count vs the vendored source.
func TestParseRealBlizzardJ(t *testing.T) {
	const path = "../../repoes/war3-types/scripts/blizzard.j"
	src, err := os.ReadFile(path)
	if err != nil {
		t.Skipf("vendored blizzard.j not present (%v)", err)
	}
	funcs := ParseFuncs(string(src))
	if len(funcs) != 985 {
		t.Errorf("functions = %d, want 985", len(funcs))
	}

	byName := map[string]Func{}
	for _, f := range funcs {
		byName[f.Name] = f
	}
	if f, ok := byName["IsUnitPausedBJ"]; !ok || f.Shape() != "passthrough-return" {
		t.Errorf("real IsUnitPausedBJ shape = %q (ok=%v), want passthrough-return", f.Shape(), ok)
	}
	if f, ok := byName["SetUnitLifeBJ"]; !ok || f.Shape() != "single-call-modified" {
		t.Errorf("real SetUnitLifeBJ shape = %q (ok=%v), want single-call-modified", f.Shape(), ok)
	}
	if f, ok := byName["PolledWait"]; !ok || f.Shape() != "control-flow" {
		t.Errorf("real PolledWait shape = %q (ok=%v), want control-flow", f.Shape(), ok)
	}
}
