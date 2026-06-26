package luabind

// #312 demo-campaign carry-over FSV (slice 1). SoT = the carry-over actually
// persisted to the campaign D-15 store after the real data/campaigns/demo hooks run
// — read back via BuildMissionView, not the hook's return value — plus its survival
// across a Storage save/load. X+X=Y: seed the store as mission "kindle" victory would
// (Ser Caldus at level 3 holding the Ember Ward), run the real OnMissionComplete
// hook, and the store + mission view must show exactly Ser Caldus level 3 / Ember
// Ward carried into "dawn", with "dawn" unlocked. Edges: a fast clear with no
// recorded level clamps to the demo minimum (2); a loss loops back to "kindle"
// without completing; the carried record survives a save/load round-trip.

import (
	"bytes"
	"os"
	"strings"
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
)

const demoCampaignDir = "../../data/campaigns/demo"

func loadDemoDef(t *testing.T) campaign.Definition {
	t.Helper()
	def, err := campaign.Load(os.DirFS(demoCampaignDir), "demo.toml")
	if err != nil {
		t.Fatalf("load demo.toml: %v", err)
	}
	return def
}

// demoArchives stubs the two mission world archives so BuildMissionView reports an
// unlocked mission as Available (the playable worlds land in slice 2).
func demoArchives() fstest.MapFS {
	return fstest.MapFS{
		"kindle": {Data: []byte("kindle")},
		"dawn":   {Data: []byte("dawn")},
	}
}

func TestDemoCampaignCarryOverFSV(t *testing.T) {
	def := loadDemoDef(t)
	g, err := api.NewGame(api.GameOptions{})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	cat := campaign.StorageCategory(def.ID)

	// Seed the store as a "kindle" victory would: Ser Caldus reached level 3 and
	// holds the Ember Ward. The hook reads these from the shared campaign store.
	g.Storage().SetInt(cat, "demo:caldus:level", 3)
	g.Storage().SetInt(cat, "demo:caldus:ember", 1)

	tr, err := RunCampaignHook(g.Storage(), def, os.DirFS(demoCampaignDir+"/hooks"),
		"demo-complete", "kindle", campaign.OutcomeComplete, CampaignHookOptions{})
	if err != nil {
		t.Fatalf("RunCampaignHook complete: %v", err)
	}
	t.Logf("FSV transition: next=%s carry=%+v log=%v", tr.NextMissionID, tr.CarryOver, tr.HookLog)

	view, err := campaign.BuildMissionView(def, g.Storage(), demoArchives(), "")
	if err != nil {
		t.Fatalf("BuildMissionView: %v", err)
	}
	t.Logf("FSV view: selected=%s kindle=%s dawn=%s carry=%+v",
		view.SelectedMissionID, view.Missions[0].Status, view.Missions[1].Status, view.CarryOver)

	// "kindle" complete unlocks "dawn", which becomes the selected mission.
	if view.Missions[0].Status != campaign.StatusComplete {
		t.Fatalf("kindle not complete: %s", view.Missions[0].Status)
	}
	if view.Missions[1].Status != campaign.StatusAvailable || view.SelectedMissionID != "dawn" {
		t.Fatalf("dawn not unlocked/selected: status=%s selected=%s", view.Missions[1].Status, view.SelectedMissionID)
	}
	// X+X=Y: the carried hero record is exactly what kindle recorded.
	if len(view.CarryOver.Heroes) != 1 {
		t.Fatalf("want 1 carried hero, got %d", len(view.CarryOver.Heroes))
	}
	h := view.CarryOver.Heroes[0]
	if h.Name != "Ser Caldus" || h.Level != 3 || strings.Join(h.Items, ",") != "Ember Ward" {
		t.Fatalf("carried hero wrong: %+v", h)
	}

	// SoT after a save/load: the carried record survives a Storage round-trip
	// (the mid-campaign save/load the M8 criterion requires).
	var buf bytes.Buffer
	if err := g.Storage().Save(&buf); err != nil {
		t.Fatalf("save: %v", err)
	}
	g2, err := api.NewGame(api.GameOptions{})
	if err != nil {
		t.Fatalf("NewGame 2: %v", err)
	}
	if err := g2.Storage().Load(bytes.NewReader(buf.Bytes())); err != nil {
		t.Fatalf("load: %v", err)
	}
	reloaded, err := campaign.BuildMissionView(def, g2.Storage(), demoArchives(), "")
	if err != nil {
		t.Fatalf("BuildMissionView after reload: %v", err)
	}
	t.Logf("FSV reloaded carry=%+v", reloaded.CarryOver)
	if len(reloaded.CarryOver.Heroes) != 1 {
		t.Fatalf("save/load lost carry-over heroes: %+v", reloaded.CarryOver.Heroes)
	}
	if rh := reloaded.CarryOver.Heroes[0]; rh.Name != h.Name || rh.Level != h.Level || strings.Join(rh.Items, ",") != strings.Join(h.Items, ",") {
		t.Fatalf("save/load changed carry-over: %+v != %+v", rh, h)
	}
}

func TestDemoCampaignFastClearClampsLevelFSV(t *testing.T) {
	// A fast clear that recorded no hero level still produces a carried hero at the
	// demo's guaranteed minimum (level 2) — the hook never carries a level-0 hero.
	def := loadDemoDef(t)
	g, _ := api.NewGame(api.GameOptions{})
	if _, err := RunCampaignHook(g.Storage(), def, os.DirFS(demoCampaignDir+"/hooks"),
		"demo-fast", "kindle", campaign.OutcomeComplete, CampaignHookOptions{}); err != nil {
		t.Fatalf("RunCampaignHook: %v", err)
	}
	view, err := campaign.BuildMissionView(def, g.Storage(), demoArchives(), "")
	if err != nil {
		t.Fatalf("view: %v", err)
	}
	h := view.CarryOver.Heroes[0]
	t.Logf("FSV fast-clear carried hero: %+v", h)
	if h.Name != "Ser Caldus" || h.Level != 2 || len(h.Items) != 0 {
		t.Fatalf("fast-clear carry wrong: %+v (want Ser Caldus level 2, no items)", h)
	}
}

func TestDemoCampaignFailLoopsBackFSV(t *testing.T) {
	// A loss runs OnMissionFail: loop back to "kindle", do NOT mark it complete.
	def := loadDemoDef(t)
	g, _ := api.NewGame(api.GameOptions{})
	before, _ := campaign.SnapshotStore(def, g.Storage())
	if _, err := RunCampaignHook(g.Storage(), def, os.DirFS(demoCampaignDir+"/hooks"),
		"demo-fail", "kindle", campaign.OutcomeFail, CampaignHookOptions{}); err != nil {
		t.Fatalf("RunCampaignHook fail: %v", err)
	}
	after, _ := campaign.SnapshotStore(def, g.Storage())
	cat := campaign.StorageCategory(def.ID)
	failed, failedOK := g.Storage().GetBool(cat, "mission:kindle:failed")
	next, nextOK := g.Storage().GetString(cat, "mission:kindle:next")
	t.Logf("FSV fail: before=%s after.kindle.complete=%v failed=(%v,%v) next=(%q,%v)",
		before.Category, after.Missions[0].Complete, failed, failedOK, next, nextOK)
	if !failed || !failedOK || next != "kindle" || !nextOK {
		t.Fatalf("fail did not record retry: failed=(%v,%v) next=(%q,%v)", failed, failedOK, next, nextOK)
	}
	if after.Missions[0].Complete {
		t.Fatalf("fail must not complete kindle: %+v", after.Missions[0])
	}
}
