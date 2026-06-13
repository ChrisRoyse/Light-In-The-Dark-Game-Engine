package main

import (
	"strings"
	"testing"
)

func luaManifest() Manifest {
	mk := func(name, sym, pkg, sig, disp string, tomb *TombstoneT) FunctionEntry {
		e := FunctionEntry{
			Name: name, Origin: "blizzard.j",
			Signature:      Signature{Params: []ParamEntry{}, Returns: "nothing"},
			Classification: "D5", ClassifiedBy: "override", Disposition: disp,
		}
		if disp == "mapped" {
			e.GoMapping = &GoMapping{Symbol: sym, Package: pkg, GoSignature: sig}
		} else {
			e.Tombstone = tomb
		}
		return e
	}
	return Manifest{SchemaVersion: 1, Functions: []FunctionEntry{
		mk("SetUnitLifeBJ", "Unit.SetLife", "litd/api", "(v float64)", "mapped", nil),
		mk("GetUnitCount", "UnitCount", "litd/ai", "(id int) int", "mapped", nil),
		mk("DoNothing", "", "", "", "tombstoned", &TombstoneT{Reason: "gameplay-irrelevant", Detail: "no-op"}),
	}}
}

func TestCollectLuaBindingsSplitAndDedup(t *testing.T) {
	m := luaManifest()
	core, ai := CollectLuaBindings(m)
	if len(core) != 1 || core[0].LuaName != "Unit_SetLife" {
		t.Errorf("core = %+v, want [Unit_SetLife]", core)
	}
	if len(ai) != 1 || ai[0].LuaName != "UnitCount" {
		t.Errorf("ai = %+v, want [UnitCount]", ai)
	}
}

func TestRenderLuaBindingsEdges(t *testing.T) {
	src := RenderLuaBindings(luaManifest())
	// Edge 1: D5 accessor present.
	if !strings.Contains(src, `LuaName: "Unit_SetLife"`) {
		t.Error("D5 accessor Unit_SetLife missing")
	}
	// Edge 2: tombstoned DoNothing absent.
	if strings.Contains(src, "DoNothing") {
		t.Error("tombstoned DoNothing must not appear in bindings")
	}
	// Edge 3: litd/ai binding present in AIBindings.
	if !strings.Contains(src, "AIBindings") || !strings.Contains(src, `LuaName: "UnitCount"`) {
		t.Error("litd/ai binding UnitCount missing from AI surface")
	}
	// DO-NOT-EDIT header + compiles-as-package guarantee.
	if !strings.Contains(src, "DO NOT EDIT") || !strings.Contains(src, "package luabind") {
		t.Error("missing header or package decl")
	}
}

func TestLuaBindingName(t *testing.T) {
	if luaBindingName("Unit.Paused") != "Unit_Paused" {
		t.Error("dotted symbol not converted")
	}
	if luaBindingName("PolledWait") != "PolledWait" {
		t.Error("bare symbol changed")
	}
}
