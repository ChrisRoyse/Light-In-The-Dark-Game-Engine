package campaignrun_test

// #534 FSV. SoT = the campaign store the runner threads end to end, plus each
// mission's recorded result — read back AFTER a real run of the demo campaign, not
// the runner's return value alone. X+X=Y: playing "First Light" must win both
// missions in order and the final store must show Ser Caldus carried at the level
// he earned at the gate (3), holding the Ember Ward, with dawn instantiating him at
// that level. This is the runtime orchestration #312's demo needed: no test
// hand-wiring of the carry, the runner does it.

import (
	"bytes"
	"os"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaignrun"
)

const demoDir = "../../data/campaigns/demo"

// loadStore reconstructs an api.Storage from a serialized blob so the test can read
// the runner's final threaded campaign store directly (SoT).
func loadStore(t *testing.T, blob []byte) *api.Storage {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.Storage().Load(bytes.NewReader(blob)); err != nil {
		t.Fatalf("load store blob: %v", err)
	}
	return g.Storage()
}

func TestRunDemoCampaignFSV(t *testing.T) {
	def, err := campaign.Load(os.DirFS(demoDir), "demo.toml")
	if err != nil {
		t.Fatalf("load demo.toml: %v", err)
	}

	res, err := campaignrun.Run(def, demoDir, os.DirFS(demoDir+"/hooks"), campaignrun.Options{
		Seed: 1, PlayerSlot: 0, MaxTicks: 200,
	})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	for _, m := range res.Missions {
		t.Logf("FSV mission %s: result=%v won=%v ticks=%d", m.MissionID, m.Result, m.Won, m.Ticks)
	}

	// Both missions played, in order, and won.
	if !res.Completed {
		t.Fatalf("campaign not completed: %+v", res.Missions)
	}
	if len(res.Missions) != 2 || res.Missions[0].MissionID != "kindle" || res.Missions[1].MissionID != "dawn" {
		t.Fatalf("mission order wrong: %+v", res.Missions)
	}
	if !res.Missions[0].Won || !res.Missions[1].Won {
		t.Fatalf("a mission was not won: %+v", res.Missions)
	}

	// SoT: re-read the FINAL threaded store and prove the carry chain landed.
	store := loadStore(t, res.StoreBlob)
	cat := campaign.StorageCategory("demo")
	kindleLvl, _ := store.GetInt(cat, "demo:caldus:level")
	carriedLvl, _ := store.GetInt(cat, "carry:dawn:hero:0:level")
	dawnLvl, okDawn := store.GetInt(cat, "dawn:caldus:level")
	held, _ := store.GetInt(cat, "dawn:held")
	// dawn reconstructs the carried hero's identity from the string carry keys.
	dawnName, _ := store.GetString(cat, "dawn:caldus:name")
	dawnItem, _ := store.GetString(cat, "dawn:caldus:item")
	t.Logf("FSV final store: kindle-earned=%d carried-to-dawn=%d dawn-instantiated=(%d,%v) held=%d name=%q item=%q",
		kindleLvl, carriedLvl, dawnLvl, okDawn, held, dawnName, dawnItem)

	if kindleLvl != 3 {
		t.Fatalf("kindle did not record Caldus level 3: %d", kindleLvl)
	}
	if carriedLvl != 3 {
		t.Fatalf("carry to dawn level = %d, want 3", carriedLvl)
	}
	if !okDawn || dawnLvl != 3 {
		t.Fatalf("dawn instantiated Caldus at %d, want carried 3", dawnLvl)
	}
	if held != 1 {
		t.Fatalf("dawn hold not completed: held=%d", held)
	}
	// The string carry leg: dawn read the carried hero name + item back from Lua.
	if dawnName != "Ser Caldus" {
		t.Fatalf("dawn reconstructed hero name = %q, want Ser Caldus", dawnName)
	}
	if dawnItem != "Ember Ward" {
		t.Fatalf("dawn reconstructed carried item = %q, want Ember Ward", dawnItem)
	}
}

func TestRunStopsAtFirstLossFSV(t *testing.T) {
	// A mission the player cannot win must stop the run (Completed=false) and not
	// play later missions. We force a loss by capping ticks below kindle's gate
	// (20), so kindle never wins.
	def, err := campaign.Load(os.DirFS(demoDir), "demo.toml")
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	res, err := campaignrun.Run(def, demoDir, os.DirFS(demoDir+"/hooks"), campaignrun.Options{
		Seed: 1, PlayerSlot: 0, MaxTicks: 10, // below kindle's 20-tick gate
	})
	if err != nil {
		t.Fatalf("Run: %v", err)
	}
	t.Logf("FSV capped run: completed=%v missions=%+v", res.Completed, res.Missions)
	if res.Completed {
		t.Fatal("run reported completed despite kindle never winning")
	}
	if len(res.Missions) != 1 || res.Missions[0].MissionID != "kindle" || res.Missions[0].Won {
		t.Fatalf("expected only an unwon kindle, got %+v", res.Missions)
	}
}
