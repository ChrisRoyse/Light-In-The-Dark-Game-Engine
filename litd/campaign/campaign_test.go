package campaign

import (
	"bytes"
	"strings"
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
)

const twoMissionCampaignTOML = `
id = "vigil-test"
title = "Vigil Test Campaign"
faction = "The Vigil"

[[mission]]
id = "m1"
title = "Kindle the Gate"
summary = "Secure the first beacon."
archive = "worlds/m1.litdworld"

[[mission]]
id = "m2"
title = "Hold the Dawn"
summary = "Carry the hero into the counterattack."
archive = "worlds/m2.litdworld"
requires = ["m1"]
`

func TestCampaignUnlockCarryOverFSV(t *testing.T) {
	def := loadFixtureDefinition(t)
	archives := completeArchives()
	g, err := api.NewGame(api.GameOptions{})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}

	before, err := BuildMissionView(def, g.Storage(), archives, "")
	if err != nil {
		t.Fatalf("view before: %v", err)
	}
	beforeStore, err := SnapshotStore(def, g.Storage())
	if err != nil {
		t.Fatalf("store before: %v", err)
	}
	t.Logf("FSV fresh BEFORE store=%+v AFTER view selected=%s statuses=%s/%s",
		beforeStore, before.SelectedMissionID, before.Missions[0].Status, before.Missions[1].Status)
	if before.Missions[0].Status != StatusAvailable || before.Missions[1].Status != StatusLocked || before.SelectedMissionID != "m1" {
		t.Fatalf("fresh unlock state wrong: %+v", before)
	}

	carry := CarryOver{MissionID: "m2", Heroes: []HeroCarryOver{{
		Name:  "Ser Caldus",
		Level: 4,
		Items: []string{"Ember Ward", "Dawnwater Flask"},
	}}}
	if err := CompleteMission(g.Storage(), def, "m1", carry); err != nil {
		t.Fatalf("complete: %v", err)
	}
	after, err := BuildMissionView(def, g.Storage(), archives, "")
	if err != nil {
		t.Fatalf("view after: %v", err)
	}
	afterStore, err := SnapshotStore(def, g.Storage())
	if err != nil {
		t.Fatalf("store after: %v", err)
	}
	t.Logf("FSV win m1 BEFORE storeHash=%s AFTER store=%+v view selected=%s statuses=%s/%s carry=%+v",
		beforeStore.SHA256, afterStore, after.SelectedMissionID, after.Missions[0].Status, after.Missions[1].Status, after.CarryOver)
	if after.Missions[0].Status != StatusComplete || after.Missions[1].Status != StatusAvailable || after.SelectedMissionID != "m2" {
		t.Fatalf("post-win unlock state wrong: %+v", after)
	}
	if got := after.CarryOver.Heroes[0]; got.Name != "Ser Caldus" || got.Level != 4 || strings.Join(got.Items, ",") != "Ember Ward,Dawnwater Flask" {
		t.Fatalf("carry-over panel source wrong: %+v", got)
	}

	var buf bytes.Buffer
	if err := g.Storage().Save(&buf); err != nil {
		t.Fatalf("save: %v", err)
	}
	g2, err := api.NewGame(api.GameOptions{})
	if err != nil {
		t.Fatalf("NewGame reload: %v", err)
	}
	if err := g2.Storage().Load(bytes.NewReader(buf.Bytes())); err != nil {
		t.Fatalf("load: %v", err)
	}
	loaded, err := BuildMissionView(def, g2.Storage(), archives, "")
	if err != nil {
		t.Fatalf("view loaded: %v", err)
	}
	loadedStore, err := SnapshotStore(def, g2.Storage())
	if err != nil {
		t.Fatalf("store loaded: %v", err)
	}
	t.Logf("FSV save/load AFTER bytes=%d hash=%s loadedStore=%+v loadedCarry=%+v", buf.Len(), loadedStore.SHA256, loadedStore, loaded.CarryOver)
	if loadedStore.SHA256 != afterStore.SHA256 || loaded.Missions[1].Status != StatusAvailable || loaded.CarryOver.Heroes[0].Name != "Ser Caldus" {
		t.Fatalf("save/load lost campaign progress: store=%+v view=%+v", loadedStore, loaded)
	}
}

func TestCampaignMidGameSaveLoadThenWinFSV(t *testing.T) {
	def := loadFixtureDefinition(t)
	g, _ := api.NewGame(api.GameOptions{})
	g.Storage().SetString("campaign:"+def.ID, "mission:m1:checkpoint", "inside-the-gate")
	before, _ := SnapshotStore(def, g.Storage())

	var buf bytes.Buffer
	if err := g.Storage().Save(&buf); err != nil {
		t.Fatalf("save checkpoint: %v", err)
	}
	reloaded, _ := api.NewGame(api.GameOptions{})
	if err := reloaded.Storage().Load(bytes.NewReader(buf.Bytes())); err != nil {
		t.Fatalf("load checkpoint: %v", err)
	}
	checkpoint, ok := reloaded.Storage().GetString("campaign:"+def.ID, "mission:m1:checkpoint")
	t.Logf("FSV mid-game save/load BEFORE store=%+v AFTER checkpoint=(%q,%v)", before, checkpoint, ok)
	if checkpoint != "inside-the-gate" || !ok {
		t.Fatalf("checkpoint missing after save/load: %q %v", checkpoint, ok)
	}

	if err := CompleteMission(reloaded.Storage(), def, "m1", CarryOver{MissionID: "m2", Heroes: []HeroCarryOver{{Name: "Mira Vale", Level: 2, Items: []string{"Signal Charm"}}}}); err != nil {
		t.Fatalf("complete after load: %v", err)
	}
	view, err := BuildMissionView(def, reloaded.Storage(), completeArchives(), "")
	if err != nil {
		t.Fatalf("view after loaded win: %v", err)
	}
	after, _ := SnapshotStore(def, reloaded.Storage())
	t.Logf("FSV loaded-then-win AFTER store=%+v selected=%s carry=%+v", after, view.SelectedMissionID, view.CarryOver)
	if view.Missions[1].Status != StatusAvailable || view.CarryOver.Heroes[0].Name != "Mira Vale" {
		t.Fatalf("loaded-then-win did not unlock/carry: %+v", view)
	}
}

func TestCampaignMissingArchiveErrorFSV(t *testing.T) {
	def := loadFixtureDefinition(t)
	g, _ := api.NewGame(api.GameOptions{})
	view, err := BuildMissionView(def, g.Storage(), fstest.MapFS{"worlds/m2.litdworld": {Data: []byte("m2")}}, "")
	if err != nil {
		t.Fatalf("view missing archive: %v", err)
	}
	store, _ := SnapshotStore(def, g.Storage())
	t.Logf("FSV missing archive BEFORE store=%+v AFTER selected=%s status=%s err=%q", store, view.SelectedMissionID, view.Missions[0].Status, view.Missions[0].Error)
	if view.Missions[0].Status != StatusMissingArchive || !strings.Contains(view.Missions[0].Error, "worlds/m1.litdworld") {
		t.Fatalf("missing archive not surfaced clearly: %+v", view.Missions[0])
	}
	if view.Missions[1].Status != StatusLocked {
		t.Fatalf("locked mission should remain locked while m1 missing: %+v", view.Missions[1])
	}
}

func TestCampaignCompleteMissionInvalidCarryFailsClosedFSV(t *testing.T) {
	def := loadFixtureDefinition(t)
	g, _ := api.NewGame(api.GameOptions{})
	before, err := SnapshotStore(def, g.Storage())
	if err != nil {
		t.Fatalf("snapshot before: %v", err)
	}
	err = CompleteMission(g.Storage(), def, "m1", CarryOver{MissionID: "m2", Heroes: []HeroCarryOver{{Name: " ", Level: 4}}})
	after, snapErr := SnapshotStore(def, g.Storage())
	if snapErr != nil {
		t.Fatalf("snapshot after: %v", snapErr)
	}
	t.Logf("FSV invalid carry BEFORE store=%+v AFTER err=%v store=%+v", before, err, after)
	if err == nil || !strings.Contains(err.Error(), "empty name") {
		t.Fatalf("invalid carry did not reject clearly: %v", err)
	}
	if after.Missions[0].CompletePresent || after.Missions[0].Complete || len(after.Carry) != 0 || after.SHA256 != before.SHA256 {
		t.Fatalf("invalid carry mutated store: before=%+v after=%+v", before, after)
	}
}

func TestCampaignCatalogAndValidationFSV(t *testing.T) {
	fsys := fstest.MapFS{
		"campaigns/a.toml": {Data: []byte(twoMissionCampaignTOML)},
		"campaigns/b.toml": {Data: []byte(strings.ReplaceAll(twoMissionCampaignTOML, "vigil-test", "unbound-test"))},
	}
	defs, err := LoadCatalog(fsys, "campaigns")
	if err != nil {
		t.Fatalf("catalog load: %v", err)
	}
	g, _ := api.NewGame(api.GameOptions{})
	if err := CompleteMission(g.Storage(), defs[0], "m1", CarryOver{}); err != nil {
		t.Fatalf("complete catalog mission: %v", err)
	}
	catalog, err := BuildCatalogView(defs, g.Storage(), defs[0].ID)
	if err != nil {
		t.Fatalf("catalog view: %v", err)
	}
	t.Logf("FSV catalog AFTER selected=%s campaigns=%+v", catalog.SelectedCampaignID, catalog.Campaigns)
	if len(catalog.Campaigns) != 2 || catalog.Campaigns[0].CompletedMissions != 1 {
		t.Fatalf("catalog view wrong: %+v", catalog)
	}

	edges := []struct {
		name string
		body string
		want string
	}{
		{name: "empty", body: "", want: "campaign id"},
		{name: "duplicate", body: twoMissionCampaignTOML + "\n[[mission]]\nid = \"m1\"\ntitle = \"Again\"\narchive = \"worlds/x.litdworld\"\n", want: "duplicated"},
		{name: "unknown-requirement", body: strings.Replace(twoMissionCampaignTOML, `requires = ["m1"]`, `requires = ["missing"]`, 1), want: "unknown mission"},
		{name: "bad-archive", body: strings.Replace(twoMissionCampaignTOML, `archive = "worlds/m1.litdworld"`, `archive = "../m1.litdworld"`, 1), want: "clean relative"},
	}
	for _, tc := range edges {
		if _, err := ReadDefinition(tc.name+".toml", []byte(tc.body)); err == nil || !strings.Contains(err.Error(), tc.want) {
			t.Fatalf("%s validation err=%v, want substring %q", tc.name, err, tc.want)
		} else {
			t.Logf("FSV validation edge=%s BEFORE invalid AFTER err=%v", tc.name, err)
		}
	}
}

func loadFixtureDefinition(t *testing.T) Definition {
	t.Helper()
	def, err := ReadDefinition("test-campaign.toml", []byte(twoMissionCampaignTOML))
	if err != nil {
		t.Fatal(err)
	}
	return def
}

func completeArchives() fstest.MapFS {
	return fstest.MapFS{
		"worlds/m1.litdworld": {Data: []byte("m1")},
		"worlds/m2.litdworld": {Data: []byte("m2")},
	}
}
