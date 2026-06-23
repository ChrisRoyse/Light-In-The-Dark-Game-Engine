package shell

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/asset/worldarchive"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/luabind"
	lua "github.com/yuin/gopher-lua"
)

func TestTriggerAuthoringSaveReloadArchiveFSV(t *testing.T) {
	app := newTestApp(t)
	dir := filepath.Join(t.TempDir(), "world")
	if err := app.NewProject(dir); err != nil {
		t.Fatal(err)
	}
	graph := TriggerGraph{
		Categories: []TriggerCategory{{ID: "arena", Name: "Arena"}},
		Triggers: []TriggerDraft{{
			ID:          "kill_p1_enter",
			Name:        "Kill P1 entering arena",
			Category:    "arena",
			Comment:     "P1 units die when they enter the test region.",
			Enabled:     true,
			InitiallyOn: true,
			Events:      []TriggerEventDraft{arenaEnterEvent()},
			Condition:   ownerCondition(1),
			Actions:     []TriggerActionDraft{{Kind: TriggerActionKillEventUnit}},
		}},
	}
	snap, err := app.SaveTriggerGraph(graph)
	if err != nil {
		t.Fatal(err)
	}
	if !snap.Valid || snap.Summary.Triggers != 1 || snap.Summary.Events != 1 || snap.Summary.Actions != 1 {
		t.Fatalf("trigger snapshot not valid/counts wrong: %+v", snap)
	}
	if err := app.Save(); err != nil {
		t.Fatal(err)
	}
	sourceJSON, err := os.ReadFile(filepath.Join(dir, triggerGraphPath))
	if err != nil {
		t.Fatal(err)
	}
	sourceLua, err := os.ReadFile(filepath.Join(dir, triggerScriptPath))
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"\"id\": \"kill_p1_enter\"", "TriggerRegisterEnterRegion", "TriggerAddCondition", "Unit_Kill(Event_Unit(e))"} {
		if !strings.Contains(string(sourceJSON)+"\n"+string(sourceLua), want) {
			t.Fatalf("saved trigger source missing %q\nJSON:\n%s\nLua:\n%s", want, sourceJSON, sourceLua)
		}
	}

	reopened := newTestApp(t)
	if err := reopened.OpenProject(dir); err != nil {
		t.Fatal(err)
	}
	reloaded := reopened.Snapshot().Triggers
	if !reloaded.Valid || !reflect.DeepEqual(snap.Graph, reloaded.Graph) {
		t.Fatalf("author->save->reload changed graph:\nbefore=%+v\nafter=%+v", snap.Graph, reloaded.Graph)
	}

	archive := filepath.Join(t.TempDir(), "trigger-authoring.litdworld")
	if err := reopened.SaveArchive(archive); err != nil {
		t.Fatal(err)
	}
	opened, err := worldarchive.Open(archive, EditorEngineVersion())
	if err != nil {
		t.Fatal(err)
	}
	archiveLua, luaErr := fs.ReadFile(opened.FS(), triggerScriptPath)
	archiveJSON, jsonErr := fs.ReadFile(opened.FS(), triggerGraphPath)
	manifest := opened.Manifest.Files
	opened.Close()
	if luaErr != nil || jsonErr != nil {
		t.Fatalf("archive missing trigger files: lua=%v json=%v manifest=%v", luaErr, jsonErr, manifest)
	}
	if string(archiveLua) != string(sourceLua) || string(archiveJSON) != string(sourceJSON) {
		t.Fatalf("archive trigger bytes diverged from source form")
	}
	t.Logf("FSV trigger authoring round-trip: JSON=%s\nLua excerpt:\n%s\nArchive=%s files=%v",
		strings.TrimSpace(string(sourceJSON)), triggerExcerpt(string(sourceLua)), archive, sortedManifestKeys(manifest))
}

func TestTriggerLuaRegionKillAndDeterministicHashFSV(t *testing.T) {
	graph := TriggerGraph{Triggers: []TriggerDraft{{
		ID:          "kill_p1_enter",
		Name:        "Kill P1 entering arena",
		Enabled:     true,
		InitiallyOn: true,
		Events:      []TriggerEventDraft{arenaEnterEvent()},
		Condition:   ownerCondition(1),
		Actions:     []TriggerActionDraft{{Kind: TriggerActionKillEventUnit}},
	}}}
	run := func() (p1Alive, p2Alive bool, hash uint64, script string) {
		g, L := triggerLuaGame(t)
		defer L.Close()
		script = compiledTriggerScript(t, graph)
		if err := L.DoString(script); err != nil {
			t.Fatalf("generated trigger script failed: %v\n%s", err, script)
		}
		p1 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
		p2 := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 32, Y: 0}, api.Deg(0))
		g.Advance(1)
		p1Before, p2Before := p1.Alive(), p2.Alive()
		enterArena(g, p1, 150, 150)
		enterArena(g, p2, 160, 160)
		t.Logf("FSV region kill log: P1 alive %v->%v; P2 alive %v->%v", p1Before, p1.Alive(), p2Before, p2.Alive())
		return p1.Alive(), p2.Alive(), g.StateHash(), script
	}
	p1a, p2a, h1, script := run()
	p1b, p2b, h2, _ := run()
	if p1a || !p2a || p1b || !p2b {
		t.Fatalf("owner-gated region trigger wrong: run1 p1=%v p2=%v run2 p1=%v p2=%v", p1a, p2a, p1b, p2b)
	}
	if h1 != h2 {
		t.Fatalf("generated trigger double-run hash diverged: %#x != %#x", h1, h2)
	}
	t.Logf("FSV generated script excerpt:\n%s\nFSV double-run hash: %#x", triggerExcerpt(script), h1)
}

func TestTriggerCompositionAndInitiallyOffFSV(t *testing.T) {
	composed := TriggerGraph{Triggers: []TriggerDraft{{
		ID:          "kill_p1_or_p2",
		Name:        "Kill P1 or P2",
		Enabled:     true,
		InitiallyOn: true,
		Events:      []TriggerEventDraft{arenaEnterEvent()},
		Condition: &TriggerConditionDraft{Kind: TriggerConditionAnd, Children: []TriggerConditionDraft{
			{Kind: TriggerConditionOr, Children: []TriggerConditionDraft{
				{Kind: TriggerConditionOwnerIsPlayer, Player: 1},
				{Kind: TriggerConditionOwnerIsPlayer, Player: 2},
			}},
			{Kind: TriggerConditionNot, Children: []TriggerConditionDraft{
				{Kind: TriggerConditionOwnerIsPlayer, Player: 3},
			}},
		}},
		Actions: []TriggerActionDraft{{Kind: TriggerActionKillEventUnit}},
	}}}
	g, L := triggerLuaGame(t)
	defer L.Close()
	composedScript := compiledTriggerScript(t, composed)
	if err := L.DoString(composedScript); err != nil {
		t.Fatalf("generated composition script failed: %v\n%s", err, composedScript)
	}
	p1 := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	p2 := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 32, Y: 0}, api.Deg(0))
	p3 := g.CreateUnit(g.Player(3), g.UnitType("hfoo"), api.Vec2{X: 64, Y: 0}, api.Deg(0))
	g.Advance(1)
	enterArena(g, p1, 150, 150)
	enterArena(g, p2, 160, 160)
	enterArena(g, p3, 170, 170)
	if p1.Alive() || p2.Alive() || !p3.Alive() {
		t.Fatalf("And/Or/Not condition wrong: p1=%v p2=%v p3=%v", p1.Alive(), p2.Alive(), p3.Alive())
	}
	if !strings.Contains(composedScript, " or ") || !strings.Contains(composedScript, " and ") || !strings.Contains(composedScript, "not (") {
		t.Fatalf("composition script does not visibly encode And/Or/Not:\n%s", composedScript)
	}

	initiallyOff := TriggerGraph{Triggers: []TriggerDraft{
		{
			ID:          "guard",
			Name:        "Initially off P1 guard",
			Enabled:     true,
			InitiallyOn: false,
			Events:      []TriggerEventDraft{arenaEnterEvent()},
			Condition:   ownerCondition(1),
			Actions:     []TriggerActionDraft{{Kind: TriggerActionKillEventUnit}},
		},
		{
			ID:          "arm_guard",
			Name:        "Arm guard for P2",
			Enabled:     true,
			InitiallyOn: true,
			Events:      []TriggerEventDraft{arenaEnterEvent()},
			Condition:   ownerCondition(2),
			Actions:     []TriggerActionDraft{{Kind: TriggerActionEnableTrigger, TargetID: "guard"}},
		},
	}}
	g2, L2 := triggerLuaGame(t)
	defer L2.Close()
	offScript := compiledTriggerScript(t, initiallyOff)
	if !strings.Contains(offScript, "DisableTrigger(litd_trigger_guard)") {
		t.Fatalf("initially-off trigger did not compile to DisableTrigger:\n%s", offScript)
	}
	if err := L2.DoString(offScript); err != nil {
		t.Fatalf("generated initially-off script failed: %v\n%s", err, offScript)
	}
	guarded := g2.CreateUnit(g2.Player(1), g2.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	arming := g2.CreateUnit(g2.Player(2), g2.UnitType("hfoo"), api.Vec2{X: 32, Y: 0}, api.Deg(0))
	g2.Advance(1)
	enterArena(g2, guarded, 150, 150)
	aliveBeforeEnable := guarded.Alive()
	exitArena(g2, guarded)
	enterArena(g2, arming, 160, 160)
	enterArena(g2, guarded, 170, 170)
	if !aliveBeforeEnable || guarded.Alive() || !arming.Alive() {
		t.Fatalf("initially-off/enable edge wrong: beforeEnable=%v guardedNow=%v arming=%v", aliveBeforeEnable, guarded.Alive(), arming.Alive())
	}
	t.Logf("FSV composition script:\n%s\nFSV initially-off script:\n%s", triggerExcerpt(composedScript), triggerExcerpt(offScript))
}

func triggerLuaGame(t *testing.T) (*api.Game, *lua.LState) {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 483})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	L := lua.NewState()
	if err := luabind.Register(L, g); err != nil {
		t.Fatalf("luabind.Register: %v", err)
	}
	return g, L
}

func compiledTriggerScript(t *testing.T, graph TriggerGraph) string {
	t.Helper()
	build, err := buildTriggerGraph(graph)
	if err != nil {
		t.Fatalf("buildTriggerGraph: %v", err)
	}
	return string(build.script)
}

func arenaEnterEvent() TriggerEventDraft {
	return TriggerEventDraft{
		Kind: TriggerEventUnitEntersRegion,
		Region: TriggerRegion{
			MinX: 100,
			MinY: 100,
			MaxX: 200,
			MaxY: 200,
		},
	}
}

func ownerCondition(player int) *TriggerConditionDraft {
	return &TriggerConditionDraft{Kind: TriggerConditionOwnerIsPlayer, Player: player}
}

func enterArena(g *api.Game, u api.Unit, x, y float64) {
	u.SetPosition(api.Vec2{X: x, Y: y})
	g.Advance(2)
}

func exitArena(g *api.Game, u api.Unit) {
	u.SetPosition(api.Vec2{X: 0, Y: 0})
	g.Advance(2)
}

func triggerExcerpt(script string) string {
	lines := strings.Split(strings.TrimSpace(script), "\n")
	if len(lines) > 18 {
		lines = lines[:18]
	}
	return strings.Join(lines, "\n")
}

func sortedManifestKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for key := range m {
		keys = append(keys, key)
	}
	sortStrings(keys)
	return keys
}

func TestTriggerValidationErrorsFSV(t *testing.T) {
	app := newTestApp(t)
	if err := app.NewProject(filepath.Join(t.TempDir(), "world")); err != nil {
		t.Fatal(err)
	}
	_, err := app.SaveTriggerGraph(TriggerGraph{Triggers: []TriggerDraft{{
		ID:          "bad",
		Name:        "Bad",
		Enabled:     true,
		InitiallyOn: true,
		Events:      []TriggerEventDraft{{Kind: TriggerEventUnitEntersRegion}},
		Actions:     []TriggerActionDraft{{Kind: TriggerActionEnableTrigger, TargetID: "missing"}},
	}}})
	if err == nil {
		t.Fatal("invalid trigger graph should fail closed")
	}
	if !strings.Contains(err.Error(), "region rect") || !strings.Contains(err.Error(), "target \"missing\" not found") {
		t.Fatalf("validation error did not expose edge causes: %v", err)
	}
	t.Logf("FSV trigger validation edge: %v", err)
}

func ExampleTriggerGraph_generatedLua() {
	build, _ := buildTriggerGraph(TriggerGraph{Triggers: []TriggerDraft{{
		ID:          "kill_p1_enter",
		Name:        "Kill P1 entering arena",
		Enabled:     true,
		InitiallyOn: true,
		Events:      []TriggerEventDraft{arenaEnterEvent()},
		Condition:   ownerCondition(1),
		Actions:     []TriggerActionDraft{{Kind: TriggerActionKillEventUnit}},
	}}})
	for _, line := range strings.Split(string(build.script), "\n") {
		if strings.Contains(line, "TriggerRegisterEnterRegion") || strings.Contains(line, "Unit_Kill") {
			fmt.Println(strings.TrimSpace(line))
		}
	}
	// Output:
	// TriggerRegisterEnterRegion(litd_trigger_kill_p1_enter, litd_trigger_kill_p1_enter_region_1)
	// Unit_Kill(Event_Unit(e))
}
