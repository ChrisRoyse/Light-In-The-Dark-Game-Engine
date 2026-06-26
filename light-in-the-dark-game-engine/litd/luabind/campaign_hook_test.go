package luabind

import (
	"strings"
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
)

const campaignHookDefinition = `
id = "vigil-hooks"
title = "Vigil Hook Campaign"
faction = "The Vigil"

[hooks]
on-complete = "OnMissionComplete"
on-fail = "OnMissionFail"

[carry]
heroes = ["Ser Caldus"]
items = ["Ember Ward", "Dawnwater Flask"]
cache-keys = ["checkpoint"]

[[mission]]
id = "m1"
title = "Kindle the Gate"
archive = "worlds/m1.litdworld"

[[mission]]
id = "m2"
title = "Hold the Dawn"
archive = "worlds/m2.litdworld"
requires = ["m1"]
`

func TestCampaignLuaCompleteHookFSV(t *testing.T) {
	def := mustCampaignDef(t)
	g, err := api.NewGame(api.GameOptions{})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	before, err := campaign.SnapshotStore(def, g.Storage())
	if err != nil {
		t.Fatalf("snapshot before: %v", err)
	}
	scripts := hookScripts(`
function OnMissionComplete(ctx)
  local s = Game_Storage()
  Storage_SetString(s, ctx.category, ctx.cachePrefix .. "checkpoint", "inside-the-gate")
  return {
    next = "m2",
    heroes = {
      { name = "Ser Caldus", level = 4, items = { "Ember Ward" } }
    },
    cache = { "checkpoint" },
    log = { "complete:" .. ctx.mission .. "->m2" }
  }
end
function OnMissionFail(ctx)
  return { next = "m1", log = { "fail:" .. ctx.mission } }
end
`)
	tr, err := RunCampaignHook(g.Storage(), def, scripts, "campaign-complete", "m1", campaign.OutcomeComplete, CampaignHookOptions{})
	if err != nil {
		t.Fatalf("RunCampaignHook complete: %v", err)
	}
	after, err := campaign.SnapshotStore(def, g.Storage())
	if err != nil {
		t.Fatalf("snapshot after: %v", err)
	}
	cat := campaign.StorageCategory(def.ID)
	cache, cacheOK := g.Storage().GetString(cat, campaign.CarryCacheKey("m2", "checkpoint"))
	sourceCache, sourceOK := g.Storage().GetString(cat, campaign.CacheSourceKey("checkpoint"))
	logBody, logOK := g.Storage().GetString(cat, "hook:m1:complete:log")
	view, err := campaign.BuildMissionView(def, g.Storage(), fstest.MapFS{
		"worlds/m1.litdworld": {Data: []byte("m1")},
		"worlds/m2.litdworld": {Data: []byte("m2")},
	}, "")
	if err != nil {
		t.Fatalf("mission view after hook: %v", err)
	}
	t.Logf("FSV complete hook BEFORE store=%+v", before)
	t.Logf("FSV complete hook AFTER transition=%+v store=%+v carryCache=(%q,%v) sourceCache=(%q,%v) log=(%q,%v) viewSelected=%s carry=%+v",
		tr, after, cache, cacheOK, sourceCache, sourceOK, logBody, logOK, view.SelectedMissionID, view.CarryOver)
	if !after.Missions[0].Complete || view.Missions[1].Status != campaign.StatusAvailable || view.SelectedMissionID != "m2" {
		t.Fatalf("mission completion did not unlock m2: store=%+v view=%+v", after, view)
	}
	if got := view.CarryOver.Heroes[0]; got.Name != "Ser Caldus" || got.Level != 4 || strings.Join(got.Items, ",") != "Ember Ward" {
		t.Fatalf("carry-over hero wrong: %+v", got)
	}
	if cache != "inside-the-gate" || !cacheOK {
		t.Fatalf("carried cache missing: (%q,%v)", cache, cacheOK)
	}
	if sourceOK {
		t.Fatalf("scratch hook source cache leaked into real store: %q", sourceCache)
	}
	if !strings.Contains(logBody, `item "Dawnwater Flask" skipped`) || !strings.Contains(logBody, "complete:m1->m2") {
		t.Fatalf("hook log missing execution/skipped evidence: %q", logBody)
	}
}

func TestCampaignLuaFailHookFSV(t *testing.T) {
	def := mustCampaignDef(t)
	g, _ := api.NewGame(api.GameOptions{})
	before, _ := campaign.SnapshotStore(def, g.Storage())
	scripts := hookScripts(`
function OnMissionComplete(ctx)
  return { next = "m2", heroes = { { name = "Ser Caldus", level = 1 } } }
end
function OnMissionFail(ctx)
  return { next = "m1", log = { "retry:" .. ctx.mission } }
end
`)
	tr, err := RunCampaignHook(g.Storage(), def, scripts, "campaign-fail", "m1", campaign.OutcomeFail, CampaignHookOptions{})
	if err != nil {
		t.Fatalf("RunCampaignHook fail: %v", err)
	}
	after, _ := campaign.SnapshotStore(def, g.Storage())
	failed, failedOK := g.Storage().GetBool(campaign.StorageCategory(def.ID), "mission:m1:failed")
	next, nextOK := g.Storage().GetString(campaign.StorageCategory(def.ID), "mission:m1:next")
	t.Logf("FSV fail hook BEFORE store=%+v AFTER transition=%+v store=%+v failed=(%v,%v) next=(%q,%v)", before, tr, after, failed, failedOK, next, nextOK)
	if !failed || !failedOK || next != "m1" || !nextOK {
		t.Fatalf("fail hook did not persist retry state: failed=(%v,%v) next=(%q,%v)", failed, failedOK, next, nextOK)
	}
	if after.Missions[0].Complete {
		t.Fatalf("fail hook must not mark mission complete: %+v", after.Missions[0])
	}
}

func TestCampaignLuaHookUnknownMissionFailsClosedFSV(t *testing.T) {
	def := mustCampaignDef(t)
	g, _ := api.NewGame(api.GameOptions{})
	before, _ := campaign.SnapshotStore(def, g.Storage())
	scripts := hookScripts(`
function OnMissionComplete(ctx)
  local s = Game_Storage()
  Storage_SetString(s, ctx.category, ctx.cachePrefix .. "checkpoint", "must-not-leak")
  return {
    next = "missing",
    heroes = {
      { name = "Ser Caldus", level = 4, items = { "Ember Ward" } }
    },
    cache = { "checkpoint" },
    log = { "bad-next" }
  }
end
`)
	tr, err := RunCampaignHook(g.Storage(), def, scripts, "campaign-unknown", "m1", campaign.OutcomeComplete, CampaignHookOptions{})
	after, _ := campaign.SnapshotStore(def, g.Storage())
	sourceCache, sourceOK := g.Storage().GetString(campaign.StorageCategory(def.ID), campaign.CacheSourceKey("checkpoint"))
	t.Logf("FSV unknown mission edge BEFORE store=%+v AFTER transition=%+v err=%v store=%+v sourceCache=(%q,%v)", before, tr, err, after, sourceCache, sourceOK)
	if err == nil || !strings.Contains(err.Error(), "unknown mission") {
		t.Fatalf("unknown next mission should fail clearly, got %v", err)
	}
	if after.SHA256 != before.SHA256 || sourceOK {
		t.Fatalf("unknown mission mutated real store: before=%+v after=%+v sourceCache=(%q,%v)", before, after, sourceCache, sourceOK)
	}
}

func TestCampaignLuaHookSandboxViolationFailsClosedFSV(t *testing.T) {
	def := mustCampaignDef(t)
	g, _ := api.NewGame(api.GameOptions{})
	before, _ := campaign.SnapshotStore(def, g.Storage())
	scripts := hookScripts(`
function OnMissionComplete(ctx)
  os.execute("echo no")
  return { next = "m2" }
end
`)
	tr, err := RunCampaignHook(g.Storage(), def, scripts, "campaign-sandbox", "m1", campaign.OutcomeComplete, CampaignHookOptions{})
	after, _ := campaign.SnapshotStore(def, g.Storage())
	t.Logf("FSV sandbox edge BEFORE store=%+v AFTER transition=%+v err=%v store=%+v", before, tr, err, after)
	if err == nil || !strings.Contains(err.Error(), "OnMissionComplete") {
		t.Fatalf("sandbox violation should fail through hook error, got %v", err)
	}
	if after.SHA256 != before.SHA256 {
		t.Fatalf("sandbox violation mutated real store: before=%+v after=%+v", before, after)
	}
}

func mustCampaignDef(t *testing.T) campaign.Definition {
	t.Helper()
	def, err := campaign.ReadDefinition("campaign-hooks.toml", []byte(campaignHookDefinition))
	if err != nil {
		t.Fatal(err)
	}
	return def
}

func hookScripts(src string) fstest.MapFS {
	return fstest.MapFS{"main.lua": {Data: []byte(src)}}
}
