package litd

import (
	"testing"

	"github.com/Light-in-the-Dark-Analytics/light-in-the-dark-game-engine/litd/sim"
)

// #566 — public Group surface. SoT = the Group's observed membership
// (Count/Contains/Each) which resolves through the sim store.

func grpUnit(t *testing.T, w *sim.World, g *Game, player uint8) Unit {
	t.Helper()
	id, ok := w.CreateUnit(vec(Vec2{X: 100, Y: 100}), 0)
	if !ok || !w.Owners.Add(w.Ents, id, player, player, player) {
		t.Fatal("unit setup failed")
	}
	return Unit{id: id, g: g}
}

func TestGroupAPIMembership(t *testing.T) {
	w, g, _ := newDriverGame(t)
	gr := g.NewGroup()
	if !gr.Valid() {
		t.Fatal("NewGroup invalid")
	}
	a, b, c := grpUnit(t, w, g, 0), grpUnit(t, w, g, 0), grpUnit(t, w, g, 0)
	gr.Add(a)
	gr.Add(b)
	gr.Add(c)
	gr.Add(b) // dup → no-op
	if gr.Count() != 3 {
		t.Fatalf("count = %d, want 3", gr.Count())
	}
	if !gr.Contains(b) || gr.First() != a {
		t.Fatal("Contains/First wrong")
	}
	// Each visits in insertion order.
	var seen []Unit
	gr.Each(func(u Unit) { seen = append(seen, u) })
	if len(seen) != 3 || seen[0] != a || seen[2] != c {
		t.Fatalf("Each order wrong: %v", seen)
	}
	gr.Remove(a) // swap-remove → c backfills slot 0
	if gr.Contains(a) || gr.Count() != 2 {
		t.Fatalf("after remove count=%d contains(a)=%v", gr.Count(), gr.Contains(a))
	}
	gr.Clear()
	if gr.Count() != 0 {
		t.Fatalf("after clear count=%d", gr.Count())
	}
	gr.Destroy()
	if gr.Valid() {
		t.Fatal("group valid after Destroy")
	}
	// Stale handle methods are safe no-ops.
	gr.Add(a)
	if gr.Count() != 0 {
		t.Fatal("stale Add mutated")
	}
}

func TestGroupAPIAlgebra(t *testing.T) {
	w, g, _ := newDriverGame(t)
	u := make([]Unit, 5)
	for i := range u {
		u[i] = grpUnit(t, w, g, 0)
	}
	a, b, dst := g.NewGroup(), g.NewGroup(), g.NewGroup()
	for _, i := range []int{0, 1, 2} {
		a.Add(u[i])
	}
	for _, i := range []int{2, 3} {
		b.Add(u[i])
	}
	dst.Union(a, b)
	if dst.Count() != 4 {
		t.Fatalf("union count = %d, want 4", dst.Count())
	}
	dst.Intersect(a, b)
	if dst.Count() != 1 || !dst.Contains(u[2]) {
		t.Fatalf("intersect = %d, want {u2}", dst.Count())
	}
	dst.Difference(a, b)
	if dst.Count() != 2 || dst.Contains(u[2]) {
		t.Fatalf("difference = %d, want {u0,u1}", dst.Count())
	}
	dst.CopyFrom(a)
	if dst.Count() != 3 {
		t.Fatalf("copy count = %d, want 3", dst.Count())
	}
}

func TestGroupAPIFillOwner(t *testing.T) {
	w, g, _ := newDriverGame(t)
	grpUnit(t, w, g, 0)
	grpUnit(t, w, g, 0)
	grpUnit(t, w, g, 1) // other player
	gr := g.NewGroup()
	n := gr.FillOwner(g.Player(0), Query{})
	if n != 2 || gr.Count() != 2 {
		t.Fatalf("owner-0 fill n=%d count=%d, want 2", n, gr.Count())
	}
	// Max caps deterministically.
	n = gr.FillOwner(g.Player(0), Query{Max: 1})
	if n != 1 {
		t.Fatalf("Max=1 fill n=%d, want 1", n)
	}
}

func TestGroupQueryToMask(t *testing.T) {
	_, g, _ := newDriverGame(t)
	q := Query{Enemy: g.Player(2), Structures: TriOnly, Flying: TriExclude, Max: 5}
	m := q.toMask()
	if !m.Enemy || m.OfPlayer != 2 || !m.StructuresOnly || !m.ExcludeFlying || m.Max != 5 {
		t.Fatalf("toMask wrong: %+v", m)
	}
	// Zero query is fully permissive.
	if z := (Query{}).toMask(); z.Enemy || z.Ally || z.StructuresOnly || z.Max != 0 {
		t.Fatalf("zero Query not permissive: %+v", z)
	}
}

