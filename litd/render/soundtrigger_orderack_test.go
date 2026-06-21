package render

// End-to-end FSV of the #313 order-ack category + its LOCAL-PLAYER filter: an
// explicit order to one of the local player's units plays an order-ack voice; the
// same order to an ENEMY unit is silent (you hear only your own units acknowledge).
// The sim is player-agnostic, so the filter lives render-side (SoundDriver). SoT =
// the audio Manager voice table after draining.

import (
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func TestSoundDriverOrderAckOwnerFilterFSV(t *testing.T) {
	const cls = `
[[sound]]
cue="hfoo_atk"
domain="world"
priority="attackimpact"
ogg="a.ogg"
[[sound]]
cue="hfoo_die"
domain="world"
priority="death"
ogg="d.ogg"
[[sound]]
cue="hfoo_rdy"
domain="ui"
priority="ambient"
ogg="r.ogg"
[[sound]]
cue="hfoo_ack"
domain="ui"
priority="ambient"
ogg="k.ogg"
[[sound]]
cue="hfoo_ord"
domain="ui"
priority="ambient"
ogg="o.ogg"
[[sound]]
cue="hfoo_warn"
domain="world"
priority="alert"
ogg="w.ogg"
`
	const sets = `
[[unit]]
type="hfoo"
attack="hfoo_atk"
death="hfoo_die"
ready="hfoo_rdy"
ack="hfoo_ack"
order_ack="hfoo_ord"
under_attack="hfoo_warn"
`
	classify, err := audio.LoadSoundTable(fstest.MapFS{"a/s.toml": {Data: []byte(cls)}}, "a/s.toml")
	if err != nil {
		t.Fatal(err)
	}
	st, err := audio.LoadSoundSetTable(fstest.MapFS{"d/s.toml": {Data: []byte(sets)}}, "d/s.toml", classify)
	if err != nil {
		t.Fatal(err)
	}
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 9})
	if err != nil {
		t.Fatal(err)
	}
	if err := g.DefineUnits([]data.Unit{
		{ID: "hfoo", Life: 100, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16},
	}); err != nil {
		t.Fatal(err)
	}
	mgr := audio.NewManager(nil)
	mgr.SetSoundTable(classify)
	mgr.SetListener(audio.Vec3{})
	trig := NewSoundTrigger(mgr, st, 0)
	const localPlayer = 1
	drv := NewSoundDriver(g, trig, st, localPlayer)

	mine := g.CreateUnit(g.Player(localPlayer), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	enemy := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 500, Y: 500}, api.Deg(0))
	if !mine.Valid() || !enemy.Valid() {
		t.Fatal("unit invalid")
	}

	// Issue an explicit move order to BOTH units (same category, different owners).
	if !mine.Order(g.Order("move"), api.TargetPoint(api.Vec2{X: 200, Y: 100})) {
		t.Fatal("order to own unit failed")
	}
	if !enemy.Order(g.Order("move"), api.TargetPoint(api.Vec2{X: 400, Y: 500})) {
		t.Fatal("order to enemy unit failed")
	}
	g.Advance(1)
	routed := drv.Drain(g)

	// Only the local player's order-ack plays; the enemy's is filtered.
	if routed != 1 {
		t.Fatalf("order-ack routed %d, want exactly 1 (enemy's must be filtered)", routed)
	}
	s := mgr.Dump()
	if len(s.Voices) != 1 {
		t.Fatalf("voice count = %d, want 1", len(s.Voices))
	}
	v := s.Voices[0]
	if v.Cue != api.CueID("hfoo_ord") {
		t.Fatalf("voice cue = %d, want hfoo_ord (%d)", v.Cue, api.CueID("hfoo_ord"))
	}
	if v.Domain != audio.DomainUI || v.HasPos {
		t.Fatalf("order-ack voice domain=%d hasPos=%v, want UI+flat", v.Domain, v.HasPos)
	}
	t.Logf("FSV #313 order-ack + owner filter: explicit order to both P%d (local) and P2 (enemy) → exactly 1 UI order-ack voice (the local one); enemy's filtered", localPlayer)
}
