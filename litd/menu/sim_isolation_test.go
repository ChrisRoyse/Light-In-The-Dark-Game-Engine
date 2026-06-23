package menu

// #311 determinism-surface guard. The settings menu must touch render/audio/input
// ONLY — never the sim. The import lint enforces it statically (config/menu import
// no litd/sim); this is the dynamic proof: churn EVERY settings command through the
// controller between sim ticks of a live match and the sim StateHash must be byte-
// identical to a control match advanced the same way with no churn. SoT = the
// authoritative sim StateHash (not a return value), read before and after.

import (
	"testing"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/config"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

// noopSink is a RuntimeSink that records nothing and touches nothing — the churn
// must not even need a real sink to prove sim isolation.
type noopSink struct{}

func (noopSink) ApplyGraphics(config.GraphicsPreset) bool { return false }
func (noopSink) ApplyAudio(config.AudioVolumes)           {}
func (noopSink) ApplyLocale(string)                       {}
func (noopSink) ApplyKeymap(string)                       {}

// buildChurnGame builds a small but ACTIVE match (a unit under a move order, so the
// sim state evolves every tick) seeded deterministically.
func buildChurnGame(t *testing.T, seed int64) *api.Game {
	t.Helper()
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: seed})
	if err != nil {
		t.Fatalf("NewGame: %v", err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatalf("DefineUnits: %v", err)
	}
	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 50, Y: 50}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("CreateUnit invalid")
	}
	u.Order(api.OrderMove, api.TargetPoint(api.Vec2{X: 900, Y: 700})) // standing order: walks each tick
	return g
}

func TestSettingsChurnDoesNotPerturbSimHashFSV(t *testing.T) {
	const ticks = 200

	// Control: advance with NO settings activity.
	ctrl := buildChurnGame(t, 7)
	for i := 0; i < ticks; i++ {
		ctrl.Advance(1)
	}
	want := ctrl.StateHash()
	t.Logf("FSV control hash after %d ticks (no churn) = %#x", ticks, want)

	// Churned: identical seed/setup, but route a settings command every tick.
	g := buildChurnGame(t, 7)
	c := NewSettingsController(config.DefaultSettings(), []string{"en", "fr", "xx"}, noopSink{}, nil)
	// Both fresh games must start byte-identical (same seed) before any churn — the
	// baseline that makes the post-churn comparison meaningful.
	if g.StateHash() != buildChurnGame(t, 7).StateHash() {
		t.Fatal("two fresh same-seed games already differ — setup is non-deterministic")
	}

	cmds := []string{
		string(config.ToggleGraphics),
		"settings.audio.master.down",
		"settings.audio.world.up",
		string(config.CycleLocale),
		string(config.ToggleKeymap),
		"settings.audio.ui.down",
		"settings.audio.music.up",
		"settings.audio.ambience.down",
	}
	for i := 0; i < ticks; i++ {
		handled, rerr := c.Route(cmds[i%len(cmds)])
		if rerr != nil || !handled {
			t.Fatalf("tick %d: churn command %q handled=%v err=%v", i, cmds[i%len(cmds)], handled, rerr)
		}
		g.Advance(1)
	}
	got := g.StateHash()
	t.Logf("FSV churned hash after %d ticks (every-tick settings churn) = %#x; final settings=%+v", ticks, got, c.Settings())

	// The churn must have REALLY changed the settings — otherwise the test is vacuous.
	if c.Settings() == config.DefaultSettings() {
		t.Fatal("settings churn left settings at default — test would be vacuous")
	}
	// The verdict: the sim hash is identical with and without the churn.
	if got != want {
		t.Fatalf("settings churn perturbed the sim hash: churned %#x != control %#x", got, want)
	}
}
