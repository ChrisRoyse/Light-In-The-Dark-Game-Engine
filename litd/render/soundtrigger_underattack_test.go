package render

// End-to-end FSV of the #313 under-attack stinger: real incoming damage in the
// sim raises a NON-HASHING under-attack render cue for the defender, which flows
// through the api render-event accessor → SoundDriver → SoundTrigger → a 3D world
// voice, filtered to the local player. SoT = the routed-cue count + the Manager
// voice table after draining. The sim throttles the cue to once per engagement
// (UnderAttackWarnTicks), re-arming after a damage-free lull, so a sustained
// beating raises one warning, not a per-hit machine-gun — and never floods the
// fixed render-event buffer.

import (
	"testing"
	"testing/fstest"

	api "github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/api"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/audio"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/data"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

const uaClassify = `
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

const uaSets = `
[[unit]]
type="hfoo"
attack="hfoo_atk"
death="hfoo_die"
ready="hfoo_rdy"
ack="hfoo_ack"
order_ack="hfoo_ord"
under_attack="hfoo_warn"
`

// uaDriver builds the full audio pipe (classification + sound-set table + Manager
// + trigger + driver) and a game whose hfoo unit type is combat-capable (an
// attack ⇒ a Combats row ⇒ it is tracked for under-attack), with a 1×1 combat
// matrix so QueueDamage actually lands. localPlayer scopes the owner filter.
func uaDriver(t *testing.T, localPlayer int) (*api.Game, *SoundDriver, *audio.Manager) {
	t.Helper()
	classify, err := audio.LoadSoundTable(fstest.MapFS{"a/s.toml": {Data: []byte(uaClassify)}}, "a/s.toml")
	if err != nil {
		t.Fatal(err)
	}
	st, err := audio.LoadSoundSetTable(fstest.MapFS{"d/s.toml": {Data: []byte(uaSets)}}, "d/s.toml", classify)
	if err != nil {
		t.Fatal(err)
	}
	g, err := api.NewGame(api.GameOptions{MaxUnits: 16, Seed: 3})
	if err != nil {
		t.Fatal(err)
	}
	if err := g.DefineDamageTypes([]string{"normal"}, []string{"unarmored"}); err != nil {
		t.Fatal(err)
	}
	if err := g.DefineCombat([][]int{{1000}}); err != nil {
		t.Fatal(err)
	}
	if err := g.DefineUnits([]data.Unit{{
		ID: "hfoo", Life: 100000, MoveSpeedPerTick: 8 * fixed.One, TurnRatePerTick: 65535, CollisionSize: 16,
		// One attack so the unit gets a Combats row (the under-attack precondition).
		Attacks: []data.Attack{{AttackType: 0, Range: 100 * fixed.One, DamageBase: 1, Dice: 1, Sides: 1, CooldownTicks: 20}},
	}}); err != nil {
		t.Fatal(err)
	}
	mgr := audio.NewManager(nil)
	mgr.SetSoundTable(classify)
	mgr.SetListener(audio.Vec3{})
	// Driver throttle set to the sim window; the sim already rate-limits, so this
	// is the render-side ceiling/defense-in-depth.
	trig := NewSoundTrigger(mgr, st, sim.UnderAttackWarnTicks)
	return g, NewSoundDriver(g, trig, st, localPlayer), mgr
}

// TestSoundDriverUnderAttackEndToEndFSV: an attacker damages a local-player
// defender → exactly one positional world under-attack voice (hfoo_warn) at the
// defender's position; a tick with no incoming damage routes nothing.
func TestSoundDriverUnderAttackEndToEndFSV(t *testing.T) {
	g, drv, mgr := uaDriver(t, 1)

	victim := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 300, Y: 400}, api.Deg(0))
	attacker := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 320, Y: 400}, api.Deg(0))
	if !victim.Valid() || !attacker.Valid() {
		t.Fatal("unit invalid")
	}

	// Quiet tick: no damage queued ⇒ nothing routed.
	g.Advance(1)
	if r := drv.Drain(g); r != 0 {
		t.Fatalf("no-damage drain routed %d, want 0", r)
	}

	// One hit ⇒ one under-attack cue (first hit ever for this defender).
	if !attacker.Damage(victim, 10) {
		t.Fatal("Damage queue failed")
	}
	g.Advance(1)
	if r := drv.Drain(g); r != 1 {
		t.Fatalf("under-attack routed %d, want 1", r)
	}

	s := mgr.Dump()
	if len(s.Voices) != 1 {
		t.Fatalf("voice count = %d, want 1", len(s.Voices))
	}
	v := s.Voices[0]
	if v.Cue != api.CueID("hfoo_warn") {
		t.Fatalf("voice cue = %v, want hfoo_warn", v.Cue)
	}
	if v.Domain != audio.DomainWorld || !v.HasPos {
		t.Fatalf("under-attack voice domain=%d hasPos=%v, want world+positional", v.Domain, v.HasPos)
	}
	if v.Pos.X != 300 || v.Pos.Y != 400 {
		t.Fatalf("under-attack voice at (%.0f,%.0f), want (300,400) — the defender", v.Pos.X, v.Pos.Y)
	}
	t.Logf("FSV under-attack end-to-end: incoming damage → 1 world warn voice (hfoo_warn, positional) at (%.0f,%.0f)", v.Pos.X, v.Pos.Y)
}

// TestSoundDriverUnderAttackThrottleAndRearmFSV: sustained per-tick fire raises
// ONE stinger (not one per hit); after a damage-free lull ≥ UnderAttackWarnTicks
// the defender re-arms and a fresh hit raises a second stinger.
func TestSoundDriverUnderAttackThrottleAndRearmFSV(t *testing.T) {
	g, drv, _ := uaDriver(t, 1)
	victim := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 0, Y: 0}, api.Deg(0))
	attacker := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 20, Y: 0}, api.Deg(0))

	// Engagement 1: damage every tick for a full window. Exactly one stinger.
	routed := 0
	for i := 0; i < int(sim.UnderAttackWarnTicks); i++ {
		attacker.Damage(victim, 5)
		g.Advance(1)
		routed += drv.Drain(g)
	}
	if routed != 1 {
		t.Fatalf("sustained %d-tick beating routed %d stingers, want exactly 1", sim.UnderAttackWarnTicks, routed)
	}
	t.Logf("FSV sustained: %d consecutive hits → 1 stinger (throttled)", sim.UnderAttackWarnTicks)

	// Lull: no damage for the full window so the defender re-arms.
	g.Advance(int(sim.UnderAttackWarnTicks) + 1)
	if r := drv.Drain(g); r != 0 {
		t.Fatalf("lull drain routed %d, want 0", r)
	}

	// Engagement 2: a fresh hit after the lull → a second stinger.
	attacker.Damage(victim, 5)
	g.Advance(1)
	if r := drv.Drain(g); r != 1 {
		t.Fatalf("re-armed hit routed %d, want 1 (defender should warn again after the lull)", r)
	}
	t.Logf("FSV re-arm: after a %d-tick lull a fresh hit → 1 new stinger", sim.UnderAttackWarnTicks)
}

// TestSoundDriverUnderAttackOwnerFilterFSV: you hear YOUR units warn, not the
// enemy's. Damaging a local (P1) and an enemy (P2) unit the same tick routes only
// the local one.
func TestSoundDriverUnderAttackOwnerFilterFSV(t *testing.T) {
	g, drv, mgr := uaDriver(t, 1) // local = P1

	mine := g.CreateUnit(g.Player(1), g.UnitType("hfoo"), api.Vec2{X: 100, Y: 100}, api.Deg(0))
	theirs := g.CreateUnit(g.Player(2), g.UnitType("hfoo"), api.Vec2{X: 500, Y: 500}, api.Deg(0))
	striker := g.CreateUnit(g.Player(3), g.UnitType("hfoo"), api.Vec2{X: 120, Y: 100}, api.Deg(0))

	striker.Damage(mine, 10)
	striker.Damage(theirs, 10)
	g.Advance(1)
	if r := drv.Drain(g); r != 1 {
		t.Fatalf("owner-filtered drain routed %d, want 1 (only the local unit's warning)", r)
	}
	s := mgr.Dump()
	if len(s.Voices) != 1 || s.Voices[0].Pos.X != 100 || s.Voices[0].Pos.Y != 100 {
		t.Fatalf("expected the single voice at the local unit (100,100); got %+v", s.Voices)
	}
	t.Logf("FSV owner filter: P1 + P2 both hit → 1 warn voice, the local P1 unit's (the enemy's is silent)")
}
