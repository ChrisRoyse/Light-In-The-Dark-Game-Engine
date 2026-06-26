package luabind_test

// #312 slice 2 — the full two-mission carry chain into the second playable world.
// SoT = the dawn mission's store after it actually loads and instantiates the
// carried hero. The carried LEVEL is not hand-picked: it is read from the carry
// the kindle playthrough + on-complete hook actually committed, then handed to the
// dawn world the way a campaign runner would (worldhost.Load builds a fresh store,
// so the runner's job — copy the committed carry into the next mission's store —
// is emulated by copying the real committed values across). X+X=Y: kindle earns
// Caldus level 3; dawn must instantiate a level-3 Ser Caldus and win the hold.

import (
	"os"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

func TestDemoTwoMissionCarryChainFSV(t *testing.T) {
	cat := campaign.StorageCategory("demo")
	def, err := campaign.Load(os.DirFS(demoDir), "demo.toml")
	if err != nil {
		t.Fatalf("load demo.toml: %v", err)
	}

	// --- Mission 1: play kindle, then commit the on-complete carry. ---
	k, err := worldhost.Load(demoDir+"/kindle", 1, 50_000_000)
	if err != nil {
		t.Fatalf("load kindle: %v", err)
	}
	defer k.Close()
	k.Game.Advance(25)
	if _, err := luabind.RunCampaignHook(k.Game.Storage(), def, os.DirFS(demoDir+"/hooks"),
		"chain", "kindle", campaign.OutcomeComplete, luabind.CampaignHookOptions{}); err != nil {
		t.Fatalf("RunCampaignHook: %v", err)
	}
	// The hook committed the carry for "dawn" into kindle's store. Read the real
	// committed values (NOT hardcoded) that the runner would hand to dawn.
	carriedLevel, okLvl := k.Game.Storage().GetInt(cat, "carry:dawn:hero:0:level")
	carriedCount, okCnt := k.Game.Storage().GetInt(cat, "carry:dawn:hero-count")
	carriedItems, okItm := k.Game.Storage().GetInt(cat, "carry:dawn:hero:0:item-count")
	t.Logf("FSV committed carry: heroes=(%d,%v) level=(%d,%v) items=(%d,%v)",
		carriedCount, okCnt, carriedLevel, okLvl, carriedItems, okItm)
	if !okLvl || carriedLevel != 3 || !okCnt || carriedCount != 1 || !okItm || carriedItems != 1 {
		t.Fatalf("committed carry wrong: heroes=%d level=%d items=%d", carriedCount, carriedLevel, carriedItems)
	}

	// --- Mission 2: load dawn and hand it the committed carry. ---
	d, err := worldhost.Load(demoDir+"/dawn", 1, 50_000_000)
	if err != nil {
		t.Fatalf("load dawn: %v", err)
	}
	defer d.Close()
	// Emulate the runner's hand-off: copy the committed carry into dawn's store
	// BEFORE the first tick (dawn reads it in tick 1 and instantiates the hero).
	d.Game.Storage().SetInt(cat, "carry:dawn:hero-count", carriedCount)
	d.Game.Storage().SetInt(cat, "carry:dawn:hero:0:level", carriedLevel)
	d.Game.Storage().SetInt(cat, "carry:dawn:hero:0:item-count", carriedItems)

	// Step dawn: tick 1 spawns Caldus at the carried level; the hold completes
	// after HOLD_TICKS (15) and declares victory.
	d.Game.Advance(20)
	spawnedLevel, okSpawn := d.Game.Storage().GetInt(cat, "dawn:caldus:level")
	carried, _ := d.Game.Storage().GetInt(cat, "dawn:carried-heroes")
	held, _ := d.Game.Storage().GetInt(cat, "dawn:held")
	res := d.Game.Player(0).Result()
	t.Logf("FSV dawn: instantiated Caldus level=(%d,%v) carried-heroes=%d held=%d result=%v",
		spawnedLevel, okSpawn, carried, held, res)

	// X+X=Y: kindle earned level 3 -> dawn instantiates a level-3 hero.
	if !okSpawn || spawnedLevel != carriedLevel {
		t.Fatalf("dawn instantiated Caldus at level %d, want carried %d", spawnedLevel, carriedLevel)
	}
	if carried != carriedCount {
		t.Fatalf("dawn saw %d carried heroes, want %d", carried, carriedCount)
	}
	if held != 1 || res != api.ResultWon {
		t.Fatalf("dawn hold not won: held=%d result=%v", held, res)
	}
}

func TestDemoDawnInstantiatesCarriedLevelDataDrivenFSV(t *testing.T) {
	// #312 FSV edge 3: "a different playthrough → mission 2 reflects the different
	// record." TestDemoTwoMissionCarryChainFSV always carries level 3, so a dawn
	// that hardcoded "3" (or clamped to it) would pass it. This hands dawn DIFFERENT
	// carried levels and asserts the instantiated hero matches each — proving the
	// carried level is data-driven, not baked. SoT = dawn's own store after stepping.
	cat := campaign.StorageCategory("demo")
	for _, level := range []int{2, 5} {
		d, err := worldhost.Load(demoDir+"/dawn", 1, 50_000_000)
		if err != nil {
			t.Fatalf("level %d: load dawn: %v", level, err)
		}
		// Hand dawn a synthetic carry at this level (the runner's hand-off seam).
		d.Game.Storage().SetInt(cat, "carry:dawn:hero-count", 1)
		d.Game.Storage().SetInt(cat, "carry:dawn:hero:0:level", level)
		d.Game.Storage().SetInt(cat, "carry:dawn:hero:0:item-count", 1)
		d.Game.Advance(20)
		got, ok := d.Game.Storage().GetInt(cat, "dawn:caldus:level")
		carried, _ := d.Game.Storage().GetInt(cat, "dawn:carried-heroes")
		t.Logf("FSV edge3: carried level %d → dawn instantiated level=(%d,%v) carried-heroes=%d", level, got, ok, carried)
		if !ok || got != level {
			t.Fatalf("dawn instantiated Caldus at level %d, want carried %d — carry is not data-driven", got, level)
		}
		if carried != 1 {
			t.Fatalf("level %d: dawn saw %d carried heroes, want 1", level, carried)
		}
		d.Close()
	}
}

func TestDemoDawnStandaloneLevelOneFSV(t *testing.T) {
	// Fail-safe: loaded with no carry committed, dawn still works — Caldus arrives
	// at level 1 (never a level-0 / no hero). SoT = dawn's own store.
	cat := campaign.StorageCategory("demo")
	d, err := worldhost.Load(demoDir+"/dawn", 1, 50_000_000)
	if err != nil {
		t.Fatalf("load dawn: %v", err)
	}
	defer d.Close()
	d.Game.Advance(20)
	lvl, ok := d.Game.Storage().GetInt(cat, "dawn:caldus:level")
	carried, _ := d.Game.Storage().GetInt(cat, "dawn:carried-heroes")
	t.Logf("FSV dawn standalone: level=(%d,%v) carried-heroes=%d result=%v", lvl, ok, carried, d.Game.Player(0).Result())
	if !ok || lvl != 1 {
		t.Fatalf("standalone dawn Caldus level=%d, want 1", lvl)
	}
	if carried != 0 {
		t.Fatalf("standalone dawn saw %d carried heroes, want 0", carried)
	}
}
