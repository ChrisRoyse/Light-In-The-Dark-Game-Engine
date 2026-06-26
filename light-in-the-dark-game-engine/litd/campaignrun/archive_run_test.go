package campaignrun_test

// #312 D-14 FSV: the demo campaign plays IDENTICALLY when its missions ship as
// `.litdworld` archives instead of loose directories — the form a versioned,
// distributable campaign actually takes (the operator's "all players get the same
// map when we version the game" requirement). SoT = the threaded campaign store
// after a real archive-sourced run: the same carry chain (Ser Caldus carried at
// the level 3 he earned at the gate, holding the Ember Ward, dawn instantiating him
// at 3) the directory run produces. Mission archives are loaded through the
// hash-verified worldarchive path (worldhost.LoadArchive), so a match proves the
// packaging is lossless AND that campaignrun's archive-vs-directory resolution is
// transparent.

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldpack"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaignrun"
)

// packMission stages one mission directory into the archive layout (Lua under
// scripts/, data tables under data/) and packs it to outPath via the production
// producer. Mirrors scripts/pack-world.sh for a mapless world.
func packMission(t *testing.T, srcDir, outPath string) {
	t.Helper()
	stage := t.TempDir()
	if err := os.MkdirAll(filepath.Join(stage, "scripts"), 0o755); err != nil {
		t.Fatal(err)
	}
	src, err := os.ReadFile(filepath.Join(srcDir, "main.lua"))
	if err != nil {
		t.Fatalf("read mission main.lua: %v", err)
	}
	if err := os.WriteFile(filepath.Join(stage, "scripts", "main.lua"), src, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.CopyFS(filepath.Join(stage, "data"), os.DirFS(filepath.Join(srcDir, "data"))); err != nil {
		t.Fatalf("stage mission data: %v", err)
	}
	if err := worldpack.Pack(stage, outPath, ">=0.1.0 <0.2.0",
		worldpack.Hosting{Author: "Light in the Dark", Title: "First Light mission", Description: "demo campaign mission"},
		nil); err != nil {
		t.Fatalf("pack mission %s: %v", srcDir, err)
	}
}

func TestRunDemoCampaignFromArchivesFSV(t *testing.T) {
	if testing.Short() {
		t.Skip("packs two mission archives + plays both missions; full gate only")
	}

	def, err := campaign.Load(os.DirFS(demoDir), "demo.toml")
	if err != nil {
		t.Fatalf("load demo.toml: %v", err)
	}

	// Pack each mission to <root>/<archive>.litdworld. The manifest names missions
	// bare ("kindle"/"dawn"); with no sibling directory present in this temp root,
	// campaignrun resolves each to its packed archive and loads it verified.
	arcRoot := t.TempDir()
	for _, m := range def.Missions {
		packMission(t, filepath.Join(demoDir, m.Archive), filepath.Join(arcRoot, m.Archive+".litdworld"))
	}

	res, err := campaignrun.Run(def, arcRoot, os.DirFS(demoDir+"/hooks"), campaignrun.Options{
		Seed: 1, PlayerSlot: 0, MaxTicks: 200, EngineVersion: "0.1.0",
	})
	if err != nil {
		t.Fatalf("Run from archives: %v", err)
	}
	for _, m := range res.Missions {
		t.Logf("FSV archive mission %s: result=%v won=%v ticks=%d", m.MissionID, m.Result, m.Won, m.Ticks)
	}
	if !res.Completed || len(res.Missions) != 2 {
		t.Fatalf("archive campaign not completed in 2 missions: %+v", res.Missions)
	}
	if res.Missions[0].MissionID != "kindle" || res.Missions[1].MissionID != "dawn" {
		t.Fatalf("archive mission order wrong: %+v", res.Missions)
	}

	// SoT: the same carry chain as the directory run (TestRunDemoCampaignFSV).
	store := loadStore(t, res.StoreBlob)
	cat := campaign.StorageCategory("demo")
	kindleLvl, _ := store.GetInt(cat, "demo:caldus:level")
	carriedLvl, _ := store.GetInt(cat, "carry:dawn:hero:0:level")
	dawnLvl, okDawn := store.GetInt(cat, "dawn:caldus:level")
	held, _ := store.GetInt(cat, "dawn:held")
	dawnName, _ := store.GetString(cat, "dawn:caldus:name")
	dawnItem, _ := store.GetString(cat, "dawn:caldus:item")
	t.Logf("FSV archive carry chain: kindle=%d carried=%d dawn=(%d,%v) held=%d name=%q item=%q",
		kindleLvl, carriedLvl, dawnLvl, okDawn, held, dawnName, dawnItem)

	if kindleLvl != 3 || carriedLvl != 3 || !okDawn || dawnLvl != 3 {
		t.Fatalf("archive carry chain wrong: kindle=%d carried=%d dawn=%d(ok=%v), want 3/3/3", kindleLvl, carriedLvl, dawnLvl, okDawn)
	}
	if held != 1 {
		t.Fatalf("archive dawn hold not completed: held=%d", held)
	}
	if dawnName != "Ser Caldus" || dawnItem != "Ember Ward" {
		t.Fatalf("archive carried identity wrong: name=%q item=%q, want \"Ser Caldus\"/\"Ember Ward\"", dawnName, dawnItem)
	}
}
