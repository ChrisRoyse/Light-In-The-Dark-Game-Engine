package data

// #300 economy-table tests. SoT = the converted rows and the named
// refusal errors.

import (
	"strings"
	"testing"
	"testing/fstest"
)

const econBase = `
resource-types = ["gold", "lumber"]

[[node]]
id = "gold-mine"
resource = "gold"
amount = 12500
exclusive = true

[[node]]
id = "tree"
resource = "lumber"
amount = 400
`

const econUnit = `
[[unit]]
id = "worker"
life = 220
armor = 0
armor-type = "none"
move-speed = 70
turn-rate = 3
collision-size = 16
pathing = "ground"
acquisition-range = 0
model = "units/worker.glb"
food-cost = 1
[unit.harvest]
gather-seconds = 1.0
capacity = 10
resources = ["gold", "lumber"]

[[unit]]
id = "hall"
life = 1500
armor = 5
armor-type = "none"
move-speed = 0
turn-rate = 0
collision-size = 64
pathing = "ground"
acquisition-range = 0
model = "units/hall.glb"
food-provided = 10
depot-for = ["gold", "lumber"]
`

func econFS(econ, units string) fstest.MapFS {
	fs := fstest.MapFS{
		"combat/damage-table.toml": &fstest.MapFile{Data: []byte("attack-types = [\"normal\"]\narmor-types = [\"none\"]\n[coefficients]\nnormal = [1000]\n")},
	}
	if econ != "" {
		fs["economy/resources.toml"] = &fstest.MapFile{Data: []byte(econ)}
	}
	if units != "" {
		fs["units/core.toml"] = &fstest.MapFile{Data: []byte(units)}
	}
	return fs
}

func TestEconomyHappyPath(t *testing.T) {
	tb, err := Load(econFS(econBase, econUnit))
	if err != nil {
		t.Fatal(err)
	}
	if len(tb.ResourceTypes) != 2 || tb.ResourceTypes[0] != "gold" {
		t.Fatalf("resource types: %v", tb.ResourceTypes)
	}
	if len(tb.Nodes) != 2 {
		t.Fatalf("nodes: %d", len(tb.Nodes))
	}
	mine := tb.Nodes[0]
	if mine.ID != "gold-mine" || mine.Resource != 0 || mine.Amount != 12500 || !mine.Exclusive {
		t.Fatalf("mine row: %+v", mine)
	}
	t.Logf("node[0]: %+v  node[1]: %+v", tb.Nodes[0], tb.Nodes[1])
	var worker, hall *Unit
	for i := range tb.Units {
		switch tb.Units[i].ID {
		case "worker":
			worker = &tb.Units[i]
		case "hall":
			hall = &tb.Units[i]
		}
	}
	// 1.0 s at 20 ticks/s = 20 ticks; mask gold|lumber = 0b11
	if worker.Harvest.GatherTicks != 20 || worker.Harvest.Capacity != 10 || worker.Harvest.Mask != 0b11 || worker.FoodCost != 1 {
		t.Fatalf("worker econ: %+v food=%d", worker.Harvest, worker.FoodCost)
	}
	if hall.DepotMask != 0b11 || hall.FoodProvided != 10 || hall.Harvest.Capacity != 0 {
		t.Fatalf("hall econ: depot=%b food=%d", hall.DepotMask, hall.FoodProvided)
	}
	t.Logf("worker: gather=%dt cap=%d mask=%02b food-cost=%d", worker.Harvest.GatherTicks, worker.Harvest.Capacity, worker.Harvest.Mask, worker.FoodCost)
	t.Logf("hall: depot=%02b food-provided=%d", hall.DepotMask, hall.FoodProvided)
}

func TestEconomyFailClosed(t *testing.T) {
	cases := []struct {
		name, econ, units, wantErr string
	}{
		{"empty registry", "resource-types = []", "", "resource-types must be non-empty"},
		{"dup type", `resource-types = ["gold","gold"]`, "", "duplicate resource type"},
		{"unknown node resource", "resource-types = [\"gold\"]\n[[node]]\nid = \"x\"\nresource = \"oil\"\namount = 5\n", "", `resource "oil" is not in resource-types`},
		{"zero amount", "resource-types = [\"gold\"]\n[[node]]\nid = \"x\"\nresource = \"gold\"\namount = 0\n", "", "amount 0 out of range"},
		{"dup node id", "resource-types = [\"gold\"]\n[[node]]\nid = \"x\"\nresource = \"gold\"\namount = 5\n[[node]]\nid = \"x\"\nresource = \"gold\"\namount = 5\n", "", "duplicate node id"},
		{"unknown depot resource", econBase, strings.Replace(econUnit, `depot-for = ["gold", "lumber"]`, `depot-for = ["oil"]`, 1), `resource "oil" is not in resource-types`},
		{"unknown harvest resource", econBase, strings.Replace(econUnit, `resources = ["gold", "lumber"]`, `resources = ["oil"]`, 1), `resource "oil" is not in resource-types`},
		{"zero capacity", econBase, strings.Replace(econUnit, "capacity = 10", "capacity = 0", 1), "0 out of range [1, 100000]"},
		{"negative gather", econBase, strings.Replace(econUnit, "gather-seconds = 1.0", "gather-seconds = -1.0", 1), "gather-seconds"},
		{"harvest without registry", "", econUnit, `is not in resource-types`},
		{"unknown key", econBase + "\nbogus = 1\n", "", "bogus"},
	}
	for _, c := range cases {
		_, err := Load(econFS(c.econ, c.units))
		if err == nil {
			t.Errorf("%s: accepted", c.name)
			continue
		}
		if !strings.Contains(err.Error(), c.wantErr) {
			t.Errorf("%s: error %q does not name %q", c.name, err, c.wantErr)
		}
		t.Logf("%s: %v", c.name, err)
	}
}

func TestEconomyFingerprintSensitivity(t *testing.T) {
	base, err := Load(econFS(econBase, econUnit))
	if err != nil {
		t.Fatal(err)
	}
	changed, err := Load(econFS(strings.Replace(econBase, "amount = 12500", "amount = 12501", 1), econUnit))
	if err != nil {
		t.Fatal(err)
	}
	if base.Fingerprint == changed.Fingerprint {
		t.Fatal("node amount change did not move the fingerprint")
	}
	changed2, err := Load(econFS(econBase, strings.Replace(econUnit, "food-cost = 1", "food-cost = 2", 1)))
	if err != nil {
		t.Fatal(err)
	}
	if base.Fingerprint == changed2.Fingerprint {
		t.Fatal("unit food-cost change did not move the fingerprint")
	}
	t.Logf("base=%016x nodeAmount+1=%016x foodCost+1=%016x", base.Fingerprint, changed.Fingerprint, changed2.Fingerprint)
}
