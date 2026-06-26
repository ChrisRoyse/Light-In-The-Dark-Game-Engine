package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestTimerKillScenarioFSV is the harness's own Full State Verification: it runs
// the committed timer-kill scenario and asserts the Source of Truth (the R-FSV-2
// state dump) shows exactly the known-expected delta — victim (P1) killed,
// survivor (P2) untouched — plus determinism. Known input -> known output.
func TestTimerKillScenarioFSV(t *testing.T) {
	sc, err := loadScenario("testdata/timer-kill.toml")
	if err != nil {
		t.Fatalf("load scenario: %v", err)
	}
	rep, err := runScenario(sc, true)
	if err != nil {
		t.Fatalf("run scenario: %v", err)
	}
	if !rep.Changed {
		t.Fatalf("expected state to change; top hash identical before/after")
	}
	if !rep.Deterministic {
		t.Fatalf("scenario not deterministic: after=%s run2=%s", rep.HashAfter, rep.HashAfterRun2)
	}
	// SoT read: the entity sub-hash MUST be among the changed systems (the kill
	// touched entity/health state); a pure clock advance would not list it.
	if !containsStr(rep.ChangedSys, "entities") {
		t.Fatalf("entities sub-hash unchanged — victim was NOT killed; changed=%v", rep.ChangedSys)
	}

	before := decodeDump(t, rep.StateBefore)
	after := decodeDump(t, rep.StateAfter)
	if before.UnitCount != 2 || after.UnitCount != 1 {
		t.Fatalf("unitCount before=%d after=%d; want 2 -> 1 (P1 victim killed)", before.UnitCount, after.UnitCount)
	}
	if owners := players(after); len(owners) != 1 || owners[0] != 2 {
		t.Fatalf("survivor identity wrong: after-state owners=%v; want only player 2", owners)
	}
	t.Logf("FSV: unitCount 2->1; before owners=%v after owners=%v; hashAfter=%s deterministic=%v",
		players(before), players(after), rep.HashAfter, rep.Deterministic)
}

// TestUndecodedKeyFailsClosed is the regression for the fail-open parse bug:
// a top-level scalar written AFTER a [[units]] array is nested under the array
// element by TOML, and the harness used to silently drop it, producing an empty
// scenario that "passed" while verifying nothing. It must now error loudly,
// naming the misplaced key.
func TestUndecodedKeyFailsClosed(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "misplaced.toml")
	// `lua` sits after [[units]] -> TOML reads it as units.lua.
	body := "name=\"bad\"\nticks=1\n[[units]]\nid=\"hfoo\"\nlife=1\nlua=\"x=1\"\n"
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	_, err := loadScenario(path)
	if err == nil {
		t.Fatal("misplaced top-level key must fail closed, but load succeeded")
	}
	if !strings.Contains(err.Error(), "units.lua") {
		t.Fatalf("error must name the misplaced key units.lua, got: %v", err)
	}
}

type dumpView struct {
	UnitCount int `json:"unitCount"`
	Entities  []struct {
		Player *uint8 `json:"player"`
	} `json:"entities"`
}

func decodeDump(t *testing.T, raw json.RawMessage) dumpView {
	t.Helper()
	var d dumpView
	if err := json.Unmarshal(raw, &d); err != nil {
		t.Fatalf("decode dump: %v", err)
	}
	return d
}

func players(d dumpView) []uint8 {
	out := make([]uint8, 0, len(d.Entities))
	for _, e := range d.Entities {
		if e.Player != nil {
			out = append(out, *e.Player)
		}
	}
	return out
}

func containsStr(s []string, want string) bool {
	for _, v := range s {
		if v == want {
			return true
		}
	}
	return false
}
