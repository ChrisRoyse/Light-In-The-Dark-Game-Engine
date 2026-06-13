package main

import (
	"os"
	"testing"
)

const dtsFixture = `/** @noSelfInFile **/
declare interface agent extends handle { __agent: never; }
declare interface unit extends widget { __unit: never; }
declare function CreateUnit(id: player, unitid: number, x: number, y: number, face: number): unit;
declare function GroupEnumUnitsOfType(whichGroup: group, unitname: string, filter: boolexpr | (() => boolean) | null): void;
declare function TimerStart(whichTimer: timer, timeout: number, periodic: boolean, handlerFunc: () => void): void;
`

func TestParseDTS(t *testing.T) {
	decls := ParseDTS(dtsFixture)
	byName := map[string]DTSDecl{}
	for _, d := range decls {
		byName[d.Name] = d
	}

	// interface hierarchy
	if d := byName["unit"]; d.Kind != "interface" || d.Extends != "widget" {
		t.Errorf("unit interface = %+v, want extends widget", d)
	}

	// CreateUnit params + return
	cu := byName["CreateUnit"]
	if len(cu.Params) != 5 {
		t.Fatalf("CreateUnit params = %d, want 5", len(cu.Params))
	}
	if cu.Params[0].Name != "id" || cu.Params[0].TSType != "player" {
		t.Errorf("CreateUnit p0 = %+v", cu.Params[0])
	}
	if cu.Returns != "unit" {
		t.Errorf("CreateUnit return = %q, want unit", cu.Returns)
	}

	// Function-type param with union null must NOT be split on its inner punctuation.
	ge := byName["GroupEnumUnitsOfType"]
	if len(ge.Params) != 3 {
		t.Fatalf("GroupEnumUnitsOfType params = %d, want 3: %+v", len(ge.Params), ge.Params)
	}
	if !ge.Params[2].Nullable {
		t.Errorf("filter param should be nullable: %+v", ge.Params[2])
	}
	if ge.Returns != "void" {
		t.Errorf("GroupEnumUnitsOfType return = %q, want void", ge.Returns)
	}

	// () => void param kept as a single param.
	ts := byName["TimerStart"]
	if len(ts.Params) != 4 {
		t.Errorf("TimerStart params = %d, want 4: %+v", len(ts.Params), ts.Params)
	}
}

func TestMergeArityMismatchAndOnly(t *testing.T) {
	jass := ParseDecls(
		"native CreateUnit takes player id, integer unitid, real x, real y, real face returns unit\n" +
			"native OnlyInJass takes nothing returns nothing\n" +
			"native AritySkew takes integer a, integer b returns nothing\n")
	dts := ParseDTS(
		"declare function CreateUnit(id: player, unitid: number, x: number, y: number, face: number): unit;\n" +
			"declare function OnlyInDTS(): void;\n" +
			"declare function AritySkew(a: number): void;\n")
	res := Merge(DeclsToSigs(jass), "common", dts)

	kinds := map[string]Discrepancy{}
	for _, d := range res.Discrepancies {
		kinds[d.Kind+":"+d.Name] = d
	}
	// Edge 1: present in .j, absent in .d.ts.
	if _, ok := kinds["jass-only:OnlyInJass"]; !ok {
		t.Errorf("missing jass-only discrepancy for OnlyInJass: %+v", res.Discrepancies)
	}
	// Edge 2: present in .d.ts only.
	if _, ok := kinds["dts-only:OnlyInDTS"]; !ok {
		t.Errorf("missing dts-only discrepancy for OnlyInDTS: %+v", res.Discrepancies)
	}
	// Edge 3: arity mismatch flagged, both signatures present in detail.
	d, ok := kinds["arity-mismatch:AritySkew"]
	if !ok {
		t.Fatalf("missing arity-mismatch for AritySkew: %+v", res.Discrepancies)
	}
	if d.Detail == "" {
		t.Error("arity-mismatch detail should print both signatures")
	}

	// CreateUnit enriched: jassType real -> tsType number.
	var cu *MergedEntry
	for i := range res.Entries {
		if res.Entries[i].Name == "CreateUnit" {
			cu = &res.Entries[i]
		}
	}
	if cu == nil {
		t.Fatal("CreateUnit not merged")
	}
	if cu.Params[2].JassType != "real" || cu.Params[2].TSType != "number" {
		t.Errorf("CreateUnit x enrichment = %+v, want jass real / ts number", cu.Params[2])
	}
}

// TestMergeRealSources asserts the empirically-verified discrepancy structure
// against the vendored files: common & blizzard join cleanly (0 discrepancies);
// commonai has exactly 120 dts-only AI-script functions and 0 jass-only.
func TestMergeRealSources(t *testing.T) {
	read := func(p string) (string, bool) {
		b, err := os.ReadFile(p)
		if err != nil {
			return "", false
		}
		return string(b), true
	}
	cj, ok1 := read("../../repoes/war3-types/scripts/common.j")
	cd, ok2 := read("../../repoes/war3-types/core/common.d.ts")
	bj, ok3 := read("../../repoes/war3-types/scripts/blizzard.j")
	bd, ok4 := read("../../repoes/war3-types/core/blizzard.d.ts")
	aj, ok5 := read("../../repoes/war3-types/scripts/common.ai")
	ad, ok6 := read("../../repoes/war3-types/core/commonai.d.ts")
	if !(ok1 && ok2 && ok3 && ok4 && ok5 && ok6) {
		t.Skip("vendored war3-types sources not present")
	}

	common := Merge(DeclsToSigs(ParseDecls(cj)), "common", ParseDTS(cd))
	if common.JassNatives != 1534 || common.DTSFunctions != 1534 {
		t.Errorf("common counts: natives=%d dtsfns=%d, want 1534/1534", common.JassNatives, common.DTSFunctions)
	}
	if len(common.Discrepancies) != 0 {
		t.Errorf("common discrepancies = %d, want 0: %+v", len(common.Discrepancies), common.Discrepancies)
	}

	bliz := Merge(FuncsToSigs(ParseFuncs(bj)), "blizzard", ParseDTS(bd))
	if len(bliz.Discrepancies) != 0 {
		t.Errorf("blizzard discrepancies = %d, want 0", len(bliz.Discrepancies))
	}

	ai := Merge(DeclsToSigs(ParseDecls(aj)), "commonai", ParseDTS(ad))
	if ai.JassNatives != 123 {
		t.Errorf("commonai natives = %d, want 123", ai.JassNatives)
	}
	jassOnly, dtsOnly := 0, 0
	for _, d := range ai.Discrepancies {
		switch d.Kind {
		case "jass-only":
			jassOnly++
		case "dts-only":
			dtsOnly++
		}
	}
	if jassOnly != 0 {
		t.Errorf("commonai jass-only = %d, want 0", jassOnly)
	}
	if dtsOnly != 120 {
		t.Errorf("commonai dts-only = %d, want 120 (AI-script functions)", dtsOnly)
	}

	// Hierarchy enrichment: unit extends widget (from common.d.ts).
	if common.Hierarchy["unit"] != "widget" {
		t.Errorf("hierarchy unit -> %q, want widget", common.Hierarchy["unit"])
	}
}
