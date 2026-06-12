package data

import (
	"strings"
	"testing"
	"testing/fstest"
)

const goodSmart = `
unit-classes = ["fighter", "worker"]
[rules]
ground-point = ["move", "move"]
enemy = ["attack", "attack"]
ally = ["move", "move"]
own-building = ["rally", "rally"]
resource = ["move", "harvest"]
item = ["move", "move"]
`

func smartFS(s string) fstest.MapFS {
	return fstest.MapFS{"orders/smart.toml": &fstest.MapFile{Data: []byte(s)}}
}

func TestSmartTableLoads(t *testing.T) {
	tbl, err := LoadSmart(smartFS(goodSmart))
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("unit classes: %v", tbl.UnitClasses)
	for tc, row := range tbl.Rules {
		t.Logf("%-13s → %v", TargetClassNames[tc], row)
	}
	if tbl.Rules[TCResource][1] != OpcodeByName["harvest"] {
		t.Fatalf("resource × worker must be harvest")
	}
	if tbl.Rules[TCEnemy][0] != OpcodeByName["attack"] {
		t.Fatalf("enemy × fighter must be attack")
	}
}

// Edge 4 (issue FSV): an order name outside the v1 opcode registry —
// e.g. the spec §4 "follow" the wire cannot carry — is a LOAD error
// naming row, class, and name. Never a runtime fallback.
func TestSmartTableUnknownOrderName(t *testing.T) {
	bad := strings.Replace(goodSmart, `ally = ["move", "move"]`, `ally = ["follow", "follow"]`, 1)
	_, err := LoadSmart(smartFS(bad))
	if err == nil {
		t.Fatal("unknown order name must fail the load")
	}
	t.Logf("loader error: %v", err)
	if !strings.Contains(err.Error(), "follow") || !strings.Contains(err.Error(), "registry") {
		t.Fatalf("error must name the rejected order: %v", err)
	}
}

// A missing target-class row is a load error — the table must be
// total.
func TestSmartTableMissingRow(t *testing.T) {
	bad := strings.Replace(goodSmart, "item = [\"move\", \"move\"]\n", "", 1)
	_, err := LoadSmart(smartFS(bad))
	if err == nil {
		t.Fatal("missing target class must fail the load")
	}
	t.Logf("loader error: %v", err)

	short := strings.Replace(goodSmart, `enemy = ["attack", "attack"]`, `enemy = ["attack"]`, 1)
	_, err = LoadSmart(smartFS(short))
	if err == nil {
		t.Fatal("short row must fail the load")
	}
	t.Logf("loader error: %v", err)
}
