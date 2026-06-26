package main

import (
	"os"
	"testing"
)

// fixture exercises every construct the parser must handle: type decls,
// plain + constant natives, `takes nothing returns nothing`, line comments,
// and a globals block interleaved between natives that must be skipped whole.
const fixture = `//============================================================================
// header comment
type agent extends handle // trailing comment
type unit  extends   widget

native FirstNative takes nothing returns nothing

globals
    integer udg_x = 5
    constant integer FOO = 1 // should NOT be parsed as a decl
endglobals

constant native ConvertRace takes integer i returns race
native          CreateUnit  takes player id, integer unitid, real x, real y, real face returns unit
native LastNative takes unit u returns boolean
`

func TestParseFixture(t *testing.T) {
	decls := ParseDecls(fixture)

	c := Tally(decls)
	if c.Types != 2 {
		t.Errorf("types = %d, want 2", c.Types)
	}
	if c.PlainNatives != 3 {
		t.Errorf("plain natives = %d, want 3", c.PlainNatives)
	}
	if c.ConstantNatives != 1 {
		t.Errorf("constant natives = %d, want 1", c.ConstantNatives)
	}

	// Source order must be preserved.
	wantOrder := []string{"agent", "unit", "FirstNative", "ConvertRace", "CreateUnit", "LastNative"}
	if len(decls) != len(wantOrder) {
		t.Fatalf("got %d decls, want %d: %+v", len(decls), len(wantOrder), decls)
	}
	for i, name := range wantOrder {
		if decls[i].Name != name {
			t.Errorf("decl[%d].Name = %q, want %q", i, decls[i].Name, name)
		}
	}

	byName := map[string]Decl{}
	for _, d := range decls {
		byName[d.Name] = d
	}

	// Edge 1: constant flag preserved.
	if d := byName["ConvertRace"]; d.Kind != KindConstantNative || !d.Constant {
		t.Errorf("ConvertRace not recorded as constant native: %+v", d)
	}
	if got, want := byName["ConvertRace"].Signature(), "constant native ConvertRace takes integer i returns race"; got != want {
		t.Errorf("ConvertRace sig = %q, want %q", got, want)
	}

	// Edge 2: `takes nothing returns nothing` -> zero params, not a param named "nothing".
	if d := byName["FirstNative"]; len(d.Params) != 0 {
		t.Errorf("FirstNative params = %+v, want none", d.Params)
	}
	if got, want := byName["FirstNative"].Signature(), "native FirstNative takes nothing returns nothing"; got != want {
		t.Errorf("FirstNative sig = %q, want %q", got, want)
	}

	// Edge 3 (globals interleave): FOO from the globals block must NOT appear,
	// while the natives before (FirstNative) and after (ConvertRace) parse.
	if _, ok := byName["FOO"]; ok {
		t.Error("FOO (globals-block constant) leaked into decls")
	}

	// CreateUnit verbatim signature must match the documented spot-check.
	wantCU := "native CreateUnit takes player id, integer unitid, real x, real y, real face returns unit"
	if got := byName["CreateUnit"].Signature(); got != wantCU {
		t.Errorf("CreateUnit sig = %q, want %q", got, wantCU)
	}
	cu := byName["CreateUnit"]
	if len(cu.Params) != 5 || cu.Params[0].Type != "player" || cu.Params[0].Name != "id" || cu.Returns != "unit" {
		t.Errorf("CreateUnit params/return malformed: %+v ret=%q", cu.Params, cu.Returns)
	}

	// type extends recorded.
	if d := byName["unit"]; d.Extends != "widget" {
		t.Errorf("type unit extends = %q, want widget", d.Extends)
	}
}

// TestParseRealCommonJ asserts exact counts against the vendored source. This
// is the regression guard: 1534 natives (1243 plain + 291 constant), 134 types.
func TestParseRealCommonJ(t *testing.T) {
	const path = "../../repoes/war3-types/scripts/common.j"
	src, err := os.ReadFile(path)
	if err != nil {
		t.Skipf("vendored common.j not present (%v); run setup to restore repoes/", err)
	}
	decls := ParseDecls(string(src))
	c := Tally(decls)

	if c.TotalNatives() != 1534 {
		t.Errorf("total natives = %d, want 1534", c.TotalNatives())
	}
	if c.PlainNatives != 1243 {
		t.Errorf("plain natives = %d, want 1243", c.PlainNatives)
	}
	if c.ConstantNatives != 291 {
		t.Errorf("constant natives = %d, want 291", c.ConstantNatives)
	}
	if c.Types != 134 {
		t.Errorf("types = %d, want 134", c.Types)
	}

	// Spot-check CreateUnit verbatim from the real file.
	var found bool
	for _, d := range decls {
		if d.Name == "CreateUnit" {
			found = true
			want := "native CreateUnit takes player id, integer unitid, real x, real y, real face returns unit"
			if d.Signature() != want {
				t.Errorf("real CreateUnit sig = %q, want %q", d.Signature(), want)
			}
		}
	}
	if !found {
		t.Error("CreateUnit not found in real common.j parse")
	}
}
