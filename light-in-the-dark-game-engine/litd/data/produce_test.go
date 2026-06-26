package data

// #302 production-table tests. SoT = converted Unit production fields
// (Costs / TrainTicks / Trains) and the named refusal errors.

import (
	"strings"
	"testing"
)

const prodUnit = `
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
train-seconds = 15.0
[unit.costs]
gold = 75
lumber = 10

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
trains = ["worker"]
`

func TestProductionHappyPath(t *testing.T) {
	tb, err := Load(econFS(econBase, prodUnit))
	if err != nil {
		t.Fatal(err)
	}
	var worker, hall *Unit
	var workerIdx int
	for i := range tb.Units {
		switch tb.Units[i].ID {
		case "worker":
			worker = &tb.Units[i]
			workerIdx = i
		case "hall":
			hall = &tb.Units[i]
		}
	}
	// 15 s at 20 ticks/s = 300 ticks; costs indexed by registry order
	if worker.TrainTicks != 300 {
		t.Fatalf("worker train ticks = %d, want 300", worker.TrainTicks)
	}
	if len(worker.Costs) != 2 || worker.Costs[0] != 75 || worker.Costs[1] != 10 {
		t.Fatalf("worker costs = %v, want [75 10]", worker.Costs)
	}
	if len(hall.Trains) != 1 || int(hall.Trains[0]) != workerIdx {
		t.Fatalf("hall trains = %v, want [%d]", hall.Trains, workerIdx)
	}
	if hall.TrainTicks != 0 || hall.Costs != nil {
		t.Fatalf("hall production block leaked: ticks=%d costs=%v", hall.TrainTicks, hall.Costs)
	}
	t.Logf("worker: train=%dt costs=%v (idx %d)", worker.TrainTicks, worker.Costs, workerIdx)
	t.Logf("hall: trains=%v", hall.Trains)
}

func TestProductionFailClosed(t *testing.T) {
	cases := []struct {
		name, units, wantErr string
	}{
		{"unknown cost resource", strings.Replace(prodUnit, "gold = 75", "oil = 75", 1), `resource "oil" is not in resource-types`},
		{"negative cost", strings.Replace(prodUnit, "gold = 75", "gold = -1", 1), "out of range [0, 1e6]"},
		{"cost over cap", strings.Replace(prodUnit, "gold = 75", "gold = 1000001", 1), "out of range [0, 1e6]"},
		{"negative train time", strings.Replace(prodUnit, "train-seconds = 15.0", "train-seconds = -1.0", 1), "train-seconds"},
		{"trains undefined unit", strings.Replace(prodUnit, `trains = ["worker"]`, `trains = ["ghost"]`, 1), `trains reference to undefined unit "ghost"`},
		{"trains untrainable unit", strings.Replace(prodUnit, "train-seconds = 15.0\n", "", 1), `trains "worker" which has no train-seconds`},
	}
	for _, c := range cases {
		_, err := Load(econFS(econBase, c.units))
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

func TestProductionFingerprintSensitivity(t *testing.T) {
	base, err := Load(econFS(econBase, prodUnit))
	if err != nil {
		t.Fatal(err)
	}
	for name, mutated := range map[string]string{
		"cost":          strings.Replace(prodUnit, "gold = 75", "gold = 76", 1),
		"train-seconds": strings.Replace(prodUnit, "train-seconds = 15.0", "train-seconds = 15.05", 1),
		"trains":        strings.Replace(prodUnit, `trains = ["worker"]`, "", 1),
	} {
		changed, err := Load(econFS(econBase, mutated))
		if err != nil {
			t.Fatalf("%s: %v", name, err)
		}
		if base.Fingerprint == changed.Fingerprint {
			t.Errorf("%s change did not move the fingerprint", name)
		}
		t.Logf("%s: base=%016x changed=%016x", name, base.Fingerprint, changed.Fingerprint)
	}
}
