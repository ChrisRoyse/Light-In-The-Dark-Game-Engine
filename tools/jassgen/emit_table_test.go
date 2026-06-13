package main

import (
	"strings"
	"testing"
)

func TestRenderMappingTableRowsAndCollapse(t *testing.T) {
	cs := []Classification{
		{Name: "SetUnitPosition", Origin: "common", Class: ClassD3, GoMapping: "Unit.SetPosition", Package: "litd/api", CollapsesWith: []string{"SetUnitX", "SetUnitY"}},
		{Name: "SetUnitX", Origin: "common", Class: ClassD3},
		{Name: "SetUnitY", Origin: "common", Class: ClassD3},
		{Name: "GetLastCreatedUnit", Origin: "blizzard", Class: ClassD2, Tombstone: "superseded", Evidence: "tombstone: return values replace it"},
		{Name: "GetUnitCount", Origin: "commonai", Class: ClassD2, GoMapping: "UnitCount", Package: "litd/ai"},
		{Name: "Unmapped", Origin: "common", Class: ClassUnclassified},
	}
	table, rows := RenderMappingTable(cs)
	if rows != 6 {
		t.Errorf("rows = %d, want 6", rows)
	}
	// Collapse members point at the canonical symbol.
	for _, name := range []string{"SetUnitX", "SetUnitY"} {
		line := findRow(table, name)
		if !strings.Contains(line, "Unit.SetPosition") || !strings.Contains(line, "D3 collapse") {
			t.Errorf("%s row not pointed at canonical: %q", name, line)
		}
	}
	// Tombstone shows reason + detail, not blank.
	tomb := findRow(table, "GetLastCreatedUnit")
	if !strings.Contains(tomb, "tombstoned") || !strings.Contains(tomb, "superseded") || !strings.Contains(tomb, "return values replace it") {
		t.Errorf("tombstone row missing reason/detail: %q", tomb)
	}
	// commonai row carries origin + litd/ai.
	ai := findRow(table, "GetUnitCount")
	if !strings.Contains(ai, "commonai") || !strings.Contains(ai, "litd/ai.UnitCount") {
		t.Errorf("commonai row wrong: %q", ai)
	}
	// Unmapped shows pending, not blank.
	if !strings.Contains(findRow(table, "Unmapped"), "pending") {
		t.Errorf("unmapped row should say pending")
	}
}

func findRow(table, name string) string {
	for _, ln := range strings.Split(table, "\n") {
		if strings.HasPrefix(ln, "| "+name+" |") {
			return ln
		}
	}
	return ""
}
