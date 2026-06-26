package luabind_test

// #312 slice 2 — the REAL kindle playthrough drives the carry-over (no seeding).
// SoT = the campaign store after actually loading and stepping the kindle mission
// world: Ser Caldus is spawned as a hero (#532), promoted to level 3, granted the
// Ember Ward, and his live record is persisted — then the campaign on-complete
// hook reads THAT record (not a hand-seeded one) and carries it to "dawn".
// X+X=Y: step the mission, the store must hold demo:caldus:level=3 / ember=1 and
// player 0 must have Won; feed that store to RunCampaignHook and the carried hero
// must be Ser Caldus at level 3 holding the Ember Ward.
//
// External test package: worldhost imports luabind, so loading a world from a
// luabind-internal test would cycle. This drives the public seams instead.

import (
	"os"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/campaign"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/worldhost"
)

const demoDir = "../../data/campaigns/demo"

func TestDemoKindlePlaythroughCarryFSV(t *testing.T) {
	host, err := worldhost.Load(demoDir+"/kindle", 1, 50_000_000)
	if err != nil {
		t.Fatalf("load kindle world: %v", err)
	}
	defer host.Close()
	g := host.Game
	cat := campaign.StorageCategory("demo") // "campaign:demo"

	// BEFORE the gate fires (t<20): no record persisted, mission still playing.
	g.Advance(5)
	lvl0, ok0 := g.Storage().GetInt(cat, "demo:caldus:level")
	t.Logf("FSV before gate (t=5): level=(%d,%v) result=%v", lvl0, ok0, g.Player(0).Result())
	if ok0 {
		t.Fatalf("hero record persisted before the gate fired: %d", lvl0)
	}

	// AFTER the gate (t>=20): the mission promotes Caldus, grants the Ember Ward,
	// persists his live hero level, and declares victory.
	g.Advance(20)
	lvl, okL := g.Storage().GetInt(cat, "demo:caldus:level")
	emb, okE := g.Storage().GetInt(cat, "demo:caldus:ember")
	res := g.Player(0).Result()
	t.Logf("FSV after gate (t=25): level=(%d,%v) ember=(%d,%v) result=%v", lvl, okL, emb, okE, res)
	if !okL || lvl != 3 {
		t.Fatalf("kindle did not persist hero level 3: got (%d,%v)", lvl, okL)
	}
	if !okE || emb != 1 {
		t.Fatalf("kindle did not persist the Ember Ward flag: got (%d,%v)", emb, okE)
	}
	if res != api.ResultWon {
		t.Fatalf("player 0 result = %v, want Won", res)
	}

	// Now drive the campaign on-complete hook against the SAME store the mission
	// wrote — the carried hero comes from the real playthrough, not a seed.
	def, err := campaign.Load(os.DirFS(demoDir), "demo.toml")
	if err != nil {
		t.Fatalf("load demo.toml: %v", err)
	}
	tr, err := luabind.RunCampaignHook(g.Storage(), def, os.DirFS(demoDir+"/hooks"),
		"demo-playthrough", "kindle", campaign.OutcomeComplete, luabind.CampaignHookOptions{})
	if err != nil {
		t.Fatalf("RunCampaignHook: %v", err)
	}
	t.Logf("FSV carry from real playthrough: next=%s carry=%+v", tr.NextMissionID, tr.CarryOver)
	if tr.NextMissionID != "dawn" {
		t.Fatalf("next mission = %q, want dawn", tr.NextMissionID)
	}
	if len(tr.CarryOver.Heroes) != 1 {
		t.Fatalf("want 1 carried hero, got %d", len(tr.CarryOver.Heroes))
	}
	h := tr.CarryOver.Heroes[0]
	if h.Name != "Ser Caldus" || h.Level != 3 || len(h.Items) != 1 || h.Items[0] != "Ember Ward" {
		t.Fatalf("carried hero wrong: %+v (want Ser Caldus L3 + Ember Ward)", h)
	}
}
