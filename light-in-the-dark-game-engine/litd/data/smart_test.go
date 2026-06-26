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
transport = ["board", "board"]
own-building = ["rally", "rally"]
damaged-own = ["move", "repair"]
construction = ["move", "resume-construction"]
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

// Edge 4 (issue FSV): an order name outside the opcode registry — here
// the invented "teleport" — is a LOAD error naming row, class, and
// name. Never a runtime fallback. ("follow"/"get-item" used to be
// unrepresentable but joined the registry in #306/#305.)
func TestSmartTableUnknownOrderName(t *testing.T) {
	bad := strings.Replace(goodSmart, `ally = ["move", "move"]`, `ally = ["teleport", "teleport"]`, 1)
	_, err := LoadSmart(smartFS(bad))
	if err == nil {
		t.Fatal("unknown order name must fail the load")
	}
	t.Logf("loader error: %v", err)
	if !strings.Contains(err.Error(), "teleport") || !strings.Contains(err.Error(), "registry") {
		t.Fatalf("error must name the rejected order: %v", err)
	}
}

// A missing target-class row is a load error — the table must be
// total.
func TestSmartTableMissingRow(t *testing.T) {
	bad := strings.Replace(goodSmart, "construction = [\"move\", \"resume-construction\"]\n", "", 1)
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
