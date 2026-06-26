package hud

import (
	"os"
	"strings"
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/locale"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
)

const campaignMenuFixture = `
id = "vigil-ui"
title = "Vigil UI Campaign"
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

func TestCampaignMenuFreshAndCatalogLayoutFSV(t *testing.T) {
	canvas, _ := NewCanvas(1366, 768, 1)
	def := readCampaignMenuFixture(t)
	g, _ := api.NewGame(api.GameOptions{})
	catalogView, err := campaign.BuildCatalogView([]campaign.Definition{def}, g.Storage(), def.ID)
	if err != nil {
		t.Fatalf("catalog view: %v", err)
	}
	labels := CampaignMenuStringsFromLocale(loadCampaignLocale(t, "en"))
	catalog := NewCampaignSelectLayout(canvas, catalogView, labels)
	t.Logf("FSV campaign-select layout widgets=%d labels=%d issues=%v labels=%+v", len(catalog.Widgets), len(catalog.Labels), catalog.Issues, catalog.Labels)
	if len(catalog.Issues) != 0 || catalog.SelectedCampaignID != def.ID || !campaignLayoutHasText(catalog, labels.CampaignSelect) {
		t.Fatalf("campaign select layout wrong: %+v", catalog)
	}

	view, err := campaign.BuildMissionView(def, g.Storage(), campaignMenuArchives(), "")
	if err != nil {
		t.Fatalf("mission view: %v", err)
	}
	mission := NewMissionSelectLayout(canvas, view, labels)
	t.Logf("FSV mission fresh layout selected=%s statuses=%s/%s labels=%+v issues=%v",
		mission.SelectedMissionID, view.Missions[0].Status, view.Missions[1].Status, mission.Labels, mission.Issues)
	if len(mission.Issues) != 0 || mission.SelectedMissionID != "m1" || !campaignLayoutHasText(mission, labels.Available) || !campaignLayoutHasText(mission, labels.Locked) {
		t.Fatalf("fresh mission layout wrong: %+v", mission)
	}
}

func TestCampaignMenuUnlockedCarryOverLayoutFSV(t *testing.T) {
	canvas, _ := NewCanvas(1920, 1080, 1)
	def := readCampaignMenuFixture(t)
	g, _ := api.NewGame(api.GameOptions{})
	if err := campaign.CompleteMission(g.Storage(), def, "m1", campaign.CarryOver{MissionID: "m2", Heroes: []campaign.HeroCarryOver{{
		Name:  "Ser Caldus",
		Level: 4,
		Items: []string{"Ember Ward", "Dawnwater Flask"},
	}}}); err != nil {
		t.Fatalf("complete: %v", err)
	}
	view, err := campaign.BuildMissionView(def, g.Storage(), campaignMenuArchives(), "")
	if err != nil {
		t.Fatalf("mission view: %v", err)
	}
	labels := CampaignMenuStringsFromLocale(loadCampaignLocale(t, "en"))
	layout := NewMissionSelectLayout(canvas, view, labels)
	t.Logf("FSV mission unlocked layout selected=%s carry=%+v labels=%+v issues=%v", layout.SelectedMissionID, view.CarryOver, layout.Labels, layout.Issues)
	if len(layout.Issues) != 0 || layout.SelectedMissionID != "m2" || !campaignLayoutHasText(layout, "Ser Caldus") || !campaignLayoutHasText(layout, "Ember Ward") {
		t.Fatalf("unlocked carry-over layout wrong: %+v", layout)
	}
}

func TestCampaignMenuMissingArchiveAndLocaleFSV(t *testing.T) {
	canvas, _ := NewCanvas(1366, 768, 1)
	def := readCampaignMenuFixture(t)
	g, _ := api.NewGame(api.GameOptions{})
	view, err := campaign.BuildMissionView(def, g.Storage(), fstest.MapFS{"worlds/m2.litdworld": {Data: []byte("m2")}}, "")
	if err != nil {
		t.Fatalf("missing archive view: %v", err)
	}
	en := NewMissionSelectLayout(canvas, view, CampaignMenuStringsFromLocale(loadCampaignLocale(t, "en")))
	t.Logf("FSV mission missing archive layout status=%s labels=%+v issues=%v", view.Missions[0].Status, en.Labels, en.Issues)
	if len(en.Issues) != 0 || !campaignLayoutHasText(en, "Missing archive") || !campaignLayoutHasText(en, "worlds/m1.litdworld") {
		t.Fatalf("missing archive layout wrong: %+v", en)
	}

	xx := NewMissionSelectLayout(canvas, view, CampaignMenuStringsFromLocale(loadCampaignLocale(t, "xx")))
	t.Logf("FSV pseudo-locale campaign menu labels=%+v", xx.Labels)
	if !campaignLayoutHasText(xx, "[xx.94]") || !campaignLayoutHasText(xx, "[xx.99]") {
		t.Fatalf("pseudo-locale labels were not used: %+v", xx.Labels)
	}
}

func readCampaignMenuFixture(t *testing.T) campaign.Definition {
	t.Helper()
	def, err := campaign.ReadDefinition("campaign-menu.toml", []byte(campaignMenuFixture))
	if err != nil {
		t.Fatal(err)
	}
	return def
}

func campaignMenuArchives() fstest.MapFS {
	return fstest.MapFS{
		"worlds/m1.litdworld": {Data: []byte("m1")},
		"worlds/m2.litdworld": {Data: []byte("m2")},
	}
}

func loadCampaignLocale(t *testing.T, tag string) *locale.Table {
	t.Helper()
	table, err := locale.Load(os.DirFS("../../../data"), tag)
	if err != nil {
		t.Fatal(err)
	}
	return table
}

func campaignLayoutHasText(layout CampaignMenuLayout, want string) bool {
	for _, label := range layout.Labels {
		if strings.Contains(label.Text, want) {
			return true
		}
	}
	return false
}
