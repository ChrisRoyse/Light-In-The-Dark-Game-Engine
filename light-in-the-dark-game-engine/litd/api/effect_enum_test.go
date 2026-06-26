package litd

// #529 effect enumeration FSV. SoT = the set of Effect handles Game.Effects()
// returns and each handle's Position(), cross-checked against the known spawn
// coordinates. Spawn 3 at known points → enumerate 3 with matching positions;
// Destroy 1 → enumerate 2 with the destroyed one gone and survivors intact. Plus
// a reused-backing AppendEffects allocation check.

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

func TestEffectsEnumerationFSV(t *testing.T) {
	w := sim.NewWorld(effectAPITestCaps())
	g := newGame(w)
	g.RegisterEffectModel("fx/a", 11)
	g.RegisterEffectModel("fx/b", 12)
	g.RegisterEffectModel("fx/c", 13)

	// Empty world enumerates to nothing (non-nil).
	if got := g.Effects(); got == nil || len(got) != 0 {
		t.Fatalf("Effects() on empty world = %v, want non-nil empty", got)
	}

	type spec struct {
		model string
		pos   Vec2
	}
	specs := []spec{
		{"fx/a", Vec2{X: 10, Y: 20}},
		{"fx/b", Vec2{X: -30, Y: 40}},
		{"fx/c", Vec2{X: 55.5, Y: -5.25}},
	}
	handles := make([]Effect, 0, len(specs))
	for _, s := range specs {
		e := g.AddSpecialEffect(s.model, s.pos)
		if !e.Valid() {
			t.Fatalf("spawn %s failed", s.model)
		}
		handles = append(handles, e)
	}

	got := g.Effects()
	t.Logf("FSV enumerate: %d effects (want 3)", len(got))
	for i, e := range got {
		p := e.Position()
		t.Logf("  effect[%d] handle=%#x pos=(%.3f,%.3f)", i, uint32(e.id), p.X, p.Y)
	}
	if len(got) != 3 {
		t.Fatalf("Effects() = %d, want 3", len(got))
	}
	// Creation order before any removal: each enumerated effect's position matches
	// the spawn coordinate exactly (SoT cross-check).
	for i, s := range specs {
		if p := got[i].Position(); p != s.pos {
			t.Fatalf("effect[%d] enumerated pos %+v != spawned %+v", i, p, s.pos)
		}
	}

	// Destroy the middle effect → enumeration drops to 2, the destroyed handle is
	// absent, survivors keep their positions (order may change under swap-remove).
	handles[1].Destroy()
	got2 := g.Effects()
	t.Logf("FSV after Destroy(fx/b): %d effects (want 2)", len(got2))
	if len(got2) != 2 {
		t.Fatalf("Effects() after destroy = %d, want 2", len(got2))
	}
	for _, e := range got2 {
		if e.id == handles[1].id {
			t.Fatal("destroyed effect still enumerated")
		}
	}
	survivors := map[Vec2]bool{specs[0].pos: false, specs[2].pos: false}
	for _, e := range got2 {
		p := e.Position()
		if _, ok := survivors[p]; !ok {
			t.Fatalf("unexpected survivor position %+v", p)
		}
		survivors[p] = true
	}
	for pos, seen := range survivors {
		if !seen {
			t.Fatalf("survivor at %+v missing from enumeration", pos)
		}
	}
}

func TestAppendEffectsReuseFSV(t *testing.T) {
	w := sim.NewWorld(effectAPITestCaps())
	g := newGame(w)
	g.RegisterEffectModel("fx/a", 11)
	for i := 0; i < 4; i++ {
		if e := g.AddSpecialEffect("fx/a", Vec2{X: float64(i), Y: 0}); !e.Valid() {
			t.Fatalf("spawn %d failed", i)
		}
	}
	dst := make([]Effect, 0, 8)
	allocs := testing.AllocsPerRun(100, func() {
		dst = g.AppendEffects(dst[:0])
	})
	t.Logf("FSV AppendEffects reuse: len=%d allocs/op=%.2f", len(dst), allocs)
	if len(dst) != 4 {
		t.Fatalf("AppendEffects len = %d, want 4", len(dst))
	}
	if allocs != 0 {
		t.Fatalf("AppendEffects into a reused slice allocated %.2f/op, want 0", allocs)
	}
}
