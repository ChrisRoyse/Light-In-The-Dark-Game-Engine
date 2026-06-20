package render

// End-to-end FSV of the #313 pipe: a real sim unit death flows through the
// non-hashing render-event channel → the api render-event accessor → the
// SoundDriver → the SoundTrigger → an admitted voice in the audio Manager. SoT =
// the Manager voice table after draining. X+X=Y: kill one hfoo at a known place →
// exactly one positional world death voice carrying hfoo's death cue.

import (
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
)

func TestSoundDriverDeathEndToEndFSV(t *testing.T) {
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

	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 3})
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
	mgr.SetListener(audio.Vec3{}) // listener at origin
	trig := NewSoundTrigger(mgr, st, 0)
	drv := NewSoundDriver(g, trig, st)

	u := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 300, Y: 400}, api.Deg(0))
	if !u.Valid() {
		t.Fatal("unit invalid")
	}
	// Baseline: a tick with no death routes nothing.
	g.Advance(1)
	if r := drv.Drain(g); r != 0 {
		t.Fatalf("pre-death drain routed %d, want 0", r)
	}

	u.Kill()
	routed := 0
	for i := 0; i < 5; i++ { // drain each tick until the death cue surfaces
		g.Advance(1)
		routed += drv.Drain(g)
	}
	if routed != 1 {
		t.Fatalf("death routed %d times, want exactly 1", routed)
	}

	// SoT: the Manager holds one positional world voice carrying hfoo's death cue —
	// proving type→code→cue resolution end to end through the real sim death.
	s := mgr.Dump()
	if len(s.Voices) != 1 {
		t.Fatalf("voice count = %d, want 1", len(s.Voices))
	}
	v := s.Voices[0]
	if v.Cue != api.CueID("hfoo_die") {
		t.Fatalf("voice cue = %d, want hfoo_die (%d) — type resolution broke", v.Cue, api.CueID("hfoo_die"))
	}
	if v.Domain != audio.DomainWorld || !v.HasPos {
		t.Fatalf("death voice domain=%d hasPos=%v, want world+positional", v.Domain, v.HasPos)
	}
	t.Logf("FSV #313 end-to-end: hfoo death → render cue → driver → 1 world death voice (cue hfoo_die, positional) at pos (%.0f,%.0f)", v.Pos.X, v.Pos.Y)
}
