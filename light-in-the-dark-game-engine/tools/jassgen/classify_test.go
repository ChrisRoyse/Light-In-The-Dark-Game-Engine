package main

import (
	"os"
	"testing"
)

// classifyOne is a test helper: classify a single BJ body against given symbol sets.
func classifyOne(t *testing.T, src string, name string, natives, bjs []string) Classification {
	t.Helper()
	funcs := ParseFuncs(src)
	var target *Func
	for i := range funcs {
		if funcs[i].Name == name {
			target = &funcs[i]
		}
	}
	if target == nil {
		t.Fatalf("function %s not parsed from fixture", name)
	}
	natSet := map[string]bool{}
	for _, n := range natives {
		natSet[n] = true
	}
	bjSet := map[string]bool{}
	for _, b := range bjs {
		bjSet[b] = true
	}
	return classifyBJ(*target, natSet, bjSet)
}

func TestClassifyBJShapes(t *testing.T) {
	src := `
function IssueTargetOrderBJ takes unit whichUnit, string order, widget targetWidget returns boolean
    return IssueTargetOrder( whichUnit, order, targetWidget )
endfunction
function PauseUnitBJ takes boolean pause, unit whichUnit returns nothing
    call PauseUnit(whichUnit, pause)
endfunction
function UnitDamageTargetBJ takes unit whichUnit, unit target, real amount, attacktype whichAttack, damagetype whichDamage returns boolean
    return UnitDamageTarget(whichUnit, target, amount, true, false, whichAttack, whichDamage, WEAPON_TYPE_WHOKNOWS)
endfunction
function GetTransportUnitBJ takes nothing returns unit
    return GetTransportUnit()
endfunction
function SetUnitLifeBJ takes unit whichUnit, real newValue returns nothing
    call SetUnitState(whichUnit, UNIT_STATE_LIFE, RMaxBJ(0,newValue))
endfunction
function ForGroupBJ takes group whichGroup, code callback returns nothing
    local boolean wantDestroy = bj_wantDestroyGroup
    set bj_wantDestroyGroup = false
    call ForGroup(whichGroup, callback)
    if (wantDestroy) then
        call DestroyGroup(whichGroup)
    endif
endfunction
function CreateUnitWrapBJ takes player p returns unit
    set bj_lastCreatedUnit = CreateUnit(p, 1, 0, 0, 0)
    return bj_lastCreatedUnit
endfunction
function WrapsAnotherBJ takes real a, real b returns real
    return RMaxBJ(a, b)
endfunction
`
	natives := []string{"IssueTargetOrder", "PauseUnit", "UnitDamageTarget", "GetTransportUnit", "SetUnitState", "ForGroup", "DestroyGroup", "CreateUnit"}
	bjs := []string{"RMaxBJ", "ForGroupBJ", "IssueTargetOrderBJ"}

	cases := []struct {
		name  string
		class Class
	}{
		{"IssueTargetOrderBJ", ClassD1}, // worked ex 1
		{"PauseUnitBJ", ClassD2},        // worked ex 2 (reorder)
		{"UnitDamageTargetBJ", ClassD2}, // worked ex 3 (hardcoded defaults)
		{"GetTransportUnitBJ", ClassD1}, // edge 3 (zero-arg passthrough)
		{"SetUnitLifeBJ", ClassD5},      // routes through SetUnitState
		{"ForGroupBJ", ClassD4},         // edge 1 (NOT D1 — bookkeeping)
		{"CreateUnitWrapBJ", ClassD2},   // side-channel capture
		{"WrapsAnotherBJ", ClassD1},     // edge 2 (wraps a BJ)
	}
	for _, tc := range cases {
		got := classifyOne(t, src, tc.name, natives, bjs)
		if got.Class != tc.class {
			t.Errorf("%s = %s, want %s (evidence: %s)", tc.name, got.Class, tc.class, got.Evidence)
		}
	}

	// Edge 2 resolution evidence: WrapsAnotherBJ must note its callee is a BJ.
	w := classifyOne(t, src, "WrapsAnotherBJ", natives, bjs)
	if w.Class != ClassD1 || !contains([]string{w.Evidence}, w.Evidence) || !containsSub(w.Evidence, "wraps BJ") {
		t.Errorf("WrapsAnotherBJ evidence should note BJ resolution: %q", w.Evidence)
	}
}

func containsSub(s, sub string) bool { return len(s) >= len(sub) && (indexOf(s, sub) >= 0) }
func indexOf(s, sub string) int {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}

func TestProposeD3Families(t *testing.T) {
	natives := []string{"SetUnitX", "SetUnitY", "SetUnitPosition", "SetUnitPositionLoc",
		"GroupEnumUnitsInRect", "GroupEnumUnitsInRectCounted", "Unrelated"}
	set := map[string]bool{}
	for _, n := range natives {
		set[n] = true
	}
	fam := proposeD3Families(natives, set)
	for _, n := range []string{"SetUnitX", "SetUnitY", "SetUnitPosition", "SetUnitPositionLoc",
		"GroupEnumUnitsInRect", "GroupEnumUnitsInRectCounted"} {
		if fam[n] == "" {
			t.Errorf("%s should be proposed D3 (family), got none", n)
		}
	}
	if fam["Unrelated"] != "" {
		t.Errorf("Unrelated should not be D3, got family %q", fam["Unrelated"])
	}
}

// TestClassifyRealWorkedExamples runs the full classifier over the vendored
// sources and asserts every deduplication-policy.md worked example + edge.
func TestClassifyRealWorkedExamples(t *testing.T) {
	rd := func(p string) (string, bool) { b, e := os.ReadFile(p); return string(b), e == nil }
	bjSrc, ok1 := rd("../../repoes/war3-types/scripts/blizzard.j")
	cjSrc, ok2 := rd("../../repoes/war3-types/scripts/common.j")
	aiSrc, ok3 := rd("../../repoes/war3-types/scripts/common.ai")
	if !(ok1 && ok2 && ok3) {
		t.Skip("vendored sources not present")
	}
	bj := ParseFuncs(bjSrc)
	commonN := ParseDecls(cjSrc)
	aiN := ParseDecls(aiSrc)
	natives := append(append([]Decl{}, commonN...), aiN...)
	origins := map[string]string{}
	for _, d := range commonN {
		origins[d.Name] = "common"
	}
	cs := ClassifyAll(bj, "blizzard", natives, origins)
	byName := map[string]Classification{}
	for _, c := range cs {
		byName[c.Name] = c
	}

	want := map[string]Class{
		"IssueTargetOrderBJ": ClassD1,
		"PauseUnitBJ":        ClassD2,
		"UnitDamageTargetBJ": ClassD2,
		"SetUnitX":           ClassD3,
		"SetUnitY":           ClassD3,
		"SetUnitPosition":    ClassD3,
		"SetUnitPositionLoc": ClassD3,
		"PolledWait":         ClassD4,
		"GetUnitState":       ClassD5,
		"ForGroupBJ":         ClassD4, // edge 1
		"GetTransportUnitBJ": ClassD1, // edge 3
	}
	for name, cls := range want {
		got, ok := byName[name]
		if !ok {
			t.Errorf("%s not classified (missing)", name)
			continue
		}
		if got.Class != cls {
			t.Errorf("%s = %s, want %s (evidence: %s)", name, got.Class, cls, got.Evidence)
		}
	}

	// Nothing is silently tombstoned; every entry carries classifiedBy=heuristic.
	for _, c := range cs {
		if c.ClassifiedBy != "heuristic" {
			t.Errorf("%s classifiedBy = %q, want heuristic", c.Name, c.ClassifiedBy)
		}
	}
}
