package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/fixed"
	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// unitAt creates a unit with health + owner at a world position.
func unitAt(t *testing.T, w *sim.World, g *Game, player uint8, x, y float64) (Unit, sim.EntityID) {
	t.Helper()
	var face fixed.Angle
	id, ok := w.CreateUnit(fixed.Vec2{X: fromFloat(x), Y: fromFloat(y)}, face)
	if !ok {
		t.Fatal("CreateUnit failed")
	}
	if !w.Healths.Add(w.Ents, id, fromFloat(200), 0, 0, 0) {
		t.Fatal("Healths.Add failed")
	}
	if !w.Owners.Add(w.Ents, id, player, player, 0) {
		t.Fatal("Owners.Add failed")
	}
	return Unit{id: id, g: g}, id
}

// stepUntil steps the world up to maxTicks, returning the tick a
// predicate first held, or -1.
func stepUntil(w *sim.World, maxTicks int, pred func() bool) int {
	for i := 0; i < maxTicks; i++ {
		w.Step()
		if pred() {
			return i + 1
		}
	}
	return -1
}

// TestMissileHomingImpact is the happy path: a homing missile reaches
// its target, fires EventMissileImpact with the struck unit, and the
// handle goes invalid after impact.
func TestMissileHomingImpact(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 64, Projectiles: 8})
	g := newGame(w)
	launcher, lID := unitAt(t, w, g, 0, 0, 0)
	target, tID := unitAt(t, w, g, 1, 200, 0)

	var impactMissile Missile
	var struck Unit
	var impactPos Vec2
	fired := 0
	g.OnEvent(EventMissileImpact, func(e Event) {
		fired++
		impactMissile = e.Missile()
		struck = e.Unit()
		impactPos = e.Missile().Position()
	})

	m := g.SpawnMissile(MissileOptions{
		Source: launcher, Origin: Vec2{X: 0, Y: 0},
		Guidance: GuidanceHoming, Target: target,
		Speed: 300, Damage: 50,
	})
	t.Logf("spawned: Valid=%v Target==T:%v Source==L:%v Owner.idx valid:%v",
		m.Valid(), m.Target().id == tID, m.Source().id == lID, m.Owner().Valid())
	if !m.Valid() || m.Target().id != tID || m.Source().id != lID {
		t.Fatalf("spawn wrong: Valid=%v target=%#x(want %#x) source=%#x(want %#x)",
			m.Valid(), uint32(m.Target().id), uint32(tID), uint32(m.Source().id), uint32(lID))
	}

	w.Step() // speed 300 > distance 200 → impact this tick
	t.Logf("after step: impacts=%d struck==T:%v missileFromEvent==spawn:%v impactPos=%v handleValidAfter=%v",
		fired, struck.id == tID, impactMissile.id == m.id, impactPos, m.Valid())

	if fired != 1 {
		t.Fatalf("EventMissileImpact fired %d times, want 1", fired)
	}
	if struck.id != tID {
		t.Fatalf("struck unit %#x, want target %#x", uint32(struck.id), uint32(tID))
	}
	if impactMissile.id != m.id {
		t.Fatalf("event missile %#x != spawned %#x", uint32(impactMissile.id), uint32(m.id))
	}
	// edge (3): handle held after impact is invalid; verbs no-op
	if m.Valid() {
		t.Fatal("missile handle still Valid() after impact")
	}
	if !m.Position().IsZero() || !m.Target().IsZero() || !m.Source().IsZero() {
		t.Fatal("verbs on a post-impact handle did not degrade to zero")
	}
	_ = target
}

// TestMissileSetTarget — edge (1): retarget mid-flight; delivery lands
// at the NEW target, not the original.
func TestMissileSetTarget(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 64, Projectiles: 8})
	g := newGame(w)
	launcher, _ := unitAt(t, w, g, 0, 0, 0)
	targetA, aID := unitAt(t, w, g, 1, 500, 0) // original heading
	targetB, bID := unitAt(t, w, g, 1, 0, 500) // redirected heading
	_ = targetA

	var struck sim.EntityID
	g.OnEvent(EventMissileImpact, func(e Event) { struck = e.Unit().id })

	m := g.SpawnMissile(MissileOptions{
		Source: launcher, Origin: Vec2{},
		Guidance: GuidanceHoming, Target: targetA,
		Speed: 100, Damage: 10,
	})
	w.Step()
	w.Step() // missile now ~ (200,0), heading toward A
	p0 := m.Position()
	m.SetTarget(targetB)
	t.Logf("at %v retargeted A(500,0)->B(0,500); new Target==B:%v", p0, m.Target().id == bID)
	if m.Target().id != bID {
		t.Fatalf("SetTarget did not switch guide to B: got %#x", uint32(m.Target().id))
	}

	tick := stepUntil(w, 20, func() bool { return struck != 0 })
	t.Logf("impact at tick offset %d, struck==B:%v (aID=%#x bID=%#x struck=%#x)",
		tick, struck == bID, uint32(aID), uint32(bID), uint32(struck))
	if struck != bID {
		t.Fatalf("redirected missile struck %#x, want B %#x", uint32(struck), uint32(bID))
	}
}

// TestMissileExpire — edge (2): Expire() removes the missile with no
// payload; EventMissileExpired fires and EventMissileImpact does not,
// leaving the would-be target's life untouched.
func TestMissileExpire(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 64, Projectiles: 8})
	g := newGame(w)
	launcher, _ := unitAt(t, w, g, 0, 0, 0)
	target, tID := unitAt(t, w, g, 1, 5000, 0) // far: no impact before we expire

	impacts, expires := 0, 0
	g.OnEvent(EventMissileImpact, func(Event) { impacts++ })
	g.OnEvent(EventMissileExpired, func(Event) { expires++ })

	m := g.SpawnMissile(MissileOptions{
		Source: launcher, Origin: Vec2{},
		Guidance: GuidanceHoming, Target: target,
		Speed: 50, Damage: 80,
	})
	lifeBefore, _ := rawLife(w, tID)
	// expire in phase 5 of tick 1 — after the missile's harmless phase-4
	// flight, before any impact could occur at this distance
	w.OnCombatPhase = func(tick uint32) {
		if tick == 1 {
			m.Expire()
		}
	}
	w.Step()
	lifeAfter, _ := rawLife(w, tID)
	t.Logf("BEFORE life=%v; AFTER expire: impacts=%d expires=%d targetLife=%v handleValid=%v",
		lifeBefore, impacts, expires, lifeAfter, m.Valid())
	if expires != 1 || impacts != 0 {
		t.Fatalf("expire wrong: impacts=%d expires=%d, want 0/1", impacts, expires)
	}
	if lifeAfter != lifeBefore {
		t.Fatalf("expired missile delivered payload: target life %v→%v", lifeBefore, lifeAfter)
	}
}

// TestMissileUnknownGuidance — edge (4): an unknown guidance fails
// deterministically with an invalid handle + a debug report.
func TestMissileUnknownGuidance(t *testing.T) {
	w := sim.NewWorld(sim.Caps{Units: 64, Projectiles: 8})
	g := newGame(w)
	launcher, _ := unitAt(t, w, g, 0, 0, 0)
	var reports []string
	g.OnInvalidHandle(func(s string) { reports = append(reports, s) })
	g.SetDebug(true)

	m := g.SpawnMissile(MissileOptions{
		Source: launcher, Origin: Vec2{},
		Guidance: Guidance(99), Speed: 100,
	})
	t.Logf("unknown guidance: Valid=%v reports=%v", m.Valid(), reports)
	if m.Valid() {
		t.Fatal("unknown guidance produced a valid missile")
	}
	if len(reports) != 1 || !containsStr(reports[0], "unknown guidance") {
		t.Fatalf("expected unknown-guidance report, got %v", reports)
	}
}

// TestMissilePoolExhaustion — edge (5): when the projectile pool is
// full, SpawnMissile returns an invalid handle, deterministically across
// runs.
func TestMissilePoolExhaustion(t *testing.T) {
	run := func() (bool, bool, bool) {
		w := sim.NewWorld(sim.Caps{Units: 64, Projectiles: 2})
		g := newGame(w)
		launcher, _ := unitAt(t, w, g, 0, 0, 0)
		opt := MissileOptions{Source: launcher, Origin: Vec2{},
			Guidance: GuidancePoint, Point: Vec2{X: 9000, Y: 0}, Speed: 10, Damage: 5}
		a := g.SpawnMissile(opt)
		b := g.SpawnMissile(opt)
		c := g.SpawnMissile(opt) // pool full (cap 2): must fail
		return a.Valid(), b.Valid(), c.Valid()
	}
	a1, b1, c1 := run()
	a2, b2, c2 := run()
	t.Logf("run1: valid=%v,%v,%v  run2: valid=%v,%v,%v", a1, b1, c1, a2, b2, c2)
	if !a1 || !b1 || c1 {
		t.Fatalf("run1 wrong: want true,true,false got %v,%v,%v", a1, b1, c1)
	}
	if a1 != a2 || b1 != b2 || c1 != c2 {
		t.Fatalf("pool exhaustion nondeterministic across runs")
	}
}

func containsStr(s, sub string) bool {
	return len(s) >= len(sub) && (indexOf(s, sub) >= 0)
}

func indexOf(s, sub string) int {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}
